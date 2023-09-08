use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::config::{HIDDEN, SCROLL_OFF, SORT_TYPE};
use crate::file::File;
use crate::utils;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SortType {
    Natural,
    ModifyTime,
    Size,
}

impl SortType {
    pub fn sort(&self, files: &mut [File]) {
        match self {
            Self::Natural => Self::natural_sort(files),
            Self::ModifyTime => Self::modify_time_sort(files),
            Self::Size => Self::size_sort(files),
        }
    }

    fn natural_sort(files: &mut [File]) {
        files.sort_by(|a, b| utils::natural_cmp(&a.file_name, &b.file_name));
    }

    fn modify_time_sort(files: &mut [File]) {
        files.sort_by(|a, b| a.mtime.cmp(&b.mtime));
    }

    fn size_sort(files: &mut [File]) {
        files.sort_by(|a, b| a.size.cmp(&b.size));
    }
}

/*
 *         file0                                         // file0 index is `0` in vec
 *          .
 *          .
 *          .
 *         file
 * -------------------------                             // `window_line`
 * |       file1           | <- sp (here, bp is 0)       // file1 index is `sp` in vec
 * |        .              |
 * |        .              |
 * |       file2           | <- bp                       // file2 index is `sp + bp` in vec, `bp` is indexed from `window_line`
 * |        .              |
 * |        .              |
 * -------------------------
 *        file
 *         .
 *         .
 *         .
*/

#[derive(Debug)]
pub struct Dir {
    pub dir_path: PathBuf,
    files: Option<Vec<File>>,
    pub files_len: usize,
    pub sp: usize,                     // start printed file postion
    pub bp: usize,                     // current viewed window cursor postion
    pub error_message: Option<String>, // such as no permission to read
    fsnd: usize,                       // files not displayed
    pub loadtime: u64,
    pub sort_type: SortType,
    pub hidden: bool,
    pub readonly: bool,
}

impl Dir {
    pub fn new(p: &Path) -> Self {
        Self {
            dir_path: p.to_path_buf(),
            files: None,
            files_len: 0,
            sp: 0,
            bp: 0,
            error_message: None,
            fsnd: 0,
            loadtime: 0,
            sort_type: *SORT_TYPE.read().unwrap(),
            hidden: *HIDDEN.read().unwrap(),
            readonly: false,
        }
    }

    pub fn update(&mut self) {
        match self.dir_path.metadata() {
            Ok(metadata) => self.readonly = metadata.permissions().readonly(),
            Err(err) => {
                self.readonly = true;
                log::error!("{err} => dir update metadata failed");
            }
        }
        match self.dir_path.read_dir() {
            Ok(iter) => {
                let new_loadtime = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap() // NOTE: `unwrap()` should not fail, because of UNIX_EPOCH.
                    .as_secs();
                self.files = iter
                    .filter_map(|res| res.ok()) // NOTE: if `res` is Error, just ignore it and don't print any error msg to logfile.
                    .map(|dir_entry| File::new(&dir_entry.path()))
                    .collect();
                self.sort();
                self.loadtime = new_loadtime;
            }
            Err(err) => self.error_message = Some(format!("{err}", err = err.kind())),
        }
    }

    pub fn files(&self) -> Option<&[File]> {
        if let Some(ref files) = self.files {
            if self.fsnd < files.len() {
                return Some(&files[self.fsnd..]);
            }
        }
        None
    }

    fn files_mut(&mut self) -> Option<&mut [File]> {
        if let Some(ref mut files) = self.files {
            if self.fsnd < files.len() {
                return Some(&mut files[self.fsnd..]);
            }
        }
        None
    }

    pub fn hidden(&mut self) {
        if let Some(ref mut all_files) = self.files {
            if all_files.is_empty() {
                return;
            }
            self.fsnd = 0;
            if self.hidden {
                all_files.sort_by(|a, b| {
                    let (a, b) = (utils::is_hidden(a), utils::is_hidden(b));
                    b.cmp(&a)
                });
                if utils::is_hidden(&all_files[all_files.len() - 1]) {
                    self.fsnd = all_files.len();
                } else {
                    for (i, f) in all_files.iter().enumerate() {
                        if !utils::is_hidden(f) {
                            self.fsnd = i;
                            break;
                        }
                    }
                }
            }
            // self.files_len = all_files[self.fsnd..].len();
            self.files_len = all_files.len() - self.fsnd;
        }
    }

    pub fn sort(&mut self) {
        self.files_len = 0;
        self.hidden();
        // NOTE: if files is empty, self.files_mut() will return None instead of empty slice.
        let sort_type = self.sort_type;
        if let Some(ref mut files) = self.files_mut() {
            sort_type.sort(files);

            // NOTE: dir-first
            files.sort_by(|a, b| {
                let (a, b) = (a.is_dir(), b.is_dir());
                b.cmp(&a)
            });
        }
    }

    pub fn sel(&mut self, name: &str, rows: usize) {
        if let Some(files) = self.files() {
            let mut ind = std::cmp::min(self.files_len - 1, self.sp + self.bp);
            if files[ind].file_name != name {
                for (i, f) in files.iter().enumerate() {
                    if f.file_name == name {
                        ind = i;
                        break;
                    }
                }
            }
            if ind <= self.sp {
                self.sp = 0;
            }
            self.bp = ind - self.sp;
            self.bound_position(rows);
        }
    }

    pub fn bound_position(&mut self, rows: usize) {
        let files_len = self.files_len;
        let sp = std::cmp::min(self.sp, files_len.saturating_sub(rows));
        self.bp += self.sp - sp;
        self.sp = sp;
        self.bp = std::cmp::min(self.bp, files_len.saturating_sub(1));
        log::info!("bp: {bp}, files_len: {files_len}", bp = self.bp);
        if files_len > rows {
            let scroll_off = if rows % 2 == 0 {
                std::cmp::min(SCROLL_OFF as usize, rows / 2 - 1)
            } else {
                std::cmp::min(SCROLL_OFF as usize, rows / 2)
            };

            if self.bp <= scroll_off && self.sp > 0 {
                let offset = std::cmp::min(scroll_off - self.bp, self.sp);
                self.sp -= offset;
                self.bp += offset;
            } else if self.bp >= rows - scroll_off && files_len - self.sp > rows {
                let offset =
                    std::cmp::min(self.bp + scroll_off + 1 - rows, files_len - self.sp - rows);
                self.sp += offset;
                self.bp -= offset;
            }
        }
    }
}
