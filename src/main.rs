use std::collections::HashSet;
use std::env;
use std::fs::{self, File};
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, Duration};
use regex::Regex;
use arboard::Clipboard;

/// Telemetry struct for tracking progress
struct Telemetry {
    files_processed: usize,
    bytes_read: usize,
    tokens_aggregated: usize,
    start_time: Instant,
}

impl Telemetry {
    fn new() -> Self {
        Telemetry {
            files_processed: 0,
            bytes_read: 0,
            tokens_aggregated: 0,
            start_time: Instant::now(),
        }
    }

    fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    fn ebt(&self, total_files: usize) -> Option<Duration> {
        if self.files_processed == 0 { return None; }
        let remaining_files = total_files.saturating_sub(self.files_processed);
        let avg_per_file = self.elapsed().as_secs_f64() / self.files_processed as f64;
        Some(Duration::from_secs_f64(avg_per_file * remaining_files as f64))
    }

    fn report(&self, total_files: usize) {
        let ebt_str = self.ebt(total_files)
            .map(|d| format!("{:.1}s", d.as_secs_f64()))
            .unwrap_or("--".to_string());

        let progress = if total_files > 0 {
            let percent = (self.files_processed as f64 / total_files as f64) * 100.0;
            format!("{:>3.0}%", percent)
        } else {
            "--%".to_string()
        };

        println!(
            "[{} | Files: {} | Bytes: {} | Tokens: {} | EBT: {}]",
            progress,
            self.files_processed,
            self.bytes_read,
            self.tokens_aggregated,
            ebt_str
        );
    }
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: bound <filter> <directory> [-tl N] [-sl N] [-dl N] [--out <file>]");
        return Ok(());
    }

    // Parse language filter and target directory
    let mut lang_filter: Option<(String, bool)> = None; // (extension, dependency-aware)
    let target_dir_arg = if args[1].starts_with('[') && args[1].ends_with(']') {
        lang_filter = Some((args[1][1..args[1].len() - 1].to_string(), false));
        args.get(2).cloned()
    } else if args[1].starts_with('{') && args[1].ends_with('}') {
        lang_filter = Some((args[1][1..args[1].len() - 1].to_string(), true));
        args.get(2).cloned()
    } else {
        args.get(1).cloned()
    };

    let target_dir_string = match target_dir_arg {
        Some(d) => d,
        None => { eprintln!("Target directory not specified."); return Ok(()); }
    };
    let target_dir = Path::new(&target_dir_string);

    // Parse flags
    let mut token_limit: Option<usize> = None;
    let mut size_limit: Option<usize> = None;
    let mut depth_limit: Option<usize> = None;
    let mut output_file: Option<String> = None;

    let mut i = if lang_filter.is_some() { 3 } else { 2 };
    while i < args.len() {
        match args[i].as_str() {
            "-tl" => { i += 1; token_limit = args.get(i).and_then(|v| v.parse::<usize>().ok()); },
            "-sl" => { i += 1; size_limit = args.get(i).and_then(|v| v.parse::<usize>().ok()); },
            "-dl" => { i += 1; depth_limit = args.get(i).and_then(|v| v.parse::<usize>().ok()); },
            "--out" => { i += 1; output_file = args.get(i).cloned(); },
            _ => {}
        }
        i += 1;
    }

    // Pre-scan total files for accurate EBT
    let total_files = count_files(target_dir, lang_filter.clone(), depth_limit)?;

    let mut aggregated = String::new();
    let mut visited_files = HashSet::new();
    let mut telemetry = Telemetry::new();

    process_dir(
        target_dir,
        0,
        &mut aggregated,
        &mut visited_files,
        lang_filter.clone(),
        token_limit,
        size_limit,
        depth_limit,
        target_dir,
        &mut telemetry,
        total_files,
    )?;

    if let Some(file_path) = output_file {
        let mut f = File::create(file_path)?;
        writeln!(f, "{}", aggregated)?;
        println!("Output written to file.");
    } else {
        let mut clipboard = Clipboard::new().expect("Failed to access clipboard");
        clipboard.set_text(aggregated).expect("Failed to write to clipboard");
        println!("Output copied to clipboard.");
    }

    Ok(())
}

/// Recursive processing of directories
fn process_dir(
    path: &Path,
    current_depth: usize,
    aggregated: &mut String,
    visited_files: &mut HashSet<PathBuf>,
    lang_filter: Option<(String, bool)>,
    token_limit: Option<usize>,
    size_limit: Option<usize>,
    depth_limit: Option<usize>,
    root_dir: &Path,
    telemetry: &mut Telemetry,
    total_files: usize,
) -> io::Result<()> {
    if let Some(dl) = depth_limit { if current_depth > dl { return Ok(()); } }

    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            process_dir(path.as_path(), current_depth + 1, aggregated, visited_files,
                lang_filter.clone(), token_limit, size_limit, depth_limit, root_dir,
                telemetry, total_files)?;
        }
    } else if path.is_file() {
        process_file(path, aggregated, visited_files, lang_filter.clone(),
            token_limit, size_limit, root_dir, telemetry, total_files)?;
    }

    Ok(())
}

/// Process a single file
fn process_file(
    path: &Path,
    aggregated: &mut String,
    visited_files: &mut HashSet<PathBuf>,
    lang_filter: Option<(String, bool)>,
    token_limit: Option<usize>,
    size_limit: Option<usize>,
    root_dir: &Path,
    telemetry: &mut Telemetry,
    total_files: usize,
) -> io::Result<()> {
    if visited_files.contains(path) { return Ok(()); }

    let mut include_file = true;
    if let Some((ref ext, dep)) = lang_filter {
        include_file = path.extension()
                           .and_then(|e| e.to_str())
                           .map(|e| e == ext)
                           .unwrap_or(false);

        if dep && include_file {
            for ref_path in parse_references_generic(path)? {
                let candidate = path.parent().unwrap_or(root_dir).join(&ref_path);
                let candidate = canonicalize_path(&candidate, root_dir);
                if candidate.exists() {
                    process_file(&candidate, aggregated, visited_files, lang_filter.clone(),
                                 token_limit, size_limit, root_dir, telemetry, total_files)?;
                }
            }
        }
    }

    if include_file {
        let mut file = File::open(path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        telemetry.bytes_read += buffer.len();

        let mut contents = String::from_utf8_lossy(&buffer).to_string();

        // Token limit handling
        if let Some(tl) = token_limit {
            let mut words: Vec<String> = contents.split_whitespace().map(|s| s.to_string()).collect();
            telemetry.tokens_aggregated += words.len();
            words.truncate(tl);
            contents = words.join(" ");
        } else {
            telemetry.tokens_aggregated += contents.split_whitespace().count();
        }

        // Size limit
        if let Some(sl) = size_limit {
            if contents.len() > sl { contents.truncate(sl); }
        }

        aggregated.push_str(&contents);
        aggregated.push('\n');
        visited_files.insert(path.to_path_buf());
        telemetry.files_processed += 1;

        // Progress update every 10 files or at end
        if telemetry.files_processed % 10 == 0 || telemetry.files_processed == total_files {
            telemetry.report(total_files);
        }
    }

    Ok(())
}

/// Count total files for EBT estimation
fn count_files(path: &Path, lang_filter: Option<(String,bool)>, depth_limit: Option<usize>) -> io::Result<usize> {
    fn inner(path: &Path, lang_filter: &Option<(String,bool)>, current_depth: usize, depth_limit: Option<usize>) -> io::Result<usize> {
        if let Some(dl) = depth_limit { if current_depth > dl { return Ok(0); } }

        let mut count = 0;
        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                count += inner(entry.path().as_path(), lang_filter, current_depth + 1, depth_limit)?;
            }
        } else if path.is_file() {
            if let Some((ref ext, _)) = lang_filter {
                if path.extension().and_then(|e| e.to_str()).map(|e| e == ext).unwrap_or(true) {
                    count += 1;
                }
            } else {
                count += 1;
            }
        }
        Ok(count)
    }

    inner(path, &lang_filter, 0, depth_limit)
}

/// Parse references in code (Python, JS, C/C++)
fn parse_references_generic(path: &Path) -> io::Result<Vec<String>> {
    let file = File::open(path)?;
    let reader = io::BufReader::new(file);
    let mut references = Vec::new();

    let patterns = vec![
        r#"(?m)^\s*import\s+([a-zA-Z0-9_\.]+)"#,
        r#"(?m)^\s*from\s+([a-zA-Z0-9_\.]+)\s+import"#,
        r#"require\(['"](.+?)['"]\)"#,
        r#"(?m)^\s*import\s+.*\s+from\s+['"](.+?)['"]"#,
        r#"(?m)^\s*#include\s*["<](.+?)["<]"#,
    ];

    for line in reader.lines() {
        let line = line?;
        for pat in &patterns {
            let re = Regex::new(pat).unwrap();
            for cap in re.captures_iter(&line) {
                if let Some(m) = cap.get(1) {
                    let mut r = m.as_str().to_string();
                    if path.extension().map(|e| e == "py" || e == "js").unwrap_or(false) {
                        r = r.replace('.', "/");
                    }
                    if !r.contains('.') {
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            r = format!("{}.{}", r, ext);
                        }
                    }
                    references.push(r);
                }
            }
        }
    }

    Ok(references)
}

/// Canonicalize a path, staying inside root_dir
fn canonicalize_path(path: &Path, root_dir: &Path) -> PathBuf {
    let path = if path.exists() { fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()) } else { path.to_path_buf() };
    if let Ok(rel) = path.strip_prefix(root_dir) {
        root_dir.join(rel)
    } else { path }
}
