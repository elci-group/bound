
//! metadata.rs
//! Provides file metadata collection for bound outputs.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub relative_path: String,
    pub size_bytes: u64,
    pub line_count: usize,
    pub modified_unix: u64,
    pub sha256: Option<String>,
}

impl FileMetadata {
    /// Generate a standardized header string for aggregation
    pub fn to_header(&self) -> String {
        let ts = self.modified_unix;
        format!(
            "📄 FILE: {} \n📏 Size: {} bytes | 📝 Lines: {} | ⏰ Modified: {}\n----------------------------------------\n",
            self.relative_path,
            self.size_bytes,
            self.line_count,
            ts
        )
    }
}

/// Collects file metadata given a file path and root directory
pub fn collect_metadata(path: &Path, root: &Path, hash: bool) -> std::io::Result<FileMetadata> {
    let content = fs::read_to_string(path)?;
    let metadata = fs::metadata(path)?;

    let line_count = content.lines().count();

    let modified_unix = metadata
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let size_bytes = metadata.len();

    let relative_path = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    // Optional SHA-256 hash
    let sha256 = if hash {
        Some(hash_string(&content))
    } else {
        None
    };

    Ok(FileMetadata {
        relative_path,
        size_bytes,
        line_count,
        modified_unix,
        sha256,
    })
}

/// Simple SHA-256 hasher for file content
fn hash_string(data: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(data.as_bytes());
    format!("{:x}", h.finalize())
}
