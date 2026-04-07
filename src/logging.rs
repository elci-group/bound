
//! logging.rs
//! Simple logging wrapper with levels and optional file output.

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use colored::Colorize;

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

pub struct Logger {
    pub level: LogLevel,
    pub file: Option<Mutex<std::fs::File>>,
}

impl Logger {
    /// Create a new Logger with optional log file
    pub fn new(level: LogLevel, file_path: Option<&str>) -> Self {
        let file = file_path.map(|p| {
            Mutex::new(
                OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(p)
                    .expect("Failed to open log file"),
            )
        });
        Logger { level, file }
    }

    /// Log a message with a given level
    pub fn log(&self, lvl: LogLevel, msg: &str) {
        if (lvl as u8) > (self.level as u8) {
            return; // Skip messages below current level
        }

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let level_plain = match lvl {
            LogLevel::Error => "❌ ERROR",
            LogLevel::Warn => "⚠️ WARN",
            LogLevel::Info => "ℹ️ INFO",
            LogLevel::Debug => "🐛 DEBUG",
            LogLevel::Trace => "🔍 TRACE",
        };

        let level_colored = match lvl {
            LogLevel::Error => "❌ ERROR".red().bold(),
            LogLevel::Warn => "⚠️ WARN".yellow().bold(),
            LogLevel::Info => "ℹ️ INFO".blue().bold(),
            LogLevel::Debug => "🐛 DEBUG".green().bold(),
            LogLevel::Trace => "🔍 TRACE".magenta().bold(),
        };

        let ts_colored = format!("[{}]", ts).dimmed();

        let plain_line = format!("[{}] [{}] {}\n", ts, level_plain, msg);

        // Print to stderr with colors
        eprint!("{} {} {}\n", ts_colored, level_colored, msg);

        // Write to file if configured (plain)
        if let Some(f) = &self.file {
            let mut f = f.lock().unwrap();
            let _ = f.write_all(plain_line.as_bytes());
        }
    }

    /// Convenience methods for each level
    pub fn error(&self, msg: &str) { self.log(LogLevel::Error, msg); }
    pub fn warn(&self, msg: &str) { self.log(LogLevel::Warn, msg); }
    pub fn info(&self, msg: &str) { self.log(LogLevel::Info, msg); }
    pub fn debug(&self, msg: &str) { self.log(LogLevel::Debug, msg); }
    pub fn trace(&self, msg: &str) { self.log(LogLevel::Trace, msg); }
}
