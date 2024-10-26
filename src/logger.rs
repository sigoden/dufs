use anyhow::{Context, Result};
use chrono::{Local, SecondsFormat};
use log::{Level, LevelFilter, Metadata, Record};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

struct SimpleLogger {
    file: Option<Mutex<File>>,
}

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let timestamp = Local::now().to_rfc3339_opts(SecondsFormat::Secs, true);
            let text = format!("{} {} - {}", timestamp, record.level(), record.args());
            match &self.file {
                Some(file) => {
                    if let Ok(mut file) = file.lock() {
                        let _ = writeln!(file, "{text}");
                    }
                }
                None => {
                    if record.level() < Level::Info {
                        eprintln!("{text}");
                    } else {
                        println!("{text}");
                    }
                }
            }
        }
    }

    fn flush(&self) {}
}

pub fn init(log_file: Option<PathBuf>) -> Result<()> {
    let file = match log_file {
        None => None,
        Some(log_file) => {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_file)
                .with_context(|| {
                    format!("Failed to open the log file at '{}'", log_file.display())
                })?;
            Some(Mutex::new(file))
        }
    };
    let logger = SimpleLogger { file };
    log::set_boxed_logger(Box::new(logger))
        .map(|_| log::set_max_level(LevelFilter::Info))
        .with_context(|| "Failed to init logger")?;
    Ok(())
}
