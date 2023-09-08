use std::env;
use std::error::Error;
use std::path::PathBuf;

mod action;
mod app;
mod buffer;
mod config;
mod dir;
mod file;
mod nav;
mod reg;
mod ui;
mod utils;

use crate::config::{LOG_FILE_PATH, LOG_LEVEL, RUST_TFM};

fn main() -> Result<(), Box<dyn Error>> {
    // NOTE: set logfile path
    if let Some(c) = LOG_FILE_PATH.chars().next() {
        let log_file_path = if c == '~' {
            PathBuf::from(format!("{}/{}", env::var("HOME")?, &LOG_FILE_PATH[1..]))
        } else {
            PathBuf::from(LOG_FILE_PATH)
        };

        if let Some(dir) = log_file_path.parent() {
            if !dir.exists() {
                std::fs::create_dir_all(dir)?;
            }
        }

        let config = simplelog::ConfigBuilder::new()
            .set_location_level(simplelog::LevelFilter::Error)
            .build();

        simplelog::WriteLogger::init(LOG_LEVEL, config, std::fs::File::create(log_file_path)?)?;
    } // if `LOG_FILE_PATH` is empty, don't print log info to log file.
    std::env::set_var("RUST_TFM", RUST_TFM);
    let size = crossterm::terminal::size()?;
    if size.0 < 12 || size.1 < 5 {
        eprintln!(
            "terminal size (cols = {cols}, rows = {rows}) is too small!!!",
            cols = size.0,
            rows = size.1
        );
    } else {
        let current_dir = std::env::current_dir()?;
        let mut app = app::App::new();
        app.run(&current_dir)?;
    }

    Ok(())
}
