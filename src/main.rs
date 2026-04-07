use std::collections::{HashSet, VecDeque};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::io::Write;

use arboard::Clipboard;
use clap::Parser;
use ignore::WalkBuilder;
use once_cell::sync::Lazy;
use regex::Regex;

mod metadata;
mod tree;
mod telemetry;
mod logging;
mod expandable;
mod furnace;

use metadata::{collect_metadata, FileMetadata};
use tree::generate_tree;
use telemetry::Telemetry;
use logging::{Logger, LogLevel};
use expandable::{wrap_expandable, ExpandableBlock};
use furnace::{analyze_file, FurnaceReport};
use serde::Serialize;

#[derive(Serialize)]
struct OutputJson {
    tree: Option<String>,
    files: Vec<FileJson>,
}

#[derive(Serialize)]
struct FileJson {
    metadata: Option<FileMetadata>,
    content: Option<String>,
    furnace_report: Option<FurnaceReport>,
}

static REF_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"(?m)^\s*import\s+([a-zA-Z0-9_\.]+)").unwrap(),
        Regex::new(r"(?m)^\s*from\s+([a-zA-Z0-9_\.]+)\s+import").unwrap(),
        Regex::new(r#"require\(['"](.+?)['"]\)"#).unwrap(),
        Regex::new(r#"(?m)^\s*import\s+.*\s+from\s+['"](.+?)['"]"#).unwrap(),
        Regex::new(r#"(?m)^\s*#include\s*["<](.+?)["<]"#).unwrap(),
    ]
});

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Language filter [.ext] or {.ext}
    #[arg()]
    filter: Option<String>,

    /// Target directory
    #[arg(default_value = ".")]
    directory: PathBuf,

    /// Token limit per file
    #[arg(short = 't', long)]
    token_limit: Option<usize>,

    /// Size limit per file (bytes)
    #[arg(short = 's', long)]
    size_limit: Option<usize>,

    /// Depth limit
    #[arg(short = 'd', long)]
    depth_limit: Option<usize>,

    /// Output file (if not given, clipboard)
    #[arg(long)]
    out: Option<PathBuf>,

    /// Include metadata headers
    #[arg(long)]
    meta: bool,

    /// Include SHA-256 hash in metadata
    #[arg(long)]
    meta_hash: bool,

    /// Include file tree
    #[arg(long)]
    tree: bool,

    /// Enable Furnace analysis
    #[arg(long)]
    furnace: bool,

    /// Output JSON format
    #[arg(long)]
    json: bool,
}

fn parse_filter(filter: Option<&str>) -> Result<(Option<String>, bool), String> {
    match filter {
        None => Ok((None, false)),
        Some(f) if f.starts_with('[') && f.ends_with(']') => Ok((Some(f[1..f.len()-1].to_string()), false)),
        Some(f) if f.starts_with('{') && f.ends_with('}') => Ok((Some(f[1..f.len()-1].to_string()), true)),
        Some(f) => Err(format!("Invalid filter format: '{}'", f)),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let (filter_ext, dep_aware) = parse_filter(args.filter.as_deref())?;
    let mut telemetry = Telemetry::new();
    let root_dir = fs::canonicalize(&args.directory)?;

    let logger = Logger::new(LogLevel::Info, None);
    logger.info(&format!("Scanning directory: {}", root_dir.display()));

    // --- Build file list ---
    let mut walker = WalkBuilder::new(&root_dir);
    walker.add_custom_ignore_filename(".boundignore");
    if let Some(dl) = args.depth_limit {
        walker.max_depth(Some(dl));
    }
    let all_files: Vec<PathBuf> = walker.build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map_or(false, |ft| ft.is_file()))
        .map(|e| e.into_path())
        .collect();

    let mut files_to_process = HashSet::new();
    let mut files_to_scan_deps = VecDeque::new();

    // --- Language filter ---
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

    // --- Resolve dependencies ---
    if dep_aware {
        let mut visited = HashSet::new();
        while let Some(path) = files_to_scan_deps.pop_front() {
            if !visited.insert(path.clone()) { continue; }
            for r in parse_references_generic(&path)? {
                let candidate = resolve_ref_path(&path, &r, &root_dir);
                if candidate.exists() && !files_to_process.contains(&candidate) {
                    files_to_process.insert(candidate.clone());
                    files_to_scan_deps.push_back(candidate);
                }
            }
        }
    }

    // --- Sort files for consistent output ---
    let mut sorted_files: Vec<PathBuf> = files_to_process.into_iter().collect();
    sorted_files.sort();

    let mut aggregated = String::new();
    let mut json_output = if args.json {
        Some(OutputJson {
            tree: None,
            files: Vec::new(),
        })
    } else {
        None
    };

    // --- File tree ---
    if args.tree && sorted_files.len() > 1 {
        let tree_str = generate_tree(&root_dir, &sorted_files);
        if let Some(ref mut j) = json_output {
            j.tree = Some(tree_str);
        } else {
            aggregated.push_str(&wrap_expandable("tree", &tree_str));
            aggregated.push_str("\n\n");
        }
    }

    // --- Process files ---
    let total_files = sorted_files.len();
    for path in &sorted_files {
        let meta = if args.meta {
            Some(collect_metadata(path, &root_dir, args.meta_hash)?)
        } else {
            None
        };

        let content = fs::read_to_string(path)?;
        let mut file_block = String::new();

        if let Some(ref m) = meta {
            file_block.push_str(&wrap_expandable("metadata", &m.to_header()));
        }

        // Apply token/size limits
        let mut processed_content = content.clone();
        if let Some(tl) = args.token_limit {
            processed_content = processed_content
                .split_whitespace()
                .take(tl)
                .collect::<Vec<&str>>()
                .join(" ");
        }
        if let Some(sl) = args.size_limit {
            if processed_content.len() > sl {
                processed_content.truncate(sl);
            }
        }

        let mut file_json = if args.json {
            Some(FileJson {
                metadata: meta.clone(),
                content: Some(processed_content.clone()),
                furnace_report: None,
            })
        } else {
            None
        };

        if !args.json {
            file_block.push_str(&processed_content);
            file_block.push_str("\n\n");
        }

        // Furnace analysis
        if args.furnace {
            if let Some(ref m) = meta {
                let report = analyze_file(path, m);
                if let Some(ref mut j) = file_json {
                    j.furnace_report = Some(report);
                } else {
                    file_block.push_str(&report.render());
                    file_block.push_str("\n\n");
                }
            }
        }

        if let Some(j) = file_json {
            json_output.as_mut().unwrap().files.push(j);
        } else {
            aggregated.push_str(&wrap_expandable("file", &file_block));
        }

        telemetry.files_processed += 1;
        telemetry.bytes_read += content.len();
        telemetry.tokens_aggregated += content.split_whitespace().count();

        if telemetry.files_processed % 10 == 0 || telemetry.files_processed == total_files {
            logger.info(&telemetry.report(total_files));
        }
    }

    if args.json {
        aggregated = serde_json::to_string_pretty(&json_output.unwrap())?;
    }

    // --- Output ---
    if let Some(out_path) = args.out {
        let mut f = File::create(&out_path)?;
        writeln!(f, "{}", aggregated)?;
        logger.info(&format!("Output written to {:?}", out_path));
    } else {
        let mut clipboard = Clipboard::new()?;
        clipboard.set_text(aggregated)?;
        logger.info("Output copied to clipboard.");
    }

    Ok(())
}

/// Parse references generically (Python, JS, C/C++)
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
                if !r.contains('.') { r = format!("{}.{}", r, ext); }
                references.push(r);
            }
        }
    }
    Ok(references)
}

/// Resolve reference path relative to source and root
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
                std::path::Component::ParentDir => { comps.pop(); },
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
