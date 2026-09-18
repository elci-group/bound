//! bound-core
//!
//! Core file-aggregation engine used by the `bound` CLI and by downstream
//! tools that need a deterministic, metadata-rich project snapshot.

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ignore::WalkBuilder;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

pub mod expandable;
pub mod furnace;
pub mod logging;
mod mesut_backend;
pub mod metadata;
pub use mesut_backend::bundle_with_mesut;
pub mod redaction;
pub mod telemetry;
pub mod tree;

pub use expandable::{wrap_expandable, ExpandableBlock};
pub use furnace::{analyze_file, FurnaceReport};
pub use logging::{LogLevel, Logger};
pub use metadata::{collect_metadata, FileMetadata};
pub use redaction::{
    build_redaction_engine, redact_paths_in_snapshot, RedactionConfig, RedactionEngine,
    RedactionOptions, RedactionStats,
};
pub use telemetry::Telemetry;
pub use tree::generate_tree;

static REF_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"(?m)^\s*import\s+([a-zA-Z0-9_\.]+)").unwrap(),
        Regex::new(r"(?m)^\s*from\s+([a-zA-Z0-9_\.]+)\s+import").unwrap(),
        Regex::new(r#"require\(['"](.+?)['"]\)"#).unwrap(),
        Regex::new(r#"(?m)^\s*import\s+.*\s+from\s+['"](.+?)['"]"#).unwrap(),
        Regex::new(r#"(?m)^\s*#include\s*["<](.+?)[">]"#).unwrap(),
    ]
});

/// Optional filter specification recorded in the snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterSpec {
    pub extension: Option<String>,
    pub dependency_aware: bool,
}

/// Optional limits recorded in the snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Limits {
    pub token: Option<usize>,
    pub size: Option<usize>,
    pub depth: Option<usize>,
}

/// Summary statistics for a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub file_count: usize,
    pub total_bytes: usize,
    pub total_lines: usize,
}

/// A single file in a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub relative_path: String,
    pub language: Option<String>,
    pub size_bytes: u64,
    pub lines: usize,
    pub modified_unix: u64,
    pub sha256: Option<String>,
    pub content: Option<String>,
    pub dependencies: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub furnace_report: Option<FurnaceReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redactions: Option<usize>,
}

/// Versioned snapshot produced by `bound-core`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema: String,
    pub schema_version: String,
    pub producer: String,
    pub producer_version: String,
    pub target: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_commit: Option<String>,
    pub generated_at: String,
    pub filter: FilterSpec,
    pub limits: Limits,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree: Option<String>,
    pub files: Vec<FileEntry>,
    pub summary: Summary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redaction_stats: Option<RedactionStats>,
}

/// Options controlling a bundling run.
#[derive(Debug, Clone)]
pub struct BundleOptions {
    pub directory: PathBuf,
    pub filter: Option<String>,
    pub token_limit: Option<usize>,
    pub size_limit: Option<usize>,
    pub depth_limit: Option<usize>,
    pub include_meta: bool,
    pub include_meta_hash: bool,
    pub include_tree: bool,
    pub include_furnace: bool,
    pub git_commit: Option<String>,
    pub redaction: Option<RedactionOptions>,
}

impl Default for BundleOptions {
    fn default() -> Self {
        Self {
            directory: PathBuf::from("."),
            filter: None,
            token_limit: None,
            size_limit: None,
            depth_limit: None,
            include_meta: false,
            include_meta_hash: false,
            include_tree: false,
            include_furnace: false,
            git_commit: None,
            redaction: None,
        }
    }
}

/// Output of a bundling run.
pub struct BundleOutput {
    pub snapshot: Snapshot,
    /// Human-readable expandable-block output. Only produced when not in JSON
    /// mode and only when the caller requests it (currently the CLI uses the
    /// snapshot directly for JSON, and builds text output from it otherwise).
    pub text: Option<String>,
}

/// Errors that can occur during bundling.
#[derive(Debug)]
pub enum BundleError {
    InvalidFilter(String),
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for BundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BundleError::InvalidFilter(s) => write!(f, "Invalid filter: {}", s),
            BundleError::Io(e) => write!(f, "IO error: {}", e),
            BundleError::Json(e) => write!(f, "JSON error: {}", e),
        }
    }
}

impl std::error::Error for BundleError {}

impl From<std::io::Error> for BundleError {
    fn from(e: std::io::Error) -> Self {
        BundleError::Io(e)
    }
}

impl From<serde_json::Error> for BundleError {
    fn from(e: serde_json::Error) -> Self {
        BundleError::Json(e)
    }
}

fn parse_filter(filter: Option<&str>) -> Result<(Option<String>, bool), BundleError> {
    match filter {
        None => Ok((None, false)),
        Some(f) if f.starts_with('[') && f.ends_with(']') => {
            let ext = f[1..f.len() - 1].trim_start_matches('.').to_string();
            Ok((Some(ext), false))
        }
        Some(f) if f.starts_with('{') && f.ends_with('}') => {
            let ext = f[1..f.len() - 1].trim_start_matches('.').to_string();
            Ok((Some(ext), true))
        }
        Some(f) => Err(BundleError::InvalidFilter(f.to_string())),
    }
}

fn parse_references_generic(path: &Path) -> std::io::Result<Vec<String>> {
    let content = fs::read_to_string(path)?;
    let mut references = Vec::new();
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

    for re in REF_PATTERNS.iter() {
        for cap in re.captures_iter(&content) {
            if let Some(m) = cap.get(1) {
                let mut r = m.as_str().to_string();
                if ext == "py" || ext == "js" || ext == "ts" {
                    r = r.replace('.', "/");
                }
                if !r.contains('.') {
                    r = format!("{}.{}", r, ext);
                }
                references.push(r);
            }
        }
    }
    Ok(references)
}

fn resolve_ref_path(source: &Path, ref_str: &str, root: &Path) -> PathBuf {
    let base_dir = source.parent().unwrap_or(root);
    let mut candidate = base_dir.join(ref_str);

    if let Ok(canon) = fs::canonicalize(&candidate) {
        candidate = canon;
    } else {
        let mut comps = Vec::new();
        for comp in candidate.components() {
            match comp {
                std::path::Component::Normal(c) => comps.push(c),
                std::path::Component::ParentDir => {
                    comps.pop();
                }
                _ => {}
            }
        }
        candidate = root.join(comps.iter().collect::<PathBuf>());
    }

    if candidate.strip_prefix(root).is_ok() {
        candidate
    } else {
        root.join(ref_str)
    }
}

fn detect_git_commit(directory: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(directory)
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// Bundle a project according to the supplied options.
pub fn bundle(options: &BundleOptions, logger: &Logger) -> Result<BundleOutput, BundleError> {
    bundle_with_loader(options, logger, |paths, root, options| {
        Ok(paths
            .iter()
            .map(|path| LoadedFile {
                meta: options
                    .include_meta
                    .then(|| collect_metadata(path, root, options.include_meta_hash).ok())
                    .flatten(),
                content: fs::read_to_string(path).ok(),
            })
            .collect())
    })
}

#[derive(Serialize, Deserialize)]
struct LoadedFile {
    meta: Option<FileMetadata>,
    content: Option<String>,
}

fn bundle_with_loader(
    options: &BundleOptions,
    logger: &Logger,
    mut load: impl FnMut(&[PathBuf], &Path, &BundleOptions) -> Result<Vec<LoadedFile>, BundleError>,
) -> Result<BundleOutput, BundleError> {
    let (filter_ext, dep_aware) = parse_filter(options.filter.as_deref())?;
    let root_dir = fs::canonicalize(&options.directory)?;
    let target_commit = options
        .git_commit
        .clone()
        .or_else(|| detect_git_commit(&root_dir));

    // Build file list.
    let mut walker = WalkBuilder::new(&root_dir);
    walker.add_custom_ignore_filename(".boundignore");
    if let Some(dl) = options.depth_limit {
        walker.max_depth(Some(dl));
    }
    let all_files: Vec<PathBuf> = walker
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map_or(false, |ft| ft.is_file()))
        .map(|e| e.into_path())
        .collect();

    let mut files_to_process = HashSet::new();
    let mut files_to_scan_deps = VecDeque::new();

    // Language filter.
    if let Some(ext) = &filter_ext {
        for path in &all_files {
            if path.extension().and_then(|s| s.to_str()) == Some(ext) {
                files_to_process.insert(path.clone());
                if dep_aware {
                    files_to_scan_deps.push_back(path.clone());
                }
            }
        }
    } else {
        files_to_process.extend(all_files.iter().cloned());
    }

    // Resolve dependencies.
    if dep_aware {
        let mut visited = HashSet::new();
        while let Some(path) = files_to_scan_deps.pop_front() {
            if !visited.insert(path.clone()) {
                continue;
            }
            for r in parse_references_generic(&path)? {
                let candidate = resolve_ref_path(&path, &r, &root_dir);
                if candidate.exists() && !files_to_process.contains(&candidate) {
                    files_to_process.insert(candidate.clone());
                    files_to_scan_deps.push_back(candidate);
                }
            }
        }
    }

    // Sort files for deterministic output.
    let mut sorted_files: Vec<PathBuf> = files_to_process.into_iter().collect();
    sorted_files.sort();

    // Generate tree.
    let tree = if options.include_tree && sorted_files.len() > 1 {
        Some(generate_tree(&root_dir, &sorted_files))
    } else {
        None
    };

    // Build redaction engine once for all files.
    let redaction_engine = options
        .redaction
        .as_ref()
        .map(|opts| build_redaction_engine(opts, logger))
        .transpose()?;

    let mut entries = Vec::new();
    let mut total_bytes: usize = 0;
    let mut total_lines: usize = 0;
    let mut files_redacted: usize = 0;
    let mut total_replacements: usize = 0;

    for paths in sorted_files.chunks(32) {
        for (path, loaded) in paths.iter().zip(load(paths, &root_dir, options)?) {
            let meta = loaded.meta;
            let content = match loaded.content {
                Some(c) => c,
                None => continue,
            };

            let mut processed_content = content.clone();
            let mut redactions: Option<usize> = None;
            if let Some(engine) = &redaction_engine {
                let (redacted, count) = engine.apply(&processed_content);
                processed_content = redacted;
                if count > 0 {
                    redactions = Some(count);
                    files_redacted += 1;
                    total_replacements += count;
                }
            }
            if let Some(tl) = options.token_limit {
                processed_content = processed_content
                    .split_whitespace()
                    .take(tl)
                    .collect::<Vec<&str>>()
                    .join(" ");
            }
            if let Some(sl) = options.size_limit {
                if processed_content.len() > sl {
                    processed_content.truncate(sl);
                }
            }

            let furnace_report = if options.include_furnace {
                meta.as_ref().map(|m| analyze_file(path, m))
            } else {
                None
            };

            let relative_path = path
                .strip_prefix(&root_dir)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();

            let size_bytes = meta
                .as_ref()
                .map(|m| m.size_bytes)
                .unwrap_or_else(|| fs::metadata(path).map(|m| m.len()).unwrap_or(0));
            let lines = meta
                .as_ref()
                .map(|m| m.line_count)
                .unwrap_or_else(|| processed_content.lines().count());
            let modified_unix = meta.as_ref().map(|m| m.modified_unix).unwrap_or(0);
            let sha256 = meta.as_ref().and_then(|m| m.sha256.clone());

            total_bytes += size_bytes as usize;
            total_lines += lines;

            entries.push(FileEntry {
                path: path.clone(),
                relative_path,
                language: path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string()),
                size_bytes,
                lines,
                modified_unix,
                sha256,
                content: Some(processed_content),
                dependencies: Vec::new(),
                furnace_report,
                redactions,
            });
        }
    }

    let mut snapshot = Snapshot {
        schema: "bound-snapshot".to_string(),
        schema_version: "1.0.0".to_string(),
        producer: "bound".to_string(),
        producer_version: env!("CARGO_PKG_VERSION").to_string(),
        target: root_dir,
        target_commit,
        generated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string(),
        filter: FilterSpec {
            extension: filter_ext,
            dependency_aware: dep_aware,
        },
        limits: Limits {
            token: options.token_limit,
            size: options.size_limit,
            depth: options.depth_limit,
        },
        tree,
        files: entries,
        summary: Summary {
            file_count: sorted_files.len(),
            total_bytes,
            total_lines,
        },
        redaction_stats: redaction_engine.as_ref().map(|engine| RedactionStats {
            rules_loaded: engine.rule_count(),
            files_redacted,
            total_replacements,
            path_replacements: None,
        }),
    };

    // Optionally redact paths and metadata in the snapshot output.
    if let (Some(engine), Some(redaction)) = (&redaction_engine, &options.redaction) {
        if redaction.redact_paths {
            let path_replacements = redact_paths_in_snapshot(engine, &mut snapshot);
            if let Some(stats) = &mut snapshot.redaction_stats {
                stats.path_replacements = Some(path_replacements);
            }
        }
    }

    Ok(BundleOutput {
        snapshot,
        text: None,
    })
}

/// Render a snapshot into the legacy expandable-block text format.
pub fn render_text(snapshot: &Snapshot) -> String {
    let mut out = String::new();

    if let Some(tree) = &snapshot.tree {
        out.push_str(&wrap_expandable("tree", tree));
        out.push_str("\n\n");
    }

    for file in &snapshot.files {
        let mut block = String::new();
        if let Some(meta) = file_metadata_for_entry(file) {
            block.push_str(&wrap_expandable("metadata", &meta.to_header()));
        }
        if let Some(content) = &file.content {
            block.push_str(content);
            block.push_str("\n\n");
        }
        if let Some(report) = &file.furnace_report {
            block.push_str(&report.render());
            block.push_str("\n\n");
        }
        out.push_str(&wrap_expandable("file", &block));
        out.push_str("\n\n");
    }

    out
}

fn file_metadata_for_entry(entry: &FileEntry) -> Option<FileMetadata> {
    Some(FileMetadata {
        relative_path: entry.relative_path.clone(),
        size_bytes: entry.size_bytes,
        line_count: entry.lines,
        modified_unix: entry.modified_unix,
        sha256: entry.sha256.clone(),
    })
}
