use std::{
    collections::{HashMap, HashSet},
    env,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
};
use walkdir::WalkDir;

use crate::config::{CASE_INSENSITIVE, HIDDEN, SORT_TYPE};
use crate::dir::Dir;
use crate::reg::Reg;
use crate::utils;

pub struct Nav {
    pub useful_rows: u16, // terminal rows - 4

    pub dirs: Vec<Arc<Mutex<Dir>>>,
    pub dir_cache: HashMap<PathBuf, Arc<Mutex<Dir>>>,
    pub dir_preview: Option<Arc<Mutex<Dir>>>,
    pub dir_chan: utils::UnitChan,

    pub reg_cache: HashMap<PathBuf, Arc<Mutex<Reg>>>,
    pub reg_preview: Option<Arc<Mutex<Reg>>>,
    pub reg_chan: utils::UnitChan,

    pub cmd_prefix: char,
    pub cmd_string: String,
    pub cmd_string_ind: usize,
    pub search_string: String,
    pub search_direction: bool,

    pub selections: HashSet<PathBuf>,
    pub cut_or_copy: HashSet<PathBuf>,
    pub is_cut: bool,
    pub mv_cp_total_size: u64,
    pub mv_cp_size: u64,
    pub mv_cp_total_chan: utils::U64Chan,
    pub mv_cp_chan: utils::U64Chan,

    pub error_message: String,
    pub err_msg_chan: utils::StringChan,
}

impl Nav {
    pub fn new() -> Self {
        let (_, rows) = utils::terminal_size();
        Self {
            useful_rows: rows - 4,

            dirs: Vec::new(),
            dir_cache: HashMap::new(),
            dir_preview: None,
            dir_chan: utils::UnitChan::new(),

            reg_cache: HashMap::new(),
            reg_preview: None,
            reg_chan: utils::UnitChan::new(),

            cmd_prefix: char::default(),
            cmd_string: String::new(),
            cmd_string_ind: 0,
            search_string: String::new(),
            search_direction: true,

            selections: HashSet::new(),
            cut_or_copy: HashSet::new(),
            is_cut: false,
            mv_cp_total_size: 0,
            mv_cp_size: 0,
            mv_cp_total_chan: utils::U64Chan::new(),
            mv_cp_chan: utils::U64Chan::new(),

            error_message: String::new(),
            err_msg_chan: utils::StringChan::new(),
        }
    }

    pub fn load_dirs(&mut self, p: &Path) -> std::io::Result<()> {
        log::info!("load_dirs starts...");
        let path = if !p.is_absolute() {
            log::warn!(
                "`init_dirs()` => path is not an absolute path, using `std::env::current_dir()` instead of {path}",
                path = p.display()
            );
            std::env::current_dir()?
        } else {
            p.to_path_buf()
        };
        let all_paths = path.ancestors().into_iter().collect::<Vec<&Path>>();
        let end = all_paths.len().saturating_sub(1);
        self.dirs = all_paths
            .iter()
            .rev()
            .zip(all_paths.as_slice()[..end].iter().rev())
            .map(|(path, name)| self.load_dir(path, Some(name)))
            .collect();
        let last = self.load_dir(all_paths.first().unwrap(), None);
        self.dirs.push(last);
        log::info!("load_dirs finished");
        Ok(())
    }

    pub fn check_dirs(&mut self) {
        let rows = self.useful_rows;
        self.dirs.iter().for_each(|dir| {
            let mut lock = dir.lock().unwrap();
            let metadata_res = lock.dir_path.metadata();
            match metadata_res {
                Ok(metadata) => {
                    if lock.loadtime <= metadata.mtime() as u64 {
                        lock.update();
                        lock.bound_position(rows as usize);
                    }
                }
                Err(err) => {
                    log::error!(
                        "{err} => check_dirs get `{p}` metadata failed",
                        p = lock.dir_path.display()
                    );
                }
            }
        })
    }

    pub fn load_dir(&mut self, p: &Path, name: Option<&Path>) -> Arc<Mutex<Dir>> {
        let ret = match self.dir_cache.get(p) {
            Some(value) => value.clone(),
            None => {
                let value = Arc::new(Mutex::new(Dir::new(p)));
                self.dir_cache.insert(p.to_path_buf(), value.clone());
                value
            }
        };
        let send_to_thread = ret.clone();
        let dir_send = self.dir_chan.unit_send.clone();
        let rows = self.useful_rows;
        let name = if let Some(name) = name {
            log::info!("{path}", path = name.display());
            // NOTE: file_name() function will return None, only name ends with `..`, that should not happen.
            name.file_name().unwrap().to_str().map(|n| n.to_string())
        } else {
            None
        };
        thread::spawn(move || {
            match send_to_thread.lock() {
                Ok(mut lock) => match lock.dir_path.metadata() {
                    Ok(metadata) => {
                        let sort_type = SORT_TYPE.read().unwrap();
                        let hidden = HIDDEN.read().unwrap();

                        if lock.loadtime <= metadata.mtime() as u64 {
                            lock.update();
                        } else if lock.sort_type != *sort_type {
                            lock.sort_type = *sort_type;
                            lock.hidden = *hidden;
                            lock.sort();
                        } else if lock.hidden != *hidden {
                            lock.hidden = *hidden;
                            lock.hidden();
                            lock.sort();
                        }

                        lock.bound_position(rows as usize);

                        if let Some(name) = name {
                            lock.sel(&name, rows as usize);
                        }
                    }
                    Err(err) => log::error!(
                        "{err} => get `{p}` metadata failed!",
                        p = lock.dir_path.display()
                    ),
                },
                Err(err) => {
                    log::error!("{err} => Get dir lock failed");
                    return;
                }
            } // drop lock here to prevent dir_send finished, but thread not end, then ui.draw_preview try_lock() will failed.
            let _ = dir_send.send(());
        });
        ret
    }

    pub fn load_reg(&mut self, p: &Path, layout: &(u16, u16, u16, u16)) -> Arc<Mutex<Reg>> {
        let ret = match self.reg_cache.get(p) {
            Some(value) => value.clone(),
            None => {
                let value = Arc::new(Mutex::new(Reg::new(p)));
                self.reg_cache.insert(p.to_path_buf(), value.clone());
                value
            }
        };
        let send_to_thread = ret.clone();
        let reg_send = self.reg_chan.unit_send.clone();
        let layout = *layout;
        thread::spawn(move || {
            match send_to_thread.lock() {
                Ok(mut lock) => match lock.path.symlink_metadata() {
                    Ok(metadata) => {
                        if lock.loadtime <= metadata.mtime() as u64 {
                            lock.update(&layout);
                        }
                    }
                    Err(err) => log::error!(
                        "{err} => get `{p}` metadata failed!",
                        p = lock.path.display()
                    ),
                },
                Err(err) => {
                    log::error!("{err} => Get reg lock failed");
                    return;
                }
            }
            let _ = reg_send.send(());
        });
        ret
    }

    // return current directory
    pub fn cdir(&self) -> Arc<Mutex<Dir>> {
        // NOTE: When call this function, self.dirs should not be empty, at least have a `/`.
        self.dirs.last().unwrap().clone()
    }

    pub fn cfile(&mut self) -> Option<PathBuf> {
        if let Ok(ref cdir) = self.cdir().lock() {
            // NOTE: if files is empty, cdir.files will return None instead of empty slice
            if let Some(files) = cdir.files() {
                let file = &files[cdir.sp + cdir.bp];
                return Some(file.file_path.clone());
            }
        }
        None
    }

    // layout: (width, height, horizontal position, vertical position) of preview pane
    pub fn update_preview(&mut self, is_try: bool, layout: &(u16, u16, u16, u16)) {
        loop {
            if let Ok(ref mut cdir) = self.cdir().try_lock() {
                if let Some(files) = cdir.files() {
                    let file = &files[cdir.sp + cdir.bp];
                    if file.file_path.is_dir() {
                        self.dir_preview = Some(self.load_dir(&file.file_path, None));
                    } else {
                        self.reg_preview = Some(self.load_reg(&file.file_path, layout));
                    }
                    break;
                } else {
                    // NOTE: When cdir is empty, buf if loadtime doesn't equal to 0, then it has been loaded, break
                    if cdir.loadtime != 0 {
                        break;
                    }
                }
            } else {
                self.dir_preview = None;
                self.reg_preview = None;
            };
            if is_try {
                break;
            }
        }
    }

    pub fn sort(&mut self) {
        let hidden = *HIDDEN.read().unwrap();
        let sort_type = *SORT_TYPE.read().unwrap();
        self.dirs.iter().for_each(|dir| match dir.lock() {
            Ok(mut lock) => {
                let files_len = lock.files_len;
                if files_len != 0 {
                    let name = lock.files().unwrap()[lock.sp + lock.bp].file_name.clone();
                    lock.hidden = hidden;
                    lock.sort_type = sort_type;
                    lock.sort();
                    lock.sel(&name, self.useful_rows as usize);
                }
            }
            Err(err) => log::error!("{err} => get self.dirs.dir lock failed"),
        });
    }

    pub fn cd(&mut self, path_str: &str) {
        let path = PathBuf::from(path_str);
        if path.is_dir() {
            if let Err(err) = self.load_dirs(&path) {
                self.error_message = format!(
                    "ERROR: cd load_dirs `{path}`: `{err}`",
                    path = path.display(),
                    err = err.kind()
                );
                log::error!("{}", self.error_message)
            }
            if let Err(err) = env::set_current_dir(&path) {
                self.error_message = format!(
                    "ERROR: cd `{path}`:`{err}`",
                    path = path.display(),
                    err = err.kind()
                );
            }
        } else {
            self.error_message = format!("No dir found for `{path_str}`");
        }
    }

    fn toggle_selection(&mut self, path: &PathBuf) {
        if self.selections.contains(path) {
            self.selections.remove(path);
        } else {
            self.selections.insert(path.to_owned());
        }
    }

    pub fn toggle(&mut self) {
        if let Ok(ref cdir) = self.cdir().lock() {
            if let Some(files) = cdir.files() {
                let path = &files[cdir.sp + cdir.bp].file_path;
                if path.exists() {
                    self.toggle_selection(path);
                }
            }
        }
        self.down(1);
    }

    pub fn unselect(&mut self) {
        self.selections.clear();
    }

    // if selections is not empty, then select unseleted files and unselet already selected files.
    pub fn toggle_all(&mut self) {
        if let Ok(ref cdir) = self.cdir().lock() {
            if let Some(files) = cdir.files() {
                for file in files {
                    let path = &file.file_path;
                    if path.exists() {
                        self.toggle_selection(path);
                    }
                }
            }
        }
    }

    fn cut_or_copy(&mut self) {
        self.cut_or_copy.clear();
        if self.selections.is_empty() {
            if let Ok(ref cdir) = self.cdir().lock() {
                if let Some(files) = cdir.files() {
                    let path = &files[cdir.sp + cdir.bp].file_path;
                    if path.exists() {
                        self.cut_or_copy.insert(path.to_owned());
                    }
                }
            }
        } else {
            std::mem::swap(&mut self.cut_or_copy, &mut self.selections);
        }
    }

    pub fn cut(&mut self) {
        self.is_cut = true;
        self.cut_or_copy();
    }

    pub fn copy(&mut self) {
        self.is_cut = false;
        self.cut_or_copy();
    }

    pub fn clear(&mut self) {
        self.cut_or_copy.clear();
    }

    pub fn paste(&mut self) {
        if self.mv_cp_total_size != 0 || self.mv_cp_size != 0 {
            self.error_message =
                "ERROR: `paste` or `remove` opration already in progress!!!".to_string();
            return;
        }
        if self.cut_or_copy.is_empty() {
            self.error_message = "ERROR: No selected file to paste!!!".to_string();
            return;
        }
        let path_list = self.cut_or_copy.clone();
        let dst_dir = match self.cdir().lock() {
            Ok(lock) => {
                if lock.readonly {
                    self.error_message = "ERROR: target directory is readonly!!!".to_string();
                    return;
                }
                lock.dir_path.clone()
            }
            Err(err) => {
                self.error_message = format!("{err}: paste dst_dir cdir lock failed");
                return;
            }
        };
        let is_cut = self.is_cut;
        let total_chan = self.mv_cp_total_chan.u64_send.clone();
        let size_chan = self.mv_cp_chan.u64_send.clone();
        let err_msg_chan = self.err_msg_chan.string_send.clone();

        thread::spawn(move || {
            let mut total_size = 0;
            let mut update_size = 0;
            for p in &path_list {
                total_size += match utils::dir_size(p) {
                    Ok(size) => size,
                    Err(err) => {
                        err_msg_chan.send(err).unwrap();
                        return;
                    }
                };
            }
            total_chan
                .send(total_size)
                .map_err(|err| log::error!("{err} => total_chan send failed"))
                .unwrap();
            for p in &path_list {
                let file_name = p.file_name().unwrap().to_str().unwrap().to_string();
                let mut dst = dst_dir.join(&file_name);
                let mut count = 1;
                loop {
                    if dst.exists() {
                        let fname = format!("{file_name}.~{count}~");
                        dst = dst_dir.join(&fname);
                        count += 1;
                    } else {
                        break;
                    }
                }
                if is_cut && std::fs::rename(p, &dst).is_ok() {
                    update_size += utils::dir_size(&dst).unwrap();
                    if update_size / 1024 > 4096 {
                        size_chan.send(update_size).unwrap();
                        update_size = 0;
                    }
                    continue;
                }
                if p.is_symlink() {
                    match std::fs::read_link(p) {
                        Ok(from) => {
                            if let Err(err) = std::os::unix::fs::symlink(&from, &dst) {
                                let err_msg = format!(
                                    "{err}: std::os::unix::fs::symlink `{f} to {d}` failed",
                                    f = from.display(),
                                    d = dst.display()
                                );
                                log::error!("{err_msg}");
                                err_msg_chan.send(err_msg).unwrap();
                            }
                        }
                        Err(err) => {
                            let err_msg =
                                format!("{err}: std::fs::read_link `{p}` failed", p = p.display());
                            log::error!("{err_msg}");
                            err_msg_chan.send(err_msg).unwrap();
                        }
                    }
                } else {
                    for entry in WalkDir::new(p) {
                        update_size += match utils::dir_copy(entry.as_ref().unwrap(), &dst, p) {
                            Ok(size) => size,
                            Err(err) => {
                                // NOTE: copy this file failed, but continue copy other files.
                                let err_msg = format!(
                                    "{err}: copy file `{p}` failed",
                                    p = entry.unwrap().path().display()
                                );
                                log::error!("{err_msg}");
                                err_msg_chan.send(err_msg).unwrap();
                                0
                            }
                        };
                        if update_size / 1024 > 4096 {
                            size_chan.send(update_size).unwrap();
                            update_size = 0;
                        }
                    }
                }
                if is_cut {
                    if let Err(err) = utils::dir_remove(p) {
                        err_msg_chan
                            .send(format!("{err}: Remove file `{p}` failed", p = p.display()))
                            .unwrap();
                        break;
                    }
                }
            }
            total_chan
                .send(0)
                .map_err(|err| log::error!("{err} => total_chan send failed"))
                .unwrap();
        });

        if self.is_cut {
            self.cut_or_copy.clear();
        }
    }

    pub fn remove(&mut self) {
        if self.mv_cp_total_size != 0 || self.mv_cp_size != 0 {
            self.error_message =
                "ERROR: `paste` or `remove` opration already in progress!!!".to_string();
            return;
        }

        if self.selections.is_empty() {
            self.error_message = "No selected file to remove".to_string();
            return;
        }
        let path_list = self.selections.clone();
        let total_chan = self.mv_cp_total_chan.u64_send.clone();
        let size_chan = self.mv_cp_chan.u64_send.clone();
        let err_msg_chan = self.err_msg_chan.string_send.clone();
        thread::spawn(move || {
            let mut total_size = 0;
            let mut update_size = 0;
            for p in &path_list {
                total_size += match utils::dir_size(p) {
                    Ok(size) => size,
                    Err(err) => {
                        err_msg_chan.send(err).unwrap();
                        return;
                    }
                };
            }
            total_chan
                .send(total_size)
                .map_err(|err| log::error!("{err} => total_chan send failed"))
                .unwrap();
            for p in &path_list {
                update_size += utils::dir_size(p).unwrap();
                if let Err(err) = utils::dir_remove(p) {
                    err_msg_chan
                        .send(format!("{err}: Remove file `{p}` failed", p = p.display()))
                        .unwrap();
                    break;
                }
                if update_size / 1024 > 4096 {
                    size_chan.send(update_size).unwrap();
                    update_size = 0;
                }
            }
            total_chan
                .send(0)
                .map_err(|err| log::error!("{err} => total_chan send failed"))
                .unwrap();
        });
        self.selections.clear();
    }

    pub fn rename(&mut self, new_name: &str) {
        match self.cdir().lock() {
            Ok(mut lock) => match lock.files() {
                Some(files) => {
                    let path = files[lock.sp + lock.bp].file_path.clone();
                    let mut new_path = path.clone();
                    new_path.set_file_name(new_name);
                    if let Err(err) = std::fs::rename(&path, &new_path) {
                        self.error_message = format!(
                            "{err}: rename `{src}` to `{dst}` failed",
                            src = path.display(),
                            dst = new_path.display()
                        );
                        log::error!("{err_msg}", err_msg = self.error_message);
                        return;
                    }
                    lock.update();
                    lock.sel(new_name, self.useful_rows as usize);
                }
                None => {
                    self.error_message = "No file to rename".to_string();
                    log::error!("{err_msg}", err_msg = self.error_message);
                }
            },
            Err(err) => {
                self.error_message = format!("{err} => rename get lock failed");
                log::error!("{err_msg}", err_msg = self.error_message);
            }
        }
    }

    pub fn export_files(&mut self) {
        let currfile = match self.cfile() {
            Some(curr) => format!("{}", curr.display()),
            None => "".to_string(),
        };

        let selections: Vec<_> = self
            .selections
            .iter()
            .map(|p| format!("{}", p.display()))
            .collect();
        let curr_seletions = selections.join("\n");
        std::env::set_var("rust_tfm_f", &currfile);
        std::env::set_var("rust_tfm_fs", &curr_seletions);
        if self.selections.is_empty() {
            std::env::set_var("rust_tfm_fx", &currfile);
        } else {
            std::env::set_var("rust_tfm_fx", &curr_seletions);
        }
    }

    // NOTE: movement
    pub fn up_dir(&mut self) {
        if self.dirs.len() > 1 {
            self.dirs.pop();
        }
        if let Ok(ref cdir) = self.cdir().lock() {
            if let Err(error) = env::set_current_dir(&cdir.dir_path) {
                self.error_message = format!(
                    "ERROR: up_dir `{path}`:`{error}`",
                    path = cdir.dir_path.display(),
                    error = error.kind()
                );
                log::error!("{}", self.error_message);
            }
        }
    }

    pub fn up(&mut self, step: usize) {
        if let Ok(ref mut cdir) = self.cdir().try_lock() {
            if cdir.bp < step {
                cdir.sp = cdir.sp.saturating_sub(step - cdir.bp);
            }
            cdir.bp = cdir.bp.saturating_sub(step);
            cdir.bound_position(self.useful_rows as usize);
        };
    }

    pub fn down(&mut self, step: usize) {
        if let Ok(ref mut cdir) = self.cdir().try_lock() {
            let files_len = cdir.files_len;
            let rows = std::cmp::min(self.useful_rows as usize, files_len);
            if cdir.bp + step > rows {
                cdir.sp = std::cmp::min(files_len - rows, cdir.sp + (cdir.bp + step - rows));
            }
            cdir.bp = std::cmp::min(cdir.bp + step, rows.saturating_sub(1));
            cdir.bound_position(rows);
        };
    }

    pub fn top(&mut self) {
        if let Ok(ref mut cdir) = self.cdir().try_lock() {
            cdir.sp = 0;
            cdir.bp = 0;
        };
    }

    pub fn bottom(&mut self) {
        if let Ok(ref mut cdir) = self.cdir().try_lock() {
            let rows = self.useful_rows as usize;
            cdir.sp = cdir.files_len.saturating_sub(rows);
            cdir.bp = std::cmp::min(cdir.files_len.saturating_sub(1), rows.saturating_sub(1));
        };
    }

    pub fn search(&mut self, rev: bool) {
        if !self.cmd_string.is_empty() {
            self.search_string = self.cmd_string.clone();
        }

        if self.search_string.is_empty() {
            self.error_message = "ERROR: Search pattern is empty.".to_string();
            return;
        }

        if self.cmd_prefix == '/' {
            self.search_direction = true;
        } else if self.cmd_prefix == '?' {
            self.search_direction = false;
        }
        let found = if self.search_direction ^ rev {
            self.search_next()
        } else {
            self.search_prev()
        };

        if !found {
            self.error_message = format!("No file found for pattern `{p}`", p = self.search_string);
        }
    }

    fn search_next(&mut self) -> bool {
        if let Ok(ref mut cdir) = self.cdir().lock() {
            let files_len = cdir.files_len;
            let cpos = cdir.sp + cdir.bp;
            let ss = if CASE_INSENSITIVE {
                self.search_string.to_ascii_lowercase()
            } else {
                self.search_string.clone()
            };
            if let Some(files) = cdir.files() {
                let mut pos = if cpos == files_len - 1 { 0 } else { cpos + 1 };
                while pos != cpos {
                    let file_name = if CASE_INSENSITIVE {
                        files[pos].file_name.to_ascii_lowercase()
                    } else {
                        files[pos].file_name.clone()
                    };
                    if file_name.contains(&ss) {
                        if pos < cdir.sp {
                            cdir.sp = 0;
                        }
                        cdir.bp = pos - cdir.sp;
                        cdir.bound_position(self.useful_rows as usize);
                        return true;
                    }
                    pos += 1;
                    if pos >= files_len {
                        pos = 0;
                    }
                }
            }
        }
        false
    }

    fn search_prev(&mut self) -> bool {
        if let Ok(ref mut cdir) = self.cdir().lock() {
            let files_len = cdir.files_len;
            let cpos = cdir.sp + cdir.bp;
            let ss = if CASE_INSENSITIVE {
                self.search_string.to_ascii_lowercase()
            } else {
                self.search_string.clone()
            };
            if let Some(files) = cdir.files() {
                let mut pos = if cpos != 0 { cpos } else { files_len } - 1;
                while pos != cpos {
                    let file_name = if CASE_INSENSITIVE {
                        files[pos].file_name.to_ascii_lowercase()
                    } else {
                        files[pos].file_name.clone()
                    };
                    if file_name.contains(&ss) {
                        if pos < cdir.sp {
                            cdir.sp = 0;
                        }
                        cdir.bp = pos - cdir.sp;
                        cdir.bound_position(self.useful_rows as usize);
                        return true;
                    }
                    pos = if pos != 0 { pos } else { files_len } - 1;
                }
            }
        }
        false
    }
}
