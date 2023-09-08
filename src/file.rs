use std::{
    fs::{read_link, Metadata},
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

use chrono::{prelude::DateTime, Local};

#[derive(PartialEq, Eq, Debug)]
pub enum LinkState {
    Working(String),
    Broken(String),
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub enum FileType {
    Directory,
    RegularFile,
    Link,
    Pipe,
    Socket,
    CharDevice,
    BlockDevice,
    Special,
}

impl FileType {
    pub fn get_file_type(p: &Metadata) -> Self {
        let ft = p.file_type();
        if ft.is_symlink() {
            Self::Link
        } else if ft.is_dir() {
            Self::Directory
        } else if ft.is_file() {
            Self::RegularFile
        } else if ft.is_fifo() {
            Self::Pipe
        } else if ft.is_socket() {
            Self::Socket
        } else if ft.is_char_device() {
            Self::CharDevice
        } else if ft.is_block_device() {
            Self::BlockDevice
        } else {
            Self::Special
        }
    }

    pub fn symbol(&self) -> char {
        match self {
            Self::Directory => 'd',
            Self::RegularFile => '-',
            Self::Link => 'l',
            Self::Pipe => 'p',
            Self::Socket => 's',
            Self::CharDevice => 'c',
            Self::BlockDevice => 'b',
            Self::Special => '-',
        }
    }
}

#[derive(Debug)]
pub struct File {
    pub file_name: String,
    pub file_path: PathBuf,
    pub file_type: FileType,
    pub size: u64,
    pub mtime: i64,
    pub link_state: Option<LinkState>,
    uid: u32,
    gid: u32,
    mode: u32,
}

impl File {
    pub fn new(p: &Path) -> Option<Self> {
        let symlink_metadata = match p.symlink_metadata() {
            Ok(m) => m,
            Err(err) => {
                log::error!(
                    "{err} => Unable to get file {path} metadata!",
                    path = p.display()
                );
                return None;
            }
        };
        let file_type = FileType::get_file_type(&symlink_metadata);
        let (link_state, metadata) = if symlink_metadata.is_symlink() {
            let link_dst = format!("{dst}", dst = read_link(p).unwrap_or_default().display());
            if let Ok(metadata) = p.metadata() {
                (Some(LinkState::Working(link_dst)), metadata)
            } else {
                (Some(LinkState::Broken(link_dst)), symlink_metadata)
            }
        } else {
            (None, symlink_metadata)
        };
        let file_name = if let Some(temp) = p.file_name() {
            temp.to_str()
                .unwrap_or_else(|| {
                    log::warn!("Get File name failed from {path}!!!", path = p.display());
                    ""
                })
                .to_string()
        } else {
            log::error!("Suppose to be unreachable!!!");
            unreachable!()
        };

        Some(Self {
            file_name,
            file_path: p.to_path_buf(),
            file_type,
            mode: metadata.mode(),
            size: metadata.size(),
            mtime: metadata.mtime(),
            uid: metadata.uid(),
            gid: metadata.gid(),
            link_state,
        })
    }

    pub fn is_dir(&self) -> bool {
        self.file_path.is_dir()
    }

    pub fn size(&self) -> String {
        let unit = &['B', 'K', 'M', 'G'];
        let mut s = self.size;
        let mut divisor = 1f64;
        let mut i = 0usize;
        while i < unit.len() - 1 {
            if s < 1024 {
                break;
            }
            s >>= 10;
            divisor *= 1024f64;
            i += 1;
        }
        format!(
            "{size:.1}{unit}",
            size = self.size as f64 / divisor,
            unit = unit[i]
        )
    }

    pub fn mode(&self) -> String {
        format!("{}{}", self.file_type.symbol(), self.permissions())
    }

    fn permissions(&self) -> String {
        // rwxrwxrwx
        // --s--s--t
        // --S--S--T
        let mut ret = String::new();
        let mode = self.mode;
        //             suid           sgid       sticky bit
        let sst = &[mode & 0x800, mode & 0x400, mode & 0x200];
        let sst_symbol_l = &["s", "s", "t"];
        let sst_symbol_u = &["S", "S", "T"];
        let mut mask = 0x100;
        let mut i = 0usize;
        while mask != 0 {
            ret += if mask & mode != 0 { "r" } else { "-" };
            mask >>= 1;
            ret += if mask & mode != 0 { "w" } else { "-" };
            mask >>= 1;
            ret += match (mask & mode != 0, sst[i] != 0) {
                (true, true) => sst_symbol_l[i],
                (true, false) => "x",
                (false, true) => sst_symbol_u[i],
                (false, false) => "-",
            };
            mask >>= 1;
            i += 1;
        }
        ret
    }

    pub fn modify_time(&self) -> String {
        let mtime = UNIX_EPOCH + Duration::from_secs(self.mtime as u64);
        DateTime::<Local>::from(mtime)
            .format("%a %d/%m/%Y %X")
            .to_string()
    }

    pub fn is_executable(&self) -> bool {
        // TODO: maybe check user, group and others
        self.file_type == FileType::RegularFile && self.mode & 0o111 != 0
    }

    pub fn file_info(&self) -> String {
        format!(
            "{p} {s} {u} {g} {mtime}",
            p = self.mode(),            // filetype and permission
            s = self.size(),            // size
            u = self.owner(),           // user
            g = self.group(),           // group
            mtime = self.modify_time()  // modify time
        )
    }

    pub fn owner(&self) -> String {
        if let Some(user) = users::get_user_by_uid(self.uid) {
            return user.name().to_str().unwrap().to_string();
        }
        String::new()
    }

    pub fn group(&self) -> String {
        if let Some(group) = users::get_group_by_gid(self.gid) {
            return group.name().to_str().unwrap().to_string();
        }
        String::new()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_file_mode_1() {
        // `/usr/bin/passwd` -rwsr-xr-x  binary file(regular file)
        //                 regular file
        //                 rf  s  r wxr xr  x
        // 35309 to bin 0b1000 1001 1110 1101
        let file = File::new(&PathBuf::from("/usr/bin/passwd")).unwrap();
        assert_eq!(0, 35309 ^ file.mode);
    }

    #[test]
    fn test_file_mode_2() {
        let cargo_toml = File::new(&PathBuf::from("./Cargo.toml")).unwrap();
        let passwd = File::new(&PathBuf::from("/usr/bin/passwd")).unwrap();
        let root_tmp_dir = File::new(&PathBuf::from("/tmp")).unwrap();
        let tty = File::new(&PathBuf::from("/dev/tty")).unwrap();

        assert_eq!(String::from("-rw-r--r--"), cargo_toml.mode());
        assert_eq!(String::from("-rwsr-xr-x"), passwd.mode());
        assert_eq!(String::from("drwxrwxrwt"), root_tmp_dir.mode());
        assert_eq!(String::from("crw-rw-rw-"), tty.mode());
    }

    #[test]
    fn test_file_size() {
        let mut file = File::new(&PathBuf::from("./Cargo.toml")).unwrap();
        file.size = 10;
        assert_eq!(String::from("10.0B"), file.size());
        file.size = 1024;
        assert_eq!(String::from("1.0K"), file.size());
        file.size = 1024 * 123;
        assert_eq!(String::from("123.0K"), file.size());
        file.size = 1024 * 1024 * 123;
        assert_eq!(String::from("123.0M"), file.size());
        file.size = 1024 * 1024 * 1024 * 123;
        assert_eq!(String::from("123.0G"), file.size());
        file.size = 1024 * 1024 * 1024 * 1024 * 3;
        assert_eq!(String::from("3072.0G"), file.size());
    }
}
