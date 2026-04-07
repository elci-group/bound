
//! telemetry.rs
//! Tracks file processing progress, bytes read, tokens aggregated, and estimated remaining time.

use std::time::{Duration, Instant};
use colored::Colorize;

#[derive(Debug)]
pub struct Telemetry {
    pub files_processed: usize,
    pub bytes_read: usize,
    pub tokens_aggregated: usize,
    pub start_time: Instant,
}

impl Telemetry {
    /// Create a new telemetry tracker
    pub fn new() -> Self {
        Telemetry {
            files_processed: 0,
            bytes_read: 0,
            tokens_aggregated: 0,
            start_time: Instant::now(),
        }
    }

    /// Returns elapsed time since start
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Estimate remaining time based on files processed
    pub fn estimate_remaining(&self, total_files: usize) -> Option<Duration> {
        if self.files_processed == 0 || self.files_processed >= total_files {
            return None;
        }
        let remaining_files = total_files.saturating_sub(self.files_processed);
        let avg_per_file = self.elapsed().as_secs_f64() / self.files_processed as f64;
        Some(Duration::from_secs_f64(avg_per_file * remaining_files as f64))
    }

    /// Generate a progress report string
    pub fn report(&self, total_files: usize) -> String {
        let ebt_str = self.estimate_remaining(total_files)
            .map(|d| format!("{:.1}s", d.as_secs_f64()))
            .unwrap_or("--".to_string());

        let progress = if total_files > 0 {
            let percent = (self.files_processed as f64 / total_files as f64) * 100.0;
            format!("{:>3.0}%", percent).green().to_string()
        } else {
            "--%".to_string()
        };

        format!(
            "[{} | 📁 Files: {} | 📏 Bytes: {} | 🔢 Tokens: {} | ⏳ EBT: {}]",
            progress,
            self.files_processed,
            self.bytes_read,
            self.tokens_aggregated,
            ebt_str
        )
    }
}
