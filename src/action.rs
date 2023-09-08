use std::env;

use crate::app::App;
use crate::config::{HIDDEN, OPENER, SORT_TYPE};
use crate::dir::SortType;

pub trait Action {
    fn run(&mut self, app: &mut App);
}

pub struct CallAction {
    op: String,
    count: usize,
}

impl CallAction {
    pub fn new(op: &str, count: usize) -> Self {
        Self {
            op: op.to_string(),
            count,
        }
    }
}

impl Action for CallAction {
    fn run(&mut self, app: &mut App) {
        match self.op.as_str() {
            "quit" => {
                // quit tfm
                if app.nav.mv_cp_total_size == 0 && app.nav.mv_cp_size == 0 {
                    app.quit = true;
                } else {
                    app.nav.error_message = "`paste` or `remove` operation in progress".to_string();
                }
            }
            "updir" => app.nav.up_dir(),
            "open" => {
                let file_path = app.nav.cfile();
                if let Some(file_path) = file_path {
                    if file_path.is_dir() {
                        if let Err(error) = env::set_current_dir(&file_path) {
                            app.nav.error_message = format!(
                                "ERROR: open `{path}`:`{error}`",
                                path = file_path.display(),
                                error = error.kind()
                            );
                        }
                        let dst_dir = app.nav.load_dir(&file_path, None);
                        app.nav.dirs.push(dst_dir);
                    } else {
                        let cmd = format!("{OPENER} '{}'", file_path.display());
                        app.run_shell(&cmd, '$');
                    }
                }
            }
            "up" => app.nav.up(self.count),
            "down" => app.nav.down(self.count),
            "top" => app.nav.top(),
            "bottom" => app.nav.bottom(),
            "redraw" => app.ui.renew(),
            "command_mode" => app.command_mode(None, None),
            "shell" => app.command_mode(Some('$'), None),
            "Shell" => {
                let shell = std::env::var("SHELL").unwrap_or("bash".to_string());
                app.run_shell(&shell, '$');
                app.ui.renew();
            }
            "search_next" => app.nav.search(false),
            "search_prev" => app.nav.search(true),
            "toggle" => app.nav.toggle(),
            "toggle_all" => app.nav.toggle_all(),
            "unselect" => app.nav.unselect(),
            "cut" => app.nav.cut(),
            "copy" => app.nav.copy(),
            "clear" => app.nav.clear(),
            "paste" => app.nav.paste(),
            "remove" => app.nav.remove(),
            "rename" => {
                if let Ok(ref cdir) = app.nav.cdir().lock() {
                    if let Some(files) = cdir.files() {
                        let file = &files[cdir.sp + cdir.bp];
                        let s = format!("rename {}", file.file_name);
                        app.command_mode(Some(':'), Some(&s));
                    } else {
                        app.nav.error_message = "ERROR: No selected file to rename".to_string();
                    }
                }
            }
            _ => {
                app.nav.error_message =
                    format!("CallAction run `{op}` failed:`No Operation`", op = &self.op);
            }
        }
    }
}

pub struct SetAction {
    op: String,
    val: String,
}

impl SetAction {
    pub fn new(c: &str) -> Self {
        let s: Vec<_> = c.split(' ').collect();
        let val = if s.len() > 1 {
            s[1].to_string()
        } else {
            String::new()
        };
        Self {
            op: s[0].to_string(),
            val,
        }
    }
}

impl Action for SetAction {
    fn run(&mut self, app: &mut App) {
        match self.op.as_str() {
            "hidden" | "unhidden" | "hidden!" => {
                match HIDDEN.write() {
                    Ok(mut hidden) => match self.op.as_str() {
                        "hidden" => *hidden = true,
                        "unhidden" => *hidden = false,
                        "hidden!" => *hidden ^= true,
                        _ => unreachable!(),
                    },
                    Err(err) => {
                        app.nav.error_message = format!("{err} => get `HIDDEN` write lock failed");
                        log::error!("{}, so didn't do any thing!!!", app.nav.error_message);
                        return;
                    }
                }
                app.nav.sort();
            }
            "sortby" => {
                match SORT_TYPE.write() {
                    Ok(mut sort_type) => {
                        *sort_type = match self.val.as_str() {
                            "natural" => SortType::Natural,
                            "mtime" => SortType::ModifyTime,
                            "size" => SortType::Size,
                            _ => SortType::Natural,
                        }
                    }
                    Err(err) => {
                        app.nav.error_message =
                            format!("{err} => get `SORT_TYPE` write lock failed");
                        log::error!("{}, so didn't do any thing!!!", app.nav.error_message);
                        return;
                    }
                }
                app.nav.sort();
            }
            _ => {
                app.nav.error_message =
                    format!("SetAction run `{op}` failed:`No Operation`", op = &self.op);
            }
        }
    }
}

pub struct CmdAction {
    prefix: char,
    cmd: String,
}

impl CmdAction {
    pub fn new(c: &str, prefix: char) -> Self {
        Self {
            prefix,
            cmd: c.to_string(),
        }
    }
}

impl Action for CmdAction {
    fn run(&mut self, app: &mut App) {
        match self.prefix {
            ':' => {
                let (op, cmd) = match self.cmd.find(' ') {
                    Some(i) => (&self.cmd[0..i], &self.cmd[i + 1..]),
                    None => (&self.cmd[..], ""),
                };
                match op {
                    "set" => {
                        SetAction::new(cmd).run(app);
                    }
                    "cd" => app.nav.cd(cmd),
                    "q" => app.normal_mode(), // quit command mode
                    "quit" => CallAction::new("quit", 0).run(app), // quit tfm
                    "quit!" => app.quit = true, // NOTE: quit tfm always
                    "rename" => {
                        let new_name = cmd.trim();
                        app.nav.rename(new_name);
                    }
                    _ => {
                        app.nav.error_message = format!(
                            "CmdAction run `{prefix}{cmd}` failed:`No Operation`",
                            prefix = self.prefix,
                            cmd = &self.cmd
                        );
                    }
                }
            }
            '/' | '?' => app.nav.search(false),
            '$' | '!' | '&' => app.run_shell(&self.cmd, self.prefix),
            _ => {
                app.nav.error_message = format!(
                    "CmdAction run `{prefix}{cmd}` failed:`No Operation`",
                    prefix = self.prefix,
                    cmd = &self.cmd
                );
            }
        }
    }
}
