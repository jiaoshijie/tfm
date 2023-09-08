use crossbeam_channel::{unbounded, Receiver, Sender};
use crossterm::event::KeyCode;
use crossterm::{event::Event, terminal};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
use unicode_width::UnicodeWidthChar;
use walkdir::{DirEntry, WalkDir};

use crate::config::WORD_SEPS;
use crate::file::File;

macro_rules! nth {
    ($val:expr, $index:expr) => {
        $val.chars().nth($index).unwrap()
    };
}

macro_rules! parse2usize {
    ($slice:expr) => {
        $slice
            .parse::<usize>()
            .map_err(|err| {
                log::error!("{err} => Parse str `{str}` to usize failed!", str = $slice);
            })
            .unwrap()
    };
}

macro_rules! impl_chan {
    ($name:ty, $send_mem:ident, $recv_mem:ident, $t:ty) => {
        impl $name {
            pub fn new() -> Self {
                let ($send_mem, $recv_mem) = unbounded::<$t>();
                Self {
                    $send_mem,
                    $recv_mem,
                }
            }
        }
    };
}

pub struct UnitChan {
    pub unit_recv: Receiver<()>,
    pub unit_send: Sender<()>,
}
impl_chan!(UnitChan, unit_send, unit_recv, ());

pub struct BoolChan {
    pub bool_recv: Receiver<bool>,
    pub bool_send: Sender<bool>,
}
impl_chan!(BoolChan, bool_send, bool_recv, bool);

pub struct CrossTermEventChan {
    pub ev_recv: Receiver<Event>,
    pub ev_send: Sender<Event>,
}
impl_chan!(CrossTermEventChan, ev_send, ev_recv, Event);

pub struct U64Chan {
    pub u64_recv: Receiver<u64>,
    pub u64_send: Sender<u64>,
}
impl_chan!(U64Chan, u64_send, u64_recv, u64);

pub struct StringChan {
    pub string_recv: Receiver<String>,
    pub string_send: Sender<String>,
}
impl_chan!(StringChan, string_send, string_recv, String);

pub fn is_hidden(f: &File) -> bool {
    f.file_name.starts_with('.') && f.file_name != "."
}

// Natural sort order: https://en.wikipedia.org/wiki/Natural_sort_order
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b) = (a.to_ascii_lowercase(), b.to_ascii_lowercase());
    let (mut a_nth, mut b_nth, mut a_high, mut b_high) = (0usize, 0usize, 0usize, 0usize);
    loop {
        if a_high >= a.len() {
            return b_high.cmp(&b.len()); // Less or Equal
        }
        if b_high >= b.len() {
            return std::cmp::Ordering::Greater;
        }

        // NOTE: `unwrap` operation in macro `nth!` should not fail.
        let a_is_digit = nth!(a, a_nth).is_ascii_digit();
        let a_low = a_high;
        while a_high < a.len() && a_is_digit == nth!(a, a_nth).is_ascii_digit() {
            a_high += nth!(a, a_nth).len_utf8();
            a_nth += 1;
        }
        let b_is_digit = nth!(b, b_nth).is_ascii_digit();
        let b_low = b_high;
        while b_high < b.len() && b_is_digit == nth!(b, b_nth).is_ascii_digit() {
            b_high += nth!(b, b_nth).len_utf8();
            b_nth += 1;
        }

        let a_slice = &a.as_str()[a_low..a_high];
        let b_slice = &b.as_str()[b_low..b_high];

        if a_slice == b_slice {
            continue;
        }

        if a_is_digit && b_is_digit {
            return parse2usize!(a_slice).cmp(&parse2usize!(b_slice));
        }

        return a_slice.cmp(b_slice);
    }
}

pub fn terminal_size() -> (u16, u16) {
    terminal::size()
        .map_err(|err| {
            log::error!("{err} => Get terminal size failed");
        })
        .unwrap()
}

pub fn pause() {
    let mut stdout = std::io::stdout();
    stdout
        .write_all(b"Press Enter to continue...")
        .map_err(|err| {
            log::error!("{err} => stdout.write_all failed");
        })
        .unwrap();
    stdout
        .flush()
        .map_err(|err| {
            log::error!("{err} => stdout.flush failed");
        })
        .unwrap();
    std::io::stdin()
        .read_exact(&mut [0])
        .map_err(|err| {
            log::error!("{err} => stdin.read_exact failed");
        })
        .unwrap();
}

pub fn shrink_unicode_str(s: &str, width: usize) -> &str {
    let (mut curr, mut end) = (0usize, 0usize);
    for c in s.chars() {
        let w = c.width().unwrap_or_default();
        curr += w;
        if curr > width {
            break;
        }
        end += c.len_utf8();
    }
    &s[0..end]
}

pub fn shrink_unicode_str_rev(s: &str, width: usize) -> &str {
    let (mut curr, mut head) = (0usize, s.len());
    for c in s.chars().rev() {
        let w = c.width().unwrap_or_default();
        curr += w;
        if curr > width {
            break;
        }
        head -= c.len_utf8();
    }
    &s[head..]
}

pub fn keycode2str(code: KeyCode) -> String {
    let mut ret = String::new();
    match code {
        KeyCode::Char(ch) => ret.push(ch),
        KeyCode::F(num) => ret += &format!("F{num}"),
        KeyCode::Esc => ret += "Esc",
        KeyCode::Backspace => ret += "Backspace",
        KeyCode::Enter => ret += "Enter",
        KeyCode::Left => ret += "Left",
        KeyCode::Right => ret += "Right",
        KeyCode::Up => ret += "Up",
        KeyCode::Down => ret += "Down",
        KeyCode::Home => ret += "Home",
        KeyCode::End => ret += "End",
        KeyCode::PageUp => ret += "PageUp",
        KeyCode::PageDown => ret += "PageDown",
        KeyCode::Tab | KeyCode::BackTab => ret += "Tab",
        KeyCode::Delete => ret += "Delete",
        KeyCode::Insert => ret += "Insert",
        _ => log::warn!("Reach unhanlded keycode => {:?}", code),
    }
    ret
}

pub fn dir_size(p: &Path) -> Result<u64, String> {
    let mut ret = 0u64;
    if p.is_symlink() {
        return Ok(0);
    }
    for entry in WalkDir::new(p) {
        match entry {
            Ok(entry) => {
                if !entry.path_is_symlink() {
                    match entry.metadata() {
                        Ok(metadata) => {
                            ret += metadata.len();
                        }
                        Err(err) => {
                            let io_err = if let Some(io_err) = err.io_error() {
                                format!("{io_err}")
                            } else {
                                "Unexpected error occurred".to_string()
                            };
                            let err_msg = format!(
                                "{io_err} => copy_size walkdir entry.metadata `{path}` failed",
                                path = entry.path().display()
                            );
                            log::error!("{err_msg}");
                            return Err(err_msg);
                        }
                    }
                }
            }
            Err(err) => {
                let io_err = if let Some(io_err) = err.io_error() {
                    format!("{io_err}")
                } else {
                    "Unexpected error occurred".to_string()
                };
                let path = if let Some(path) = err.path() {
                    format!("{path}", path = path.display())
                } else {
                    "path = None".to_string()
                };
                let err_msg = format!("{io_err} => copy_size walkdir `{path}` failed");
                log::error!("{err_msg}");
                return Err(err_msg);
            }
        }
    }
    Ok(ret)
}

pub fn dir_copy(entry: &DirEntry, dst_dir: &Path, base_dir: &Path) -> std::io::Result<u64> {
    let from = entry.path();
    let suffix_path = format!("{}", from.strip_prefix(base_dir).unwrap().display());
    let to = if suffix_path.is_empty() {
        dst_dir.to_owned()
    } else {
        dst_dir.join(&suffix_path)
    };
    if let Some(parent) = to.parent() {
        if !parent.exists() || !parent.is_dir() {
            if let Err(err) = fs::create_dir_all(parent) {
                log::error!(
                    "{err} => fs::create_dir_all `{path}` failed",
                    path = parent.display()
                );
            }
        }
    }
    if from.is_symlink() {
        let link_from = fs::read_link(from)?;
        std::os::unix::fs::symlink(link_from, to)?;
    } else if from.is_dir() {
        fs::create_dir_all(to)?;
    } else {
        return fs::copy(from, to);
    }
    Ok(0)
}

pub fn dir_remove(p: &Path) -> std::io::Result<()> {
    if p.exists() {
        if p.is_dir() {
            fs::remove_dir_all(p)?
        } else {
            fs::remove_file(p)?
        }
    }
    Ok(())
}

pub fn find_word(s: &str, ind: usize) -> (usize, usize) {
    let (old, mut new) = (ind, ind);
    if !s.is_empty() && new <= s.len() && new > 0 {
        let mut ch = s[..new].chars().last().unwrap();
        while new > 0 && (ch.is_ascii_whitespace() || WORD_SEPS.contains(&ch)) {
            new -= ch.len_utf8();
            ch = if let Some(ch) = s[..new].chars().last() {
                ch
            } else {
                break;
            }
        }
        while new > 0 && !ch.is_ascii_whitespace() && !WORD_SEPS.contains(&ch) {
            new -= ch.len_utf8();
            ch = if let Some(ch) = s[..new].chars().last() {
                ch
            } else {
                break;
            }
        }
    }
    (old, new)
}

pub fn find_word_rev(s: &str, ind: usize) -> (usize, usize) {
    let (old, mut new) = (ind, ind);
    if !s.is_empty() && new < s.len() {
        let mut ch = s[new..].chars().next().unwrap();
        while new < s.len() && (ch.is_ascii_whitespace() || WORD_SEPS.contains(&ch)) {
            new += ch.len_utf8();
            ch = if let Some(ch) = s[new..].chars().next() {
                ch
            } else {
                break;
            }
        }
        while new < s.len() && !ch.is_ascii_whitespace() && !WORD_SEPS.contains(&ch) {
            new += ch.len_utf8();
            ch = if let Some(ch) = s[new..].chars().next() {
                ch
            } else {
                break;
            }
        }
    }
    (old, new)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_natural_cmp() {
        use std::cmp::Ordering::*;
        assert_eq!(Less, natural_cmp("a1", "a2"));
        assert_eq!(Greater, natural_cmp("a2", "a1"));
        assert_eq!(Greater, natural_cmp("123a", "122"));
        assert_eq!(Less, natural_cmp("1a2", "1b1"));
        assert_eq!(Equal, natural_cmp("1a1", "1a1"));
        assert_eq!(Greater, natural_cmp("1a1a", "1a1"));
        assert_eq!(Less, natural_cmp("2", "10"));
        assert_eq!(Less, natural_cmp("foo2bar", "foo10bar"));
        assert_eq!(Greater, natural_cmp("foo2bar", "bar10bar"));
    }

    #[test]
    fn test_shrink_unicode_str() {
        assert_eq!("站着说话", shrink_unicode_str("站着说话不腰疼", 8));
        assert_eq!("abcd站着说", shrink_unicode_str("abcd站着说话不腰疼", 11));
        assert_eq!("abcd🤣站着", shrink_unicode_str("abcd🤣站着说话不腰疼", 11));
        assert_eq!("abc", shrink_unicode_str("abcd🤣站着说话不腰疼", 3));
    }

    #[test]
    fn test_shrink_unicode_str_rev() {
        assert_eq!("话不腰疼", shrink_unicode_str_rev("站着说话不腰疼", 8));
        assert_eq!(
            "不腰疼abcd",
            shrink_unicode_str_rev("站着说话不腰疼abcd", 11)
        );
        assert_eq!(
            "腰疼🤣abcd",
            shrink_unicode_str_rev("站着说话不腰疼🤣abcd", 11)
        );
        assert_eq!("bcd", shrink_unicode_str_rev("站着说话不腰疼🤣abcd", 3));
    }
}
