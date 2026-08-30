//! redaction.rs
//!
//! Content redaction for `bound` snapshots. Supports explicit regex rules and
//! forbidden-string stores in CSV, SQLite, local Padagonia exports, and live
//! Padagonia HTTP queries. Redaction can also be configured via a TOML config
//! file and applied to file paths and metadata headers.

use std::collections::hash_map::DefaultHasher;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use regex::{escape, Regex};
use serde::{Deserialize, Serialize};

use crate::{BundleError, Logger};

/// A single redaction rule.
#[derive(Debug, Clone)]
pub enum RedactionRule {
    /// A raw regex pattern applied as-is.
    Regex(Regex),
    /// A literal string that is escaped before being turned into a regex.
    Literal(String),
}

impl RedactionRule {
    /// Compile the rule into a regex that can be applied to content.
    pub fn compile(&self) -> Result<Regex, regex::Error> {
        match self {
            RedactionRule::Regex(re) => Ok(re.clone()),
            RedactionRule::Literal(s) => Regex::new(&escape(s)),
        }
    }
}

/// TOML-configurable representation of redaction options. Nested under a
/// `[redaction]` table (or used as the top-level object when the config file
/// is dedicated to redaction).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RedactionConfig {
    #[serde(default)]
    pub replacement: Option<String>,
    #[serde(default)]
    pub regex: Vec<String>,
    #[serde(default, rename = "regex_file")]
    pub regex_file: Option<PathBuf>,
    #[serde(default, rename = "csv")]
    pub csv_file: Option<PathBuf>,
    #[serde(default, rename = "sqlite")]
    pub sqlite_file: Option<PathBuf>,
    #[serde(default, rename = "sqlite_query")]
    pub sqlite_query: Option<String>,
    #[serde(default, rename = "padagonia_file")]
    pub padagonia_file: Option<PathBuf>,
    #[serde(default)]
    pub redact_paths: Option<bool>,
    #[serde(default)]
    pub padagonia: Option<PadagoniaConfig>,
}

/// TOML representation of Padagonia-specific options.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PadagoniaConfig {
    pub url: Option<String>,
    pub token: Option<String>,
    pub namespace: Option<String>,
    pub label: Option<String>,
    pub property: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub cache_ttl: Option<u64>,
    #[serde(default)]
    pub retries: Option<usize>,
}

/// Options describing where forbidden strings and regex rules come from.
#[derive(Debug, Clone, Default)]
pub struct RedactionOptions {
    /// Explicit regex patterns supplied on the command line.
    pub regex_patterns: Vec<String>,
    /// Path to a file containing one regex pattern per line.
    pub regex_file: Option<PathBuf>,
    /// Path to a CSV file whose first column contains forbidden strings.
    pub csv_file: Option<PathBuf>,
    /// Path to a SQLite database file.
    pub sqlite_file: Option<PathBuf>,
    /// SELECT query run against `sqlite_file`; the first text column is read.
    pub sqlite_query: Option<String>,
    /// Path to a Padagonia export (CSV or JSONL) containing forbidden strings.
    pub padagonia_file: Option<PathBuf>,
    /// Base URL of a live Padagonia API (e.g. `http://127.0.0.1:7373`).
    pub padagonia_url: Option<String>,
    /// Bearer token for authenticating with the live Padagonia API.
    pub padagonia_token: Option<String>,
    /// Namespace to scope the live Padagonia query.
    pub padagonia_namespace: Option<String>,
    /// Node label used for forbidden-string nodes in Padagonia.
    pub padagonia_label: Option<String>,
    /// Property name on Padagonia nodes that holds the forbidden string.
    pub padagonia_property: Option<String>,
    /// Maximum number of forbidden strings to fetch from Padagonia.
    pub padagonia_limit: usize,
    /// Cache TTL in seconds for Padagonia forbidden strings. `0` disables caching.
    pub padagonia_cache_ttl: u64,
    /// Number of retries for transient Padagonia failures.
    pub padagonia_retries: usize,
    /// String used to replace every match.
    pub replacement: String,
    /// When true, also redact file paths and metadata headers.
    pub redact_paths: bool,
}

impl RedactionOptions {
    /// Create options with sensible defaults.
    pub fn new() -> Self {
        Self {
            replacement: "[REDACTED]".to_string(),
            padagonia_limit: 10_000,
            padagonia_cache_ttl: 3_600,
            padagonia_retries: 3,
            ..Default::default()
        }
    }

    /// Load redaction options from a TOML config file. The file may either be a
    /// dedicated redaction config (keys at top level) or contain a `[redaction]`
    /// table.
    pub fn from_config(path: &Path) -> Result<Self, BundleError> {
        let contents = std::fs::read_to_string(path).map_err(BundleError::Io)?;
        let config: RedactionConfig = toml::from_str(&contents).map_err(|e| {
            BundleError::InvalidFilter(format!(
                "failed to parse redaction config {}: {}",
                path.display(),
                e
            ))
        })?;
        Ok(Self::from(config))
    }

    /// Merge another `RedactionOptions` on top of this one, with the other
    /// options taking precedence for any non-default value.
    pub fn merge(&mut self, other: &RedactionOptions) {
        if !other.regex_patterns.is_empty() {
            self.regex_patterns.clone_from(&other.regex_patterns);
        }
        if other.regex_file.is_some() {
            self.regex_file.clone_from(&other.regex_file);
        }
        if other.csv_file.is_some() {
            self.csv_file.clone_from(&other.csv_file);
        }
        if other.sqlite_file.is_some() {
            self.sqlite_file.clone_from(&other.sqlite_file);
        }
        if other.sqlite_query.is_some() {
            self.sqlite_query.clone_from(&other.sqlite_query);
        }
        if other.padagonia_file.is_some() {
            self.padagonia_file.clone_from(&other.padagonia_file);
        }
        if other.padagonia_url.is_some() {
            self.padagonia_url.clone_from(&other.padagonia_url);
        }
        if other.padagonia_token.is_some() {
            self.padagonia_token.clone_from(&other.padagonia_token);
        }
        if other.padagonia_namespace.is_some() {
            self.padagonia_namespace
                .clone_from(&other.padagonia_namespace);
        }
        if other.padagonia_label.is_some() {
            self.padagonia_label.clone_from(&other.padagonia_label);
        }
        if other.padagonia_property.is_some() {
            self.padagonia_property
                .clone_from(&other.padagonia_property);
        }
        if other.padagonia_limit != 10_000 {
            self.padagonia_limit = other.padagonia_limit;
        }
        if other.padagonia_cache_ttl != 3_600 {
            self.padagonia_cache_ttl = other.padagonia_cache_ttl;
        }
        if other.padagonia_retries != 3 {
            self.padagonia_retries = other.padagonia_retries;
        }
        if !other.replacement.is_empty() && other.replacement != "[REDACTED]" {
            self.replacement.clone_from(&other.replacement);
        }
        if other.redact_paths {
            self.redact_paths = other.redact_paths;
        }
    }

    /// Return true if no redaction source is configured.
    pub fn is_empty(&self) -> bool {
        self.regex_patterns.is_empty()
            && self.regex_file.is_none()
            && self.csv_file.is_none()
            && self.sqlite_file.is_none()
            && self.padagonia_file.is_none()
            && self.padagonia_url.is_none()
    }

    /// Return true if a live Padagonia query is configured.
    pub fn has_live_padagonia(&self) -> bool {
        self.padagonia_url.is_some() && self.padagonia_namespace.is_some()
    }
}

impl From<RedactionConfig> for RedactionOptions {
    fn from(config: RedactionConfig) -> Self {
        let mut opts = RedactionOptions::new();
        if let Some(r) = config.replacement {
            opts.replacement = r;
        }
        opts.regex_patterns = config.regex;
        opts.regex_file = config.regex_file;
        opts.csv_file = config.csv_file;
        opts.sqlite_file = config.sqlite_file;
        opts.sqlite_query = config.sqlite_query;
        opts.padagonia_file = config.padagonia_file;
        if let Some(p) = config.redact_paths {
            opts.redact_paths = p;
        }
        if let Some(p) = config.padagonia {
            opts.padagonia_url = p.url;
            opts.padagonia_token = p.token;
            opts.padagonia_namespace = p.namespace;
            opts.padagonia_label = p.label;
            opts.padagonia_property = p.property;
            if let Some(l) = p.limit {
                opts.padagonia_limit = l;
            }
            if let Some(t) = p.cache_ttl {
                opts.padagonia_cache_ttl = t;
            }
            if let Some(r) = p.retries {
                opts.padagonia_retries = r;
            }
        }
        opts
    }
}

/// Compiled redaction engine that can scrub arbitrary text.
#[derive(Debug, Clone)]
pub struct RedactionEngine {
    patterns: Vec<Regex>,
    replacement: String,
}

impl RedactionEngine {
    /// Build an engine from a list of rules and a replacement token.
    pub fn new(rules: Vec<RedactionRule>, replacement: &str) -> Result<Self, BundleError> {
        let mut patterns = Vec::with_capacity(rules.len());
        for rule in rules {
            patterns.push(rule.compile().map_err(|e| {
                BundleError::InvalidFilter(format!("failed to compile redaction rule: {}", e))
            })?);
        }
        Ok(Self {
            patterns,
            replacement: replacement.to_string(),
        })
    }

    /// Return the number of loaded patterns.
    pub fn rule_count(&self) -> usize {
        self.patterns.len()
    }

    /// Apply every loaded pattern to `text` and return the scrubbed string along
    /// with the total number of replacements performed.
    pub fn apply(&self, text: &str) -> (String, usize) {
        let mut total = 0;
        let mut out = text.to_string();
        for pattern in &self.patterns {
            let count = pattern.find_iter(&out).count();
            if count > 0 {
                out = pattern.replace_all(&out, &self.replacement).into_owned();
                total += count;
            }
        }
        (out, total)
    }
}

/// Build a redaction engine from the supplied options.
pub fn build_redaction_engine(
    options: &RedactionOptions,
    logger: &Logger,
) -> Result<RedactionEngine, BundleError> {
    let mut rules: Vec<RedactionRule> = Vec::new();

    for pattern in &options.regex_patterns {
        rules.push(RedactionRule::Regex(Regex::new(pattern).map_err(|e| {
            BundleError::InvalidFilter(format!("invalid redaction regex '{}': {}", pattern, e))
        })?));
    }

    if let Some(path) = &options.regex_file {
        for line in read_lines(path)? {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            rules.push(RedactionRule::Regex(Regex::new(trimmed).map_err(|e| {
                BundleError::InvalidFilter(format!(
                    "invalid redaction regex in {}: {}",
                    path.display(),
                    e
                ))
            })?));
        }
    }

    if let Some(path) = &options.csv_file {
        for value in load_csv(path)? {
            rules.push(RedactionRule::Literal(value));
        }
    }

    if let (Some(db_path), Some(query)) = (&options.sqlite_file, &options.sqlite_query) {
        for value in load_sqlite(db_path, query)? {
            rules.push(RedactionRule::Literal(value));
        }
    }

    if let Some(path) = &options.padagonia_file {
        for value in load_padagonia(path)? {
            rules.push(RedactionRule::Literal(value));
        }
    }

    if options.has_live_padagonia() {
        match padagonia_client::query_padagonia(options, logger) {
            Ok(values) => {
                for value in values {
                    rules.push(RedactionRule::Literal(value));
                }
            }
            Err(e) => {
                logger.warn(&format!(
                    "Failed to load forbidden strings from Padagonia: {}. Continuing without them.",
                    e
                ));
            }
        }
    }

    RedactionEngine::new(rules, &options.replacement)
}

/// Apply the redaction engine to snapshot-level path-like strings. Returns the
/// number of replacements performed.
pub fn redact_paths_in_snapshot(engine: &RedactionEngine, snapshot: &mut crate::Snapshot) -> usize {
    let mut total = 0;
    for file in &mut snapshot.files {
        let (redacted, count) = engine.apply(&file.relative_path);
        if count > 0 {
            file.relative_path = redacted;
            total += count;
        }
        let path_str = file.path.to_string_lossy().to_string();
        let (redacted_path, count) = engine.apply(&path_str);
        if count > 0 {
            file.path = PathBuf::from(redacted_path);
            total += count;
        }
    }
    if let Some(tree) = &mut snapshot.tree {
        let (redacted, count) = engine.apply(tree);
        if count > 0 {
            *tree = redacted;
            total += count;
        }
    }
    total
}

fn read_lines(path: &Path) -> Result<impl Iterator<Item = String>, BundleError> {
    let file = File::open(path).map_err(BundleError::Io)?;
    Ok(BufReader::new(file).lines().map_while(Result::ok))
}

fn load_csv(path: &Path) -> Result<Vec<String>, BundleError> {
    let file = File::open(path).map_err(BundleError::Io)?;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(file);
    let mut values = Vec::new();
    for result in reader.records() {
        let record = result.map_err(|e| BundleError::Io(std::io::Error::other(e)))?;
        if let Some(value) = record.get(0) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                values.push(trimmed.to_string());
            }
        }
    }
    Ok(values)
}

fn load_sqlite(db_path: &Path, query: &str) -> Result<Vec<String>, BundleError> {
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| BundleError::Io(std::io::Error::other(e)))?;
    let mut stmt = conn
        .prepare(query)
        .map_err(|e| BundleError::Io(std::io::Error::other(e)))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| BundleError::Io(std::io::Error::other(e)))?;
    let mut values = Vec::new();
    for row in rows {
        let value = row.map_err(|e| BundleError::Io(std::io::Error::other(e)))?;
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            values.push(trimmed.to_string());
        }
    }
    Ok(values)
}

/// Padagonia export loader. Supports:
/// - `.csv`: first column treated as the forbidden string.
/// - `.jsonl`: each line parsed as JSON; the `value` field is used, or the first
///   string field found in the object.
fn load_padagonia(path: &Path) -> Result<Vec<String>, BundleError> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext == "csv" {
        return load_csv(path);
    }

    let mut values = Vec::new();
    for line in read_lines(path)? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value = if let Ok(obj) =
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(trimmed)
        {
            if let Some(serde_json::Value::String(s)) = obj.get("value") {
                s.clone()
            } else {
                obj.values()
                    .find_map(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_default()
            }
        } else {
            trimmed.to_string()
        };
        let value = value.trim();
        if !value.is_empty() {
            values.push(value.to_string());
        }
    }
    Ok(values)
}

/// Live Padagonia query support with caching and retries.
mod padagonia_client {
    use super::*;

    const DEFAULT_PADAGONIA_URL: &str = "http://127.0.0.1:7373";

    #[derive(Debug, Clone, Serialize)]
    struct QueryNodesRequest<'a> {
        namespace: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<&'a str>,
        limit: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        cursor: Option<&'a str>,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct PageCursorResponse {
        #[serde(default)]
        nodes: Vec<NodeRecordResponse>,
        #[serde(default)]
        next_cursor: Option<String>,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct NodeRecordResponse {
        #[serde(default)]
        properties: serde_json::Map<String, serde_json::Value>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub(crate) struct CacheEntry {
        pub(crate) fetched_at: u64,
        pub(crate) values: Vec<String>,
    }

    /// Query Padagonia for forbidden strings, using a local cache when available.
    pub fn query_padagonia(
        options: &RedactionOptions,
        logger: &Logger,
    ) -> Result<Vec<String>, BundleError> {
        let cache_path = if options.padagonia_cache_ttl > 0 {
            cache_path(options)
        } else {
            None
        };

        if let Some(path) = &cache_path {
            if let Some(entry) = load_cache(path, options.padagonia_cache_ttl) {
                logger.info(&format!(
                    "Loaded {} forbidden strings from Padagonia cache ({})",
                    entry.values.len(),
                    path.display()
                ));
                return Ok(entry.values);
            }
        }

        let url = options
            .padagonia_url
            .as_deref()
            .unwrap_or(DEFAULT_PADAGONIA_URL)
            .trim_end_matches('/');
        let namespace = options.padagonia_namespace.as_deref().ok_or_else(|| {
            BundleError::InvalidFilter(
                "--redact-padagonia-namespace is required for live Padagonia queries".to_string(),
            )
        })?;
        let label = options.padagonia_label.as_deref();
        let property = options.padagonia_property.as_deref().unwrap_or("value");
        let page_size = options.padagonia_limit.clamp(1, 1000);
        let max_total = options.padagonia_limit;

        let mut values = Vec::new();
        let mut cursor: Option<String> = None;
        let mut fetched = 0;

        loop {
            if fetched >= max_total {
                break;
            }

            let request = QueryNodesRequest {
                namespace,
                label,
                limit: page_size.min(max_total - fetched),
                cursor: cursor.as_deref(),
            };

            let body = serde_json::to_value(&request)
                .map_err(|e| BundleError::Io(std::io::Error::other(e)))?;
            let response = send_with_retries(
                &format!("{}/api/v1/query/nodes", url),
                body,
                options.padagonia_token.as_deref(),
                options.padagonia_retries,
            );

            let response = match response {
                Ok(r) => r,
                Err(ureq::Error::Status(code, r)) => {
                    let body = r.into_string().unwrap_or_default();
                    return Err(BundleError::InvalidFilter(format!(
                        "Padagonia query failed with HTTP {}: {}",
                        code, body
                    )));
                }
                Err(ureq::Error::Transport(e)) => {
                    return Err(BundleError::Io(std::io::Error::other(e)));
                }
            };

            let body = response
                .into_string()
                .map_err(|e| BundleError::Io(std::io::Error::other(e)))?;
            let (page_values, next_cursor) = parse_page_response(&body, property)?;

            fetched += page_values.len();
            for value in page_values {
                values.push(value);
            }
            cursor = next_cursor;
            if cursor.is_none() {
                break;
            }
        }

        if let Some(path) = cache_path {
            if let Err(e) = write_cache(&path, &values) {
                logger.warn(&format!(
                    "Failed to write Padagonia cache to {}: {}. Continuing.",
                    path.display(),
                    e
                ));
            }
        }

        logger.info(&format!(
            "Loaded {} forbidden strings from Padagonia at {}{}",
            values.len(),
            url,
            label
                .map(|l| format!(" (label: {})", l))
                .unwrap_or_default()
        ));

        Ok(values)
    }

    pub(crate) fn send_with_retries(
        endpoint: &str,
        body: serde_json::Value,
        token: Option<&str>,
        retries: usize,
    ) -> Result<ureq::Response, ureq::Error> {
        let mut last_err = None;
        for attempt in 0..=retries {
            let mut req = ureq::post(endpoint)
                .set("Content-Type", "application/json")
                .timeout(Duration::from_secs(30));
            if let Some(t) = token {
                req = req.set("Authorization", &format!("Bearer {}", t));
            }
            match req.send_json(body.clone()) {
                Ok(r) => return Ok(r),
                Err(ureq::Error::Status(code, r)) if code >= 500 => {
                    last_err = Some(ureq::Error::Status(code, r));
                }
                Err(ureq::Error::Transport(e)) => {
                    last_err = Some(ureq::Error::Transport(e));
                }
                Err(e) => return Err(e),
            }
            if attempt < retries {
                let base = Duration::from_millis(500) * 2_u32.pow(attempt as u32);
                let capped = base.min(Duration::from_secs(8));
                let jitter_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos() as u64
                    % 200;
                std::thread::sleep(capped + Duration::from_millis(jitter_ms));
            }
        }
        Err(last_err.expect("retry loop returned without an error"))
    }

    /// Parse a single Padagonia page response body into forbidden strings and the
    /// next cursor. Exposed for unit testing.
    pub(crate) fn parse_page_response(
        body: &str,
        property: &str,
    ) -> Result<(Vec<String>, Option<String>), BundleError> {
        let page: PageCursorResponse =
            serde_json::from_str(body).map_err(|e| BundleError::Io(std::io::Error::other(e)))?;
        let mut values = Vec::new();
        for node in page.nodes {
            if let Some(serde_json::Value::String(s)) = node.properties.get(property) {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    values.push(trimmed.to_string());
                }
            }
        }
        Ok((values, page.next_cursor))
    }

    fn cache_path(options: &RedactionOptions) -> Option<PathBuf> {
        let mut hasher = DefaultHasher::new();
        options.padagonia_url.hash(&mut hasher);
        options.padagonia_namespace.hash(&mut hasher);
        options.padagonia_label.hash(&mut hasher);
        options.padagonia_property.hash(&mut hasher);
        let key = format!("{:016x}", hasher.finish());

        let base = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("."));
        let dir = base.join("bound").join("padagonia-redaction");
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir.join(format!("{}.json", key)))
    }

    pub(crate) fn load_cache(path: &Path, ttl_seconds: u64) -> Option<CacheEntry> {
        let metadata = std::fs::metadata(path).ok()?;
        let modified = metadata.modified().ok()?;
        let age = SystemTime::now()
            .duration_since(modified)
            .unwrap_or_default()
            .as_secs();
        if age > ttl_seconds {
            return None;
        }
        let contents = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    pub(crate) fn write_cache(path: &Path, values: &[String]) -> Result<(), BundleError> {
        let entry = CacheEntry {
            fetched_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            values: values.to_vec(),
        };
        let tmp = path.with_extension("tmp");
        let mut file = File::create(&tmp).map_err(BundleError::Io)?;
        file.write_all(
            serde_json::to_string(&entry)
                .map_err(|e| BundleError::Io(std::io::Error::other(e)))?
                .as_bytes(),
        )
        .map_err(BundleError::Io)?;
        drop(file);
        std::fs::rename(tmp, path).map_err(BundleError::Io)
    }
}

/// Serializable summary of redaction work performed on a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionStats {
    pub rules_loaded: usize,
    pub files_redacted: usize,
    pub total_replacements: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_replacements: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LogLevel;
    use std::io::Write;

    #[test]
    fn regex_redaction() {
        let engine = RedactionEngine::new(
            vec![RedactionRule::Regex(
                Regex::new(r"sk-[a-zA-Z0-9]{10}").unwrap(),
            )],
            "[REDACTED]",
        )
        .unwrap();
        let (out, count) = engine.apply("key=sk-abc123def4 secret");
        assert_eq!(count, 1);
        assert_eq!(out, "key=[REDACTED] secret");
    }

    #[test]
    fn literal_redaction() {
        let engine = RedactionEngine::new(
            vec![RedactionRule::Literal("hunter2".to_string())],
            "[REDACTED]",
        )
        .unwrap();
        let (out, count) = engine.apply("password=hunter2 and hunter2again");
        assert_eq!(count, 2);
        assert_eq!(out, "password=[REDACTED] and [REDACTED]again");
    }

    #[test]
    fn overlapping_rules_replace_all() {
        let engine = RedactionEngine::new(
            vec![
                RedactionRule::Literal("foo".to_string()),
                RedactionRule::Literal("bar".to_string()),
            ],
            "[REDACTED]",
        )
        .unwrap();
        let (out, count) = engine.apply("foo bar baz");
        assert_eq!(count, 2);
        assert_eq!(out, "[REDACTED] [REDACTED] baz");
    }

    #[test]
    fn load_csv_picks_first_column() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "secret-one").unwrap();
        writeln!(tmp, "secret-two,extra").unwrap();
        tmp.flush().unwrap();

        let values = load_csv(tmp.path()).unwrap();
        assert_eq!(values, vec!["secret-one", "secret-two"]);
    }

    #[test]
    fn load_sqlite_query() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let conn = rusqlite::Connection::open(tmp.path()).unwrap();
        conn.execute(
            "CREATE TABLE secrets (id INTEGER PRIMARY KEY, value TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO secrets (value) VALUES (?1), (?2)",
            ["alpha", "beta"],
        )
        .unwrap();
        drop(conn);

        let values = load_sqlite(tmp.path(), "SELECT value FROM secrets ORDER BY id").unwrap();
        assert_eq!(values, vec!["alpha", "beta"]);
    }

    #[test]
    fn load_padagonia_jsonl() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, r#"{{"id":1,"value":"forbidden-one"}}"#).unwrap();
        writeln!(tmp, r#"{{"id":2,"value":"forbidden-two"}}"#).unwrap();
        tmp.flush().unwrap();

        let values = load_padagonia(tmp.path()).unwrap();
        assert_eq!(values, vec!["forbidden-one", "forbidden-two"]);
    }

    #[test]
    fn load_padagonia_csv() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "value").unwrap();
        writeln!(tmp, "csv-secret").unwrap();
        tmp.flush().unwrap();

        let values = load_padagonia(tmp.path()).unwrap();
        assert_eq!(values, vec!["value", "csv-secret"]);
    }

    #[test]
    fn parse_padagonia_page_response() {
        let body = r#"{
            "api_version": "v1",
            "node_ids": [1, 2],
            "nodes": [
                {"id": 1, "label": "ForbiddenString", "properties": {"value": "secret-one"}, "provenance": {"agent": "test", "model": "test"}},
                {"id": 2, "label": "ForbiddenString", "properties": {"value": "secret-two"}, "provenance": {"agent": "test", "model": "test"}}
            ],
            "next_cursor": "cursor-123"
        }"#;
        let (values, cursor) = padagonia_client::parse_page_response(body, "value").unwrap();
        assert_eq!(values, vec!["secret-one", "secret-two"]);
        assert_eq!(cursor, Some("cursor-123".to_string()));
    }

    #[test]
    fn live_padagonia_query_against_mock_server() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).unwrap();
            let response = r#"{
                "api_version": "v1",
                "node_ids": [1],
                "nodes": [
                    {"id": 1, "label": "ForbiddenString", "properties": {"value": "live-secret"}, "provenance": {"agent": "test", "model": "test"}}
                ],
                "next_cursor": null
            }"#;
            let headers = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: ";
            let body = format!("{}{}\r\n\r\n{}", headers, response.len(), response);
            stream.write_all(body.as_bytes()).unwrap();
        });

        let options = RedactionOptions {
            padagonia_url: Some(format!("http://127.0.0.1:{}", port)),
            padagonia_namespace: Some("test-ns".to_string()),
            padagonia_label: Some("ForbiddenString".to_string()),
            padagonia_property: Some("value".to_string()),
            padagonia_limit: 10,
            padagonia_cache_ttl: 0,
            ..RedactionOptions::new()
        };
        let logger = Logger::new(LogLevel::Info, None);
        let values = padagonia_client::query_padagonia(&options, &logger).unwrap();
        assert_eq!(values, vec!["live-secret"]);
    }

    #[test]
    fn config_file_round_trip() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            tmp,
            r#"
replacement = "***"
regex = ["abc"]
csv = "/tmp/secrets.csv"
redact_paths = true

[padagonia]
url = "http://example.com"
namespace = "ns"
label = "ForbiddenString"
property = "value"
limit = 500
cache_ttl = 60
retries = 5
"#
        )
        .unwrap();
        tmp.flush().unwrap();

        let opts = RedactionOptions::from_config(tmp.path()).unwrap();
        assert_eq!(opts.replacement, "***");
        assert_eq!(opts.regex_patterns, vec!["abc"]);
        assert_eq!(opts.csv_file, Some(PathBuf::from("/tmp/secrets.csv")));
        assert!(opts.redact_paths);
        assert_eq!(opts.padagonia_url, Some("http://example.com".to_string()));
        assert_eq!(opts.padagonia_namespace, Some("ns".to_string()));
        assert_eq!(opts.padagonia_limit, 500);
        assert_eq!(opts.padagonia_cache_ttl, 60);
        assert_eq!(opts.padagonia_retries, 5);
    }

    #[test]
    fn merge_options_prefers_other() {
        let mut base = RedactionOptions::new();
        base.regex_patterns = vec!["base".to_string()];
        base.replacement = "BASE".to_string();

        let mut other = RedactionOptions::new();
        other.regex_patterns = vec!["other".to_string()];
        other.csv_file = Some(PathBuf::from("/tmp/other.csv"));
        other.replacement = "OTHER".to_string();

        base.merge(&other);
        assert_eq!(base.regex_patterns, vec!["other"]);
        assert_eq!(base.csv_file, Some(PathBuf::from("/tmp/other.csv")));
        assert_eq!(base.replacement, "OTHER");
    }

    #[test]
    fn padagonia_cache_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("cache.json");
        let values = vec!["one".to_string(), "two".to_string()];

        padagonia_client::write_cache(&cache, &values).unwrap();
        let entry = padagonia_client::load_cache(&cache, 60).unwrap();
        assert_eq!(entry.values, values);
    }

    #[test]
    fn padagonia_cache_expires() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("cache.json");
        let values = vec!["old".to_string()];

        padagonia_client::write_cache(&cache, &values).unwrap();
        // Set modification time far in the past.
        let past = SystemTime::now() - Duration::from_secs(10_000);
        filetime::set_file_mtime(&cache, filetime::FileTime::from_system_time(past)).unwrap();

        assert!(padagonia_client::load_cache(&cache, 60).is_none());
    }

    #[test]
    fn retry_succeeds_after_transient_failure() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        thread::spawn(move || {
            for mut stream in listener.incoming().flatten().take(2) {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).unwrap();
                let count = counter_clone.fetch_add(1, Ordering::SeqCst);
                let (status, body) = if count == 0 {
                    ("503 Service Unavailable", "{}")
                } else {
                    (
                        "200 OK",
                        r#"{"api_version":"v1","nodes":[{"properties":{"value":"retry-secret"}}],"next_cursor":null}"#,
                    )
                };
                let response = format!(
                    "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    status,
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let response = padagonia_client::send_with_retries(
            &format!("http://127.0.0.1:{}/api/v1/query/nodes", port),
            serde_json::json!({"namespace":"ns","limit":10}),
            None,
            3,
        )
        .unwrap();

        let body = response.into_string().unwrap();
        assert!(body.contains("retry-secret"));
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn redact_paths_in_snapshot_works() {
        let engine = RedactionEngine::new(
            vec![RedactionRule::Literal("secret".to_string())],
            "[REDACTED]",
        )
        .unwrap();

        let mut snapshot = crate::Snapshot {
            schema: "bound-snapshot".to_string(),
            schema_version: "1.0.0".to_string(),
            producer: "bound".to_string(),
            producer_version: "0.1.5".to_string(),
            target: PathBuf::from("/project"),
            target_commit: None,
            generated_at: "0".to_string(),
            filter: crate::FilterSpec {
                extension: None,
                dependency_aware: false,
            },
            limits: crate::Limits {
                token: None,
                size: None,
                depth: None,
            },
            tree: Some("secret-folder/\n  secret-file.rs".to_string()),
            files: vec![crate::FileEntry {
                path: PathBuf::from("/project/secret-folder/secret-file.rs"),
                relative_path: "secret-folder/secret-file.rs".to_string(),
                language: Some("rs".to_string()),
                size_bytes: 0,
                lines: 0,
                modified_unix: 0,
                sha256: None,
                content: None,
                dependencies: Vec::new(),
                furnace_report: None,
                redactions: None,
            }],
            summary: crate::Summary {
                file_count: 1,
                total_bytes: 0,
                total_lines: 0,
            },
            redaction_stats: None,
        };

        let count = redact_paths_in_snapshot(&engine, &mut snapshot);
        assert!(count > 0);
        assert!(!snapshot.files[0].relative_path.contains("secret"));
        assert!(!snapshot.files[0].path.to_string_lossy().contains("secret"));
        assert!(snapshot.tree.as_ref().unwrap().contains("[REDACTED]"));
    }
}
