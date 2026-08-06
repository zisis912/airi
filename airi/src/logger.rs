use chrono::{DateTime, Local};
use colored::Colorize;
use log::{Level, LevelFilter, Metadata, Record, SetLoggerError};

struct SimpleLogger;

static LOGGER: SimpleLogger = SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        // if metadata.target().starts_with("wgpu") {
        //     return false;
        // }

        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let dt: DateTime<Local> = Local::now();
            println!(
                "[{} {} {}] {}",
                dt.format("%Y-%m-%d %H:%M:%S").to_string().cyan(),
                record.level().to_string().purple(),
                record.target(),
                record.args(),
            );
        }
    }

    fn flush(&self) {}
}

pub fn init_logger() -> Result<(), SetLoggerError> {
    log::set_logger(&LOGGER).map(|()| log::set_max_level(LevelFilter::Trace))
}
