use crossbeam_channel::select;
use crossterm::event::{self, Event, KeyEvent, KeyModifiers};
use std::{
    collections::HashMap,
    path::Path,
    process::{Command, Stdio},
    thread, time,
};

use crate::action::{Action, CallAction, CmdAction, SetAction};
use crate::config::actions::KEYS;
use crate::nav::Nav;
use crate::ui::Ui;
use crate::utils;

pub struct App {
    pub ui: Ui,
    pub nav: Nav,
    pub quit: bool,
    keys: String,

    pub ev_chan: utils::CrossTermEventChan,
}

impl App {
    pub fn new() -> Self {
        Self {
            ui: Ui::new(),
            nav: Nav::new(),
            quit: false,
            keys: String::new(),

            ev_chan: utils::CrossTermEventChan::new(),
        }
    }

    pub fn run(&mut self, p: &Path) -> std::io::Result<()> {
        let mut keys: HashMap<&'static str, Box<dyn Action>> = HashMap::new();
        for t in KEYS {
            if t.0 == "c" {
                keys.insert(t.1, Box::new(CallAction::new(t.2, 1)));
            } else if t.0 == "s" {
                keys.insert(t.1, Box::new(SetAction::new(t.2)));
            } else if t.0 == "m" {
                let prefix = t.2.chars().next().unwrap();
                keys.insert(t.1, Box::new(CmdAction::new(&t.2[1..], prefix)));
            }
        }

        self.ui.init()?;
        self.nav.load_dirs(p)?;
        self.nav.update_preview(false, &self.ui.preview_layout);
        self.ev_chan_thread();

        // Infinite loop
        while !self.quit {
            self.ui.draw(&self.nav)?;
            select! {
                recv(self.nav.dir_chan.unit_recv) -> _ => {
                    log::info!("`dir_chan` received!!!");
                }
                recv(self.nav.reg_chan.unit_recv) -> _ => {
                    log::info!("`reg_chan` received!!!");
                }
                recv(self.nav.err_msg_chan.string_recv) -> err => {
                    self.nav.error_message = err.unwrap();
                }
                recv(self.nav.mv_cp_total_chan.u64_recv) -> n => {
                    self.nav.mv_cp_total_size = n.unwrap();
                    self.nav.mv_cp_size = 0;
                    log::info!("mv_cp_total_chan received => total_size: {}, mv_cp_size: {}", self.nav.mv_cp_total_size, self.nav.mv_cp_size);
                    self.nav.check_dirs();  // NOTE: this function doesn't run asynchronously.
                    self.nav.update_preview(true, &self.ui.preview_layout);
                }
                recv(self.nav.mv_cp_chan.u64_recv) -> n => {
                    self.nav.mv_cp_size += n.unwrap();
                    self.nav.check_dirs();  // NOTE: this function doesn't run asynchronously.
                    self.nav.update_preview(true, &self.ui.preview_layout);
                }
                recv(self.ev_chan.ev_recv) -> ev => {
                    log::info!("`ev_chan` received!!!");
                    let ev = ev.unwrap();
                    self.handle_event(ev, &mut keys)?;
                    // NOTE: When Operating System send keys(such as Paste text from clipboard) to tfm,
                    // the time slot between keys are very short.
                    // So the `loop` ensure tfm will receive all keys then redraw window.
                    loop {
                        select! {
                            recv(self.ev_chan.ev_recv) -> ev => {
                                let ev = ev.unwrap();
                                if !self.handle_event(ev, &mut keys)? {
                                    break;
                                }
                            }
                            default => break,
                        }
                    }
                    self.nav.check_dirs();  // NOTE: this function doesn't run asynchronously.
                    self.nav.update_preview(true, &self.ui.preview_layout);
                }
            }
        }
        Ok(())
    }

    fn handle_event(
        &mut self,
        ev: Event,
        keys: &mut HashMap<&'static str, Box<dyn Action>>,
    ) -> std::io::Result<bool> {
        match ev {
            Event::Key(ev) => {
                self.nav.error_message.clear();
                self.handle_key(&ev, keys);
            }
            Event::Resize(cols, rows) => self.resize(cols, rows)?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn ev_chan_thread(&self) {
        if let Err(err) = self.ui.toggle_ev_chan.bool_send.send(true) {
            log::error!("{err} => ev_chan_thread toggle_ev_chan.bool_send send failed!!!");
        }
        let ev_send = self.ev_chan.ev_send.clone();
        let toggle_ev_recv = self.ui.toggle_ev_chan.bool_recv.clone();
        thread::spawn(move || {
            while let Ok(t) = toggle_ev_recv.recv() {
                if !t {
                    continue;
                }
                loop {
                    if event::poll(time::Duration::from_micros(1_000)).unwrap() {
                        match event::read() {
                            Ok(ev) => {
                                if let Err(err) = ev_send.send(ev) {
                                    log::error!("{err} => Thread: ev_chan send ev_send failed!!!");
                                }
                            }
                            Err(err) => log::error!("{err} => Thread: event::read event failed!!!"),
                        }
                    }
                    if let Ok(t) = toggle_ev_recv.try_recv() {
                        // Err(err): empty channel recviced
                        if !t {
                            break;
                        }
                    }
                }
            }
        });
    }

    fn handle_key(&mut self, ev: &KeyEvent, keys: &mut HashMap<&'static str, Box<dyn Action>>) {
        if self.nav.cmd_prefix == char::default() {
            self.handle_normal_key(ev, keys);
        } else {
            self.handle_cmd_key(ev);
        }
    }

    fn handle_normal_key(
        &mut self,
        ev: &KeyEvent,
        keys: &mut HashMap<&'static str, Box<dyn Action>>,
    ) {
        // NOTE: not supported C-A-j
        let key = utils::keycode2str(ev.code);
        if ev.modifiers.contains(KeyModifiers::CONTROL) {
            self.keys += &format!("<C-{}>", key);
        } else if ev.modifiers.contains(KeyModifiers::ALT) {
            self.keys += &format!("<A-{}>", key);
        } else if ev.modifiers.contains(KeyModifiers::SHIFT) {
            if key.len() == 1 {
                self.keys += &key;
            } else {
                self.keys += &format!("<S-{}>", key);
            }
        } else {
            // NOTE: NONE etc.
            if key.len() == 1 {
                self.keys += &key;
            } else {
                self.keys += &format!("<{}>", key);
            }
        }
        log::info!("{keys}:{key}", keys = self.keys);
        if key == "Esc" {
            self.keys.clear();
        } else if let Some(a) = keys.get_mut(self.keys.as_str()) {
            a.run(self);
            self.keys.clear();
        } else {
            let mut have = false;
            for k in keys.keys() {
                if k.starts_with(&self.keys) {
                    have = true;
                    break;
                }
            }
            if !have {
                self.nav.error_message = format!("Unknown mapping: {key}", key = self.keys);
                self.keys.clear();
            }
        }
    }

    fn handle_cmd_key(&mut self, ev: &KeyEvent) {
        let key = utils::keycode2str(ev.code);
        log::info!("{key}:{cmd}", cmd = self.nav.cmd_string);
        let (control, _shift, alt) = (
            ev.modifiers.contains(KeyModifiers::CONTROL),
            ev.modifiers.contains(KeyModifiers::SHIFT),
            ev.modifiers.contains(KeyModifiers::ALT),
        );
        if key == "Esc" || (control && (key == "c" || key == "[")) {
            self.normal_mode();
        } else if key == "Enter" {
            CmdAction::new(&self.nav.cmd_string, self.nav.cmd_prefix).run(self);
            self.normal_mode();
        } else if (control && key == "Left") || (alt && key == "b") {
            let (_, new) = utils::find_word(&self.nav.cmd_string, self.nav.cmd_string_ind);
            self.nav.cmd_string_ind = new;
        } else if key == "Left" || (control && key == "b") {
            if self.nav.cmd_string_ind > 0 {
                let ch = self.nav.cmd_string[..self.nav.cmd_string_ind]
                    .chars()
                    .last()
                    .unwrap();
                self.nav.cmd_string_ind -= ch.len_utf8();
            }
        } else if (control && key == "Right") || (alt && key == "f") {
            let (_, new) = utils::find_word_rev(&self.nav.cmd_string, self.nav.cmd_string_ind);
            self.nav.cmd_string_ind = new;
        } else if key == "Right" || (control && key == "f") {
            if self.nav.cmd_string_ind < self.nav.cmd_string.len() {
                let ch = self.nav.cmd_string[self.nav.cmd_string_ind..]
                    .chars()
                    .next()
                    .unwrap();
                self.nav.cmd_string_ind += ch.len_utf8();
            }
        } else if control && key == "w" {
            let (old, new) = utils::find_word(&self.nav.cmd_string, self.nav.cmd_string_ind);
            self.nav.cmd_string_ind = new;
            self.nav.cmd_string = format!(
                "{}{}",
                &self.nav.cmd_string[..new],
                &self.nav.cmd_string[old..]
            );
        } else if key == "Backspace" || (control && key == "h") {
            if !self.nav.cmd_string.is_empty() && self.nav.cmd_string_ind > 0 {
                let ch = self.nav.cmd_string[..self.nav.cmd_string_ind]
                    .chars()
                    .last()
                    .unwrap();
                self.nav.cmd_string_ind -= ch.len_utf8();
                self.nav.cmd_string.remove(self.nav.cmd_string_ind);
            }
        } else if key == "Delete" {
            if self.nav.cmd_string_ind < self.nav.cmd_string.len() {
                self.nav.cmd_string.remove(self.nav.cmd_string_ind);
            }
        } else if alt && key == "d" {
            let (old, new) = utils::find_word_rev(&self.nav.cmd_string, self.nav.cmd_string_ind);
            self.nav.cmd_string = format!(
                "{}{}",
                &self.nav.cmd_string[..old],
                &self.nav.cmd_string[new..]
            );
            self.nav.cmd_string_ind = std::cmp::min(old, self.nav.cmd_string.len());
        } else if control && key == "u" {
            self.nav.cmd_string = self.nav.cmd_string[self.nav.cmd_string_ind..].to_string();
            self.nav.cmd_string_ind = 0;
        } else if control && key == "k" {
            self.nav.cmd_string = self.nav.cmd_string[..self.nav.cmd_string_ind].to_string();
        } else if (control && key == "a") || key == "Home" {
            self.nav.cmd_string_ind = 0;
        } else if (control && key == "e") || key == "End" {
            self.nav.cmd_string_ind = self.nav.cmd_string.len();
        } else if key.chars().count() == 1 && !control && !alt {
            self.nav
                .cmd_string
                .insert_str(self.nav.cmd_string_ind, &key);
            self.nav.cmd_string_ind += key.len();
        }
    }

    fn resize(&mut self, cols: u16, rows: u16) -> std::io::Result<()> {
        if cols < 12 || rows < 5 {
            log::warn!("The size({cols},{rows}) is too small!!!");
        } else {
            log::info!("({cols}, {rows})");
            self.ui.resize(&mut self.nav, cols, rows)?;
            self.nav.dirs.iter().for_each(|dir| match dir.lock() {
                Ok(mut dir) => dir.bound_position(self.nav.useful_rows as usize),
                Err(err) => log::error!("{err} => get self.dirs.dir lock failed"),
            });
            self.nav.reg_cache.clear();
        }
        Ok(())
    }

    pub fn command_mode(&mut self, prefix: Option<char>, cmd_string: Option<&str>) {
        self.nav.cmd_prefix = if let Some(p) = prefix {
            p
        } else {
            // NOTE: this `unwrap()` suppose not to failed!
            self.keys.as_str().chars().next().unwrap()
        };
        self.nav.cmd_string = if let Some(s) = cmd_string {
            s.to_string()
        } else {
            String::new()
        };
        self.nav.cmd_string_ind = self.nav.cmd_string.len();
        log::info!("cmd_prefix:{prefix}", prefix = self.nav.cmd_prefix);
        self.ui.show_cursor();
    }

    pub fn normal_mode(&mut self) {
        self.nav.cmd_prefix = char::default();
        self.nav.cmd_string.clear();
        self.ui.hide_cursor();
    }

    // prefix Wait Async Stdin Stdout Stderr UI_action
    //   $    No   No    Yes   Yes    Yes    Pasuse and then resume
    //   !    Yes  No    Yes   Yes    Yes    Pasuse and then wait user to enter a key to resume
    //   &    No   Yes   No    No     No     Do nothing
    pub fn run_shell(&mut self, cmd_str: &str, prefix: char) {
        self.nav.export_files();
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(cmd_str);
        log::info!("run_shell: running command `{cmd_str}`");
        match prefix {
            '$' | '!' => {
                self.ui.suspend();
                match cmd.status() {
                    Ok(status) => {
                        if !status.success() {
                            log::warn!(
                                "`{cmd_str}` return code is {code}",
                                code = status.code().unwrap()
                            );
                        }
                    }
                    Err(error) => log::error!("{error} -> running `{cmd_str}` failed!"),
                }
                if prefix == '!' {
                    utils::pause();
                }
                self.ui.resume();
            }
            '&' => {
                cmd.stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                if let Err(error) = cmd.spawn() {
                    log::error!("{error} -> asynchronously running `{cmd_str}` failed!");
                }
            }
            _ => unreachable!(),
        }
    }
}
