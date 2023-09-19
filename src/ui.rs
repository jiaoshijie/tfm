use std::path::Path;

use crossterm::{cursor, execute, queue, terminal};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::buffer::{Attr, Buffer, Style};
use crate::config::theme;
use crate::dir::Dir;
use crate::file::{File, FileType, LinkState};
use crate::nav::Nav;
use crate::reg::Reg;
use crate::utils;

/*
 * ------------------------------------------------------------------
 * file path
 * ------------------------------------------------------------------
 * |           |                   |                                |
 * |           |                   |                                |
 * |           |                   |                                |
 * |  parent   |     current       |                                |
 * |   dir     |      dir          |         preview window         |
 * |  window   |     window        |                                |
 * |           |                   |                                |
 * |  panel-1  |     panel-2       |         panel-3                |
 * | index: 0  |    index: 1       |        index: 2                |
 * |           |                   |                                |
 * ------------------------------------------------------------------
 * file info, cmdline, and error message
 * ------------------------------------------------------------------
*/

pub struct Ui {
    out: Box<dyn std::io::Write>, // TODO: maybe just use `Stdout` instead of `Box<...>`
    buffer: Buffer,
    wins: [u16; 4],
    user: String,

    pub preview_layout: (u16, u16, u16, u16),
    pub toggle_ev_chan: utils::BoolChan,
}

impl Ui {
    pub fn new() -> Self {
        let buffer = Buffer::new();
        let (cols, rows) = buffer.size;
        Self {
            out: Box::new(std::io::stdout()),
            buffer,
            user: std::env::var("USER").unwrap_or_default(),
            wins: [0, cols / 6, cols / 2, cols - 1],
            preview_layout: (cols / 2 - 2, rows - 4, cols / 2 + 1, 2),
            toggle_ev_chan: utils::BoolChan::new(),
        }
    }

    pub fn init(&mut self) -> std::io::Result<()> {
        execute!(self.out, terminal::EnterAlternateScreen, cursor::Hide)?;
        terminal::enable_raw_mode()?;
        Ok(())
    }

    fn draw_border(&mut self) {
        let (cols, rows) = self.buffer.size;
        let style = theme::UI_BORDER_STYLE;
        for i in 1..cols - 1 {
            self.buffer.set_content(i, 1, '─', &style);
            self.buffer.set_content(i, rows - 2, '─', &style);
        }

        for i in 2..rows - 2 {
            self.buffer.set_content(0, i, '│', &style);
            self.buffer.set_content(self.wins[1], i, '│', &style);
            self.buffer.set_content(self.wins[2], i, '│', &style);
            self.buffer.set_content(cols - 1, i, '│', &style);
        }
        for i in 1..3 {
            self.buffer.set_content(self.wins[i], 1, '┬', &style);
            self.buffer.set_content(self.wins[i], rows - 2, '┴', &style);
        }
        self.buffer.set_content(0, 1, '┌', &style);
        self.buffer.set_content(cols - 1, 1, '┐', &style);
        self.buffer.set_content(0, rows - 2, '└', &style);
        self.buffer.set_content(cols - 1, rows - 2, '┘', &style);
    }

    pub fn draw(&mut self, nav: &Nav) -> std::io::Result<()> {
        for i in 0..self.buffer.size.0 {
            for j in 0..self.buffer.size.1 {
                self.buffer.set_content(i, j, ' ', &Style::default());
            }
        }
        self.draw_border();

        // panel-1 and panel-2
        let mut iter = nav.dirs.iter().rev();
        for i in (0..2).rev() {
            if let Some(dir) = iter.next() {
                if let Ok(ref dir) = dir.try_lock() {
                    self.draw_dir(nav, dir, i);
                } else {
                    self.draw_warn_message(i, "loading...");
                }
            }
        }

        // panel-3
        if let Ok(ref cdir) = nav.cdir().try_lock() {
            if let Some(files) = cdir.files() {
                log::info!(
                    "Index files: dir path => {path}, file len => {len}, index => {index}",
                    path = cdir.dir_path.display(),
                    len = cdir.files_len,
                    index = cdir.sp + cdir.bp
                );
                let file = &files[cdir.sp + cdir.bp];
                self.draw_pwd(&cdir.dir_path, &file.file_name);
                self.draw_preview(nav, &file.file_path);
                self.draw_status_line(nav, cdir, Some(file));
            } else {
                self.draw_pwd(&cdir.dir_path, "");
                self.draw_status_line(nav, cdir, None);
            }
        } else {
            self.draw_warn_message(2, "loading...");
        }

        self.buffer.draw(&mut self.out)?;
        Ok(())
    }

    fn draw_pwd(&mut self, path: &Path, file_name: &str) {
        let cols = self.buffer.size.0;
        let mut pos = 0;
        let user = format!("{user}:", user = self.user);
        self.buffer
            .set_line(pos, 0, cols.saturating_sub(pos), &user, &theme::USER_STYLE);
        pos += user.width() as u16;

        let suffix = if path.ends_with("/") { "" } else { "/" };
        let min_cols =
            (cols as usize).saturating_sub(suffix.width() + file_name.width() + pos as usize + 1);
        let path_str = if let Some(p) = path.to_str() {
            if p.width() > min_cols {
                self.fit_screen(p, "...", "", min_cols as u16, true)
            } else {
                p.to_string()
            }
        } else {
            log::error!("{path} may contain non-UTF-8!!!", path = path.display());
            String::from("")
        };
        self.buffer.set_line(
            pos,
            0,
            cols.saturating_sub(pos),
            &path_str,
            &theme::DIR_STYLE,
        );
        pos += path_str.width() as u16;
        self.buffer
            .set_line(pos, 0, cols.saturating_sub(pos), suffix, &theme::DIR_STYLE);
        pos += suffix.width() as u16;
        self.buffer.set_line(
            pos,
            0,
            cols.saturating_sub(pos),
            file_name,
            &theme::REG_FILE_STYLE,
        );
    }

    fn draw_status_line(&mut self, nav: &Nav, dir: &Dir, file: Option<&File>) {
        if nav.cmd_prefix != char::default() {
            self.draw_command_line(nav);
            return;
        }

        let (cols, rows) = self.buffer.size;
        if !nav.error_message.is_empty() {
            self.buffer.set_line(
                0,
                rows - 1,
                cols,
                &nav.error_message,
                &theme::ERROR_MSG_STYLE,
            );
            return;
        }

        let mut start = cols;
        if dir.files_len != 0 {
            let proportion = format!(
                " [{cur}/{all}]",
                cur = dir.sp + dir.bp + 1,
                all = dir.files_len
            );
            self.buffer
                .set_line_from_right(start, rows - 1, &proportion, &theme::PROPORTION_STYLE);
            start = start.saturating_sub(proportion.width() as u16);
        }

        if nav.mv_cp_total_size != 0 {
            let progress = format!(
                " [{p:.0}%] ",
                p = nav.mv_cp_size as f64 / nav.mv_cp_total_size as f64 * 100.0
            );
            self.buffer
                .set_line_from_right(start, rows - 1, &progress, &theme::PROGRESS_STYLE);
            start = start.saturating_sub(progress.width() as u16);
        }

        if !nav.selections.is_empty() {
            let sel = format!(" {count} ", count = nav.selections.len());
            self.buffer
                .set_line_from_right(start, rows - 1, &sel, &theme::SELECTION_STYLE);
            start = start.saturating_sub(sel.width() as u16);
        }

        if !nav.cut_or_copy.is_empty() {
            let cc = format!(" {count} ", count = nav.cut_or_copy.len());
            let style = if nav.is_cut {
                &theme::CUT_STYLE
            } else {
                &theme::COPY_STYLE
            };
            self.buffer.set_line_from_right(start, rows - 1, &cc, style);
            start = start.saturating_sub(cc.width() as u16);
        }

        if let Some(file) = file {
            let info = file.file_info();
            let mut pos = 0;
            self.buffer.set_line(
                pos,
                rows - 1,
                start.saturating_sub(pos),
                &info,
                &theme::FILE_INFO_STYLE,
            );
            pos += info.width() as u16;
            if file.file_type == FileType::Link {
                let (dst, style) = match file.link_state.as_ref().unwrap() {
                    LinkState::Working(dst) => (dst, &theme::LINK_WORKING_STYLE),
                    LinkState::Broken(dst) => (dst, &theme::LINK_BROKEN_STYLE),
                };
                let dst = format!(" -> {dst}");
                self.buffer
                    .set_line(pos, rows - 1, start.saturating_sub(pos), &dst, style);
                // pos += dst.len() as u16;
            }
        }
    }

    fn draw_command_line(&mut self, nav: &Nav) {
        if nav.cmd_prefix == char::default() {
            return;
        }
        let (cols, rows) = self.buffer.size;
        let cmd = format!(
            "{prefix}{cmd}",
            prefix = nav.cmd_prefix,
            cmd = nav.cmd_string
        );
        self.buffer
            .set_line(0, rows - 1, cols, &cmd, &Style::default());
        let cmd_cursor_pos = nav.cmd_string[..nav.cmd_string_ind].width() + 1;
        queue!(self.out, cursor::MoveTo(cmd_cursor_pos as u16, rows - 1))
            .unwrap_or_else(|err| log::error!("{err} => crossterm::queue!() failed"));
    }

    fn draw_preview(&mut self, nav: &Nav, p: &Path) {
        if p.is_dir() {
            if let Some(dir) = nav.dir_preview.clone() {
                if let Ok(preview) = dir.try_lock() {
                    self.draw_dir(nav, &preview, 2);
                    return;
                }
            }
        } else if let Some(reg) = nav.reg_preview.clone() {
            if let Ok(preview) = reg.try_lock() {
                self.draw_reg(nav, &preview, 2);
                return;
            }
        }
        self.draw_warn_message(2, "loading...");
    }

    fn draw_reg(&mut self, nav: &Nav, reg: &Reg, win_id: usize) {
        if reg.loadtime == 0 {
            self.draw_warn_message(2, "loading...");
            return;
        }
        let cols = self.wins[win_id + 1] - self.wins[win_id] - 1;
        let rows = std::cmp::min(reg.lines.len(), nav.useful_rows as usize);
        let mut st = Style::default();
        for ind in 0..rows {
            let mut pos = 0usize;
            let mut chars = reg.lines[ind].chars();
            let (mut l, mut r) = (0usize, 0usize);
            while let Some(ch) = chars.next() {
                l += ch.len_utf8();
                if ch == 27 as char {
                    if let Some(bch) = chars.next() {
                        if bch == '[' {
                            l += ch.len_utf8();
                            r = l;
                            for mch in chars.by_ref() {
                                if mch == 'm' {
                                    break;
                                }
                                r += mch.len_utf8();
                            }
                        }
                    }
                    st.parse_ansi_code(&reg.lines[ind][l..r]);
                    l = r + 1;
                } else {
                    // NOTE: if ch.width.unwrap() failed, it probably is a unshown character, so return 0 for this situation
                    let width = ch.width().unwrap_or_default();
                    if (pos + width) as u16 >= cols {
                        break;
                    }
                    if width != 0 {
                        self.buffer.set_content(
                            self.wins[win_id] + 1 + pos as u16,
                            2 + ind as u16,
                            ch,
                            &st,
                        );
                        pos += width;
                    }
                }
            }
        }
    }

    fn draw_dir(&mut self, nav: &Nav, dir: &Dir, win_id: usize) {
        if let Some(ref err) = dir.error_message {
            self.draw_error_message(win_id, err);
            return;
        };

        if dir.loadtime == 0 {
            self.draw_warn_message(win_id, "loading...");
            return;
        }

        let cc_style = if nav.is_cut {
            &theme::CUT_STYLE
        } else {
            &theme::COPY_STYLE
        };

        if let Some(files) = dir.files() {
            let offset = 1u16;
            let cols = self.wins[win_id + 1] - self.wins[win_id] - 1;
            let range = std::cmp::min(dir.files_len - dir.sp, nav.useful_rows as usize);
            for ind in 0..range {
                let file = &files[dir.sp + ind];
                let (filename, style) =
                    self.gen_styled_filename(file, cols - offset, ind == dir.bp);
                self.buffer.set_line(
                    self.wins[win_id] + 1 + offset,
                    2 + ind as u16,
                    cols - offset,
                    &filename,
                    &style,
                );
                if nav.selections.contains(&file.file_path) {
                    self.buffer.set_content(
                        self.wins[win_id] + 1,
                        2 + ind as u16,
                        ' ',
                        &theme::SELECTION_STYLE,
                    );
                }
                if nav.cut_or_copy.contains(&file.file_path) {
                    self.buffer
                        .set_content(self.wins[win_id] + 1, 2 + ind as u16, ' ', cc_style);
                }
            }
        } else {
            self.draw_warn_message(win_id, "empty");
        }
    }

    fn fit_screen(&self, s: &str, prefix: &str, suffix: &str, cols: u16, rev: bool) -> String {
        let plen = prefix.len();
        let slen = suffix.len();
        let cols = (cols as usize).saturating_sub(plen + slen);
        let s = if rev {
            utils::shrink_unicode_str_rev(s, cols)
        } else {
            utils::shrink_unicode_str(s, cols)
        };
        format!("{prefix}{s}{suffix}")
    }

    fn gen_styled_filename(&self, file: &File, cols: u16, sel: bool) -> (String, Style) {
        use crate::config::theme::*;
        let mut style = match file.file_type {
            FileType::RegularFile => {
                if file.is_executable() {
                    EXECUTABLE_FILE_STYLE
                } else {
                    REG_FILE_STYLE
                }
            }
            FileType::Directory => DIR_STYLE,
            FileType::Link => match file.link_state {
                Some(ref state) => match state {
                    LinkState::Working(_) => LINK_WORKING_STYLE,
                    LinkState::Broken(_) => LINK_BROKEN_STYLE,
                },
                None => REG_FILE_STYLE,
            },
            FileType::Pipe => PIPE_FILE_STYLE,
            FileType::Socket => SOCKET_FILE_STYLE,
            FileType::CharDevice => CHAR_FILE_STYLE,
            FileType::BlockDevice => BLOCK_FILE_STYLE,
            _ => REG_FILE_STYLE,
        };

        if sel {
            style.attrs.set(Attr::Reverse);
        }

        let width = file.file_name.width();
        // NOTE: `width + 1` for prefix space ' '
        let filename = if width + 1 >= cols as usize {
            self.fit_screen(&file.file_name, " ", "... ", cols, false)
        } else {
            let mut name = format!(" {name}", name = file.file_name);
            if sel {
                for _ in width..cols as usize {
                    name.push(' ');
                }
            }
            name
        };
        (filename, style)
    }

    fn draw_warn_message(&mut self, win_id: usize, msg: &str) {
        let cols = self.wins[win_id + 1] - self.wins[win_id] - 1;
        self.buffer
            .set_line(self.wins[win_id] + 1, 2, cols, msg, &theme::WARN_MSG_STYLE);
    }

    fn draw_error_message(&mut self, win_id: usize, msg: &str) {
        let cols = self.wins[win_id + 1] - self.wins[win_id] - 1;
        self.buffer
            .set_line(self.wins[win_id] + 1, 2, cols, msg, &theme::ERROR_MSG_STYLE);
    }

    pub fn suspend(&mut self) {
        self.toggle_ev_chan.bool_send.send(false).unwrap();
        self.buffer.reset();
        execute!(self.out, terminal::LeaveAlternateScreen, cursor::Show)
            .map_err(|err| {
                log::error!("{err} => LeaveAlternateScreen failed!");
            })
            .unwrap();
        terminal::disable_raw_mode()
            .map_err(|err| {
                log::error!("{err} => disable_raw_mode failed!");
            })
            .unwrap();
    }

    pub fn resume(&mut self) {
        self.toggle_ev_chan.bool_send.send(true).unwrap();
        execute!(self.out, terminal::EnterAlternateScreen, cursor::Hide)
            .map_err(|err| {
                log::error!("{err} => EnterAlternateScreen failed!");
            })
            .unwrap();
        terminal::enable_raw_mode()
            .map_err(|err| {
                log::error!("{err} => enable_raw_mode failed!");
            })
            .unwrap();
    }

    pub fn resize(&mut self, nav: &mut Nav, cols: u16, rows: u16) -> std::io::Result<()> {
        self.buffer.resize_x_y(cols, rows);
        nav.useful_rows = rows - 4;
        self.wins = [0, cols / 6, cols / 2, cols - 1];
        self.preview_layout = (cols / 2 - 2, rows - 4, cols / 2 + 1, 2);
        execute!(self.out, terminal::Clear(terminal::ClearType::All))?;
        Ok(())
    }

    pub fn renew(&mut self) {
        self.buffer.reset();
        execute!(self.out, terminal::Clear(terminal::ClearType::All))
            .map_err(|err| log::error!("{err} => crossterm::execute!() failed"))
            .unwrap();
    }

    pub fn show_cursor(&mut self) {
        queue!(self.out, cursor::Show)
            .map_err(|err| log::error!("{err} => crossterm::queue!() failed"))
            .unwrap();
    }

    pub fn hide_cursor(&mut self) {
        queue!(self.out, cursor::Hide)
            .map_err(|err| log::error!("{err} => crossterm::queue!() failed"))
            .unwrap();
    }
}

impl Drop for Ui {
    fn drop(&mut self) {
        terminal::disable_raw_mode()
            .unwrap_or_else(|err| log::error!("{err} => disable_raw_mode failed!"));
        execute!(self.out, terminal::LeaveAlternateScreen, cursor::Show).unwrap_or_else(|err| {
            log::error!("{err} => crossterm::execute!() LeaveAlternateScreen failed!")
        });
    }
}
