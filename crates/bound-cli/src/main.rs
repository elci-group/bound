mod curly_expand;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use arboard::Clipboard;
use bound_core::{bundle, render_text, BundleOptions, LogLevel, Logger, RedactionOptions};
use clap::Parser;

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

    /// Record a specific Git commit in the snapshot metadata
    #[arg(long)]
    git_commit: Option<String>,

    /// Redaction config file (TOML)
    #[arg(long)]
    redact_config: Option<PathBuf>,

    /// Redaction regex pattern (repeatable)
    #[arg(long)]
    redact_regex: Vec<String>,

    /// File containing redaction regex patterns, one per line
    #[arg(long)]
    redact_regex_file: Option<PathBuf>,

    /// CSV file whose first column contains forbidden strings
    #[arg(long)]
    redact_csv: Option<PathBuf>,

    /// SQLite database file containing forbidden strings
    #[arg(long)]
    redact_sqlite: Option<PathBuf>,

    /// SELECT query for --redact-sqlite; first text column is read
    #[arg(long)]
    redact_sqlite_query: Option<String>,

    /// Padagonia export file (CSV or JSONL) containing forbidden strings
    #[arg(long)]
    redact_padagonia: Option<PathBuf>,

    /// Base URL of a live Padagonia API
    #[arg(long)]
    redact_padagonia_url: Option<String>,

    /// Bearer token for live Padagonia API authentication
    #[arg(long)]
    redact_padagonia_token: Option<String>,

    /// Namespace for live Padagonia queries
    #[arg(long)]
    redact_padagonia_namespace: Option<String>,

    /// Node label for forbidden-string nodes in Padagonia
    #[arg(long)]
    redact_padagonia_label: Option<String>,

    /// Property name holding the forbidden string in Padagonia
    #[arg(long)]
    redact_padagonia_property: Option<String>,

    /// Maximum number of forbidden strings to fetch from Padagonia
    #[arg(long)]
    redact_padagonia_limit: Option<usize>,

    /// Cache TTL in seconds for Padagonia forbidden strings (0 disables caching)
    #[arg(long)]
    redact_padagonia_cache_ttl: Option<u64>,

    /// Number of retries for transient Padagonia failures
    #[arg(long)]
    redact_padagonia_retries: Option<usize>,

    /// Also redact file paths and metadata headers
    #[arg(long)]
    redact_paths: bool,

    /// Replacement string used by redaction
    #[arg(long)]
    redact_replacement: Option<String>,
}

fn __curly_original_main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let logger = Logger::new(LogLevel::Info, None);
    logger.info(&format!("Scanning directory: {}", args.directory.display()));

    let mut redaction = if let Some(config_path) = &args.redact_config {
        Some(RedactionOptions::from_config(config_path)?)
    } else {
        None
    };

    let cli_redaction = RedactionOptions {
        regex_patterns: args.redact_regex,
        regex_file: args.redact_regex_file,
        csv_file: args.redact_csv,
        sqlite_file: args.redact_sqlite,
        sqlite_query: args.redact_sqlite_query,
        padagonia_file: args.redact_padagonia,
        padagonia_url: args.redact_padagonia_url,
        padagonia_token: args.redact_padagonia_token,
        padagonia_namespace: args.redact_padagonia_namespace,
        padagonia_label: args.redact_padagonia_label,
        padagonia_property: args.redact_padagonia_property,
        padagonia_limit: args.redact_padagonia_limit.unwrap_or(10_000),
        padagonia_cache_ttl: args.redact_padagonia_cache_ttl.unwrap_or(3_600),
        padagonia_retries: args.redact_padagonia_retries.unwrap_or(3),
        replacement: args
            .redact_replacement
            .unwrap_or_else(|| "[REDACTED]".to_string()),
        redact_paths: args.redact_paths,
    };

    if let Some(base) = redaction.as_mut() {
        base.merge(&cli_redaction);
    } else if !cli_redaction.is_empty() {
        redaction = Some(cli_redaction);
    }

    let options = BundleOptions {
        directory: args.directory,
        filter: args.filter,
        token_limit: args.token_limit,
        size_limit: args.size_limit,
        depth_limit: args.depth_limit,
        include_meta: args.meta,
        include_meta_hash: args.meta_hash,
        include_tree: args.tree,
        include_furnace: args.furnace,
        git_commit: args.git_commit,
        redaction,
    };

    let output = bundle(&options, &logger)?;

    let aggregated = if args.json {
        serde_json::to_string_pretty(&output.snapshot)?
    } else {
        render_text(&output.snapshot)
    };

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

fn main() {
    let raw_args: Vec<String> = std::env::args().collect();
    let mut positions: Vec<usize> = Vec::new();
    let mut fields: Vec<Vec<String>> = Vec::new();
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--out" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--out=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--out={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--git-commit" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--git-commit=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--git-commit={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--redact-config" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--redact-config=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--redact-config={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--redact-regex" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--redact-regex=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--redact-regex={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--redact-regex-file" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--redact-regex-file=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--redact-regex-file={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--redact-csv" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--redact-csv=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--redact-csv={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--redact-sqlite" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--redact-sqlite=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--redact-sqlite={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--redact-sqlite-query" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--redact-sqlite-query=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--redact-sqlite-query={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--redact-padagonia" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--redact-padagonia=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--redact-padagonia={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--redact-padagonia-url" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--redact-padagonia-url=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--redact-padagonia-url={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--redact-padagonia-token" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--redact-padagonia-token=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--redact-padagonia-token={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--redact-padagonia-namespace" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--redact-padagonia-namespace=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--redact-padagonia-namespace={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--redact-padagonia-label" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--redact-padagonia-label=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--redact-padagonia-label={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--redact-padagonia-property" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--redact-padagonia-property=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--redact-padagonia-property={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--redact-replacement" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--redact-replacement=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--redact-replacement={}", v))
                    .collect(),
            );
            break;
        }
    }

    if fields.is_empty() || fields.iter().all(|f| f.len() <= 1) {
        __curly_original_main();
        return;
    }

    let combos = curly_expand::cartesian(&fields);
    let exe = std::env::current_exe().expect("resolve current exe");
    let mut had_failure = false;
    for combo in &combos {
        let mut new_args = raw_args.clone();
        for (slot, value) in positions.iter().zip(combo.iter()) {
            new_args[*slot] = value.clone();
        }
        let status = std::process::Command::new(&exe)
            .args(&new_args[1..])
            .status()
            .expect("failed to re-exec self");
        if !status.success() {
            had_failure = true;
        }
    }
    if had_failure {
        std::process::exit(1);
    }
}
