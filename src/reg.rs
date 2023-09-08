use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::config::PREVIEWER;

pub struct Reg {
    pub path: PathBuf,
    pub lines: Vec<String>,
    pub loadtime: u64,
}

impl Reg {
    pub fn new(p: &Path) -> Self {
        Self {
            path: p.to_path_buf(),
            lines: Vec::new(),
            loadtime: 0,
        }
    }

    pub fn update(&mut self, layout: &(u16, u16, u16, u16)) {
        self.lines.clear();
        let new_loadtime = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap() // NOTE: `unwrap()` should not fail, because of UNIX_EPOCH.
            .as_secs();
        let mut cmd = std::process::Command::new("sh");
        // Five arguments are passed to the file,
        // $1 current file name
        // $2 width
        // $3 height
        // $4 horizontal position
        // $5 vertical position
        // of preview pane respectively.
        cmd.arg("-c").arg(format!(
            "{} '{}' '{}' '{}' '{}' '{}'",
            PREVIEWER,
            self.path.display(),
            layout.0,
            layout.1,
            layout.2,
            layout.3
        ));
        match cmd.output() {
            Ok(output) => {
                if output.status.success() {
                    match String::from_utf8(output.stdout) {
                        Ok(out) => {
                            for line in out.lines() {
                                self.lines.push(line.to_string());
                            }
                        }
                        Err(err) => log::error!(
                            "{err} => reg_preview convert command `{cmd} {file}` output failed!",
                            cmd = PREVIEWER,
                            file = self.path.display()
                        ),
                    }
                }
            }
            Err(err) => log::error!(
                "{err} => reg_preview run command `{cmd} {file}` failed!",
                cmd = PREVIEWER,
                file = self.path.display()
            ),
        }
        if self.lines.is_empty() {
            self.lines.push(String::from("\x1b[7mbinary\x1b[0m"));
        }
        self.loadtime = new_loadtime;
    }

    // TODO:
    // pub fn scroll_down(&mut self) {
    //     todo!()
    // }

    // pub fn scroll_up(&mut self) {
    //     todo!()
    // }
}
