# AGENTS.md

## Project Overview
Bound is a Rust-based CLI utility for recursively aggregating file contents from directories. It supports filtering by language extensions, dependency resolution, limits on tokens/size/depth, and output to clipboard or file. Features include metadata headers, file tree generation, Furnace analysis, and telemetry reporting.

The project is structured as a Cargo crate with binary target. No tests observed.

## Build and Installation
Requires Rust >= 1.70.

To build:
```
git clone https://github.com/&lt;your-username&gt;/bound.git
cd bound
cargo build --release
```

The binary will be at `target/release/bound`.

## Running the Application
Run the built binary with arguments:
```
./target/release/bound [FILTER] [DIRECTORY] [OPTIONS]
```

- FILTER: Optional language filter in `[ext]` or `[.ext]` (exact extension) or `{ext}`/`{.ext}` (extension with dependencies). The dot prefix is optional.
- DIRECTORY: Target directory (defaults to `.`).
- OPTIONS:
  - `-t, --token-limit <N>`: Token limit per file.
  - `-s, --size-limit <N>`: Size limit in bytes per file.
  - `-d, --depth-limit <N>`: Depth limit.
  - `--out <FILE>`: Output to file instead of clipboard.
  - `--meta`: Include metadata headers.
  - `--meta-hash`: Include SHA-256 hash in metadata.
  - `--mesut`: Opt in to Mesut Blocking reads and Compute metadata/hash pipelines; output semantics and the default synchronous backend are unchanged.
  - `--tree`: Include file tree.
  - `--furnace`: Enable Furnace analysis.
  - `--redact-config <FILE>`: TOML config file for redaction rules. CLI flags override config values.
  - `--redact-regex <PATTERN>`: Redaction regex pattern (repeatable).
  - `--redact-regex-file <FILE>`: File containing redaction regex patterns, one per line.
  - `--redact-csv <FILE>`: CSV file whose first column contains forbidden strings.
  - `--redact-sqlite <FILE>`: SQLite database file containing forbidden strings.
  - `--redact-sqlite-query <QUERY>`: SELECT query for `--redact-sqlite`; first text column is read.
  - `--redact-padagonia <FILE>`: Padagonia export file (CSV or JSONL) containing forbidden strings.
  - `--redact-padagonia-url <URL>`: Base URL of a live Padagonia API (default: `http://127.0.0.1:7373`).
  - `--redact-padagonia-token <TOKEN>`: Bearer token for live Padagonia API authentication.
  - `--redact-padagonia-namespace <NAMESPACE>`: Namespace for live Padagonia queries.
  - `--redact-padagonia-label <LABEL>`: Node label for forbidden-string nodes in Padagonia (default: `ForbiddenString`).
  - `--redact-padagonia-property <PROPERTY>`: Property name holding the forbidden string (default: `value`).
  - `--redact-padagonia-limit <N>`: Maximum number of forbidden strings to fetch from Padagonia (default: `10000`).
  - `--redact-padagonia-cache-ttl <SECONDS>`: Cache TTL for Padagonia forbidden strings (default: `3600`; `0` disables caching).
  - `--redact-padagonia-retries <N>`: Number of retries for transient Padagonia failures (default: `3`).
  - `--redact-paths`: Also redact file paths, tree, and metadata headers.
  - `--redact-replacement <TEXT>`: Replacement string used by redaction (default: `[REDACTED]`).

### Redaction config file format
A TOML file passed with `--redact-config` may contain:

```toml
replacement = "[REDACTED]"
regex = ["sk-[a-zA-Z0-9]{20,}"]
regex_file = "/path/to/patterns.txt"
csv = "/path/to/forbidden.csv"
sqlite = "/path/to/secrets.db"
sqlite_query = "SELECT value FROM secrets"
padagonia_file = "/path/to/export.jsonl"
redact_paths = false

[padagonia]
url = "http://127.0.0.1:7373"
token = "..."
namespace = "my-namespace"
label = "ForbiddenString"
property = "value"
limit = 10000
cache_ttl = 3600
retries = 3
```

CLI flags override config values when both are provided.

Standard Cargo commands:
- `cargo build`: Build the project.
- `cargo run -- [ARGS]`: Run with arguments.
- `cargo test`: Run tests (none observed).

## Code Structure
- Source files in `src/` directory.
- Modular design with separate files for functionalities.
- Entry point: `src/main.rs`.

Key modules:
- `main.rs`: Argument parsing, directory walking, file processing, dependency resolution, aggregation, and output.
- `metadata.rs`: Collects file metadata (path, size, lines, modified time, optional SHA-256).
- `tree.rs`: Generates indented file tree representation.
- `telemetry.rs`: Tracks processing metrics (files, bytes, tokens) and reports progress.
- `logging.rs`: Handles logging with levels.
- `expandable.rs`: Wraps content in expandable sections.
- `furnace.rs`: Performs file analysis (details in module).
- `redaction.rs`: Applies regex and forbidden-string redaction to file content.

## Dependencies
- regex: For parsing references.
- arboard: For clipboard support.
- once_cell: For lazy statics.
- clap (with derive): For argument parsing.
- ignore: For directory walking with ignores.
- sha2: For hashing.
- csv: For reading forbidden-string CSV stores.
- rusqlite: For reading forbidden-string SQLite stores.
- ureq: For live Padagonia HTTP queries.
- toml: For redaction config files.
- dirs: For platform cache directories.

## Naming Conventions and Style
- Standard Rust conventions: snake_case for variables/functions, CamelCase for types.
- Modules named in snake_case (e.g., `metadata.rs`).
- Doc comments using `//!` at module level.
- Consistent use of error handling with `Result`.
- Uses `Lazy` for static regex patterns.

## Testing Approach
Unit tests live in `crates/bound-core/src/redaction.rs`. Run `cargo test` to execute them.

## Important Gotchas
- Uses `.boundignore` for custom ignore patterns during directory walking.
- Dependency resolution supports Python, JS/TS, C/C++ import patterns.
- Relative path resolution handles parent directories (`..`).
- Content truncation applies after reading full file; limits are per-file.
- Output defaults to clipboard; specify `--out` for file output.
- Telemetry reports every 10 files or at end.
- Filter extensions work with or without leading dot: `[rs]` and `[.rs]` are equivalent.
- Non-UTF-8 files are skipped with a warning instead of causing errors.
- Live Padagonia redaction queries log a warning and continue without them if the API is unreachable or returns an error.
