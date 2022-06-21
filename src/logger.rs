use chrono::{Local, SecondsFormat};
use log::{Level, Metadata, Record};
use log::{LevelFilter, SetLoggerError};

struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let timestamp = Local::now().to_rfc3339_opts(SecondsFormat::Secs, true);
            if record.level() < Level::Info {
                eprintln!("{} {} - {}", timestamp, record.level(), record.args());
            } else {
                println!("{} {} - {}", timestamp, record.level(), record.args());
            }
        }
    }

    fn flush(&self) {}
}

static LOGGER: SimpleLogger = SimpleLogger;

pub fn init() -> Result<(), SetLoggerError> {
    log::set_logger(&LOGGER).map(|()| log::set_max_level(LevelFilter::Info))
}
