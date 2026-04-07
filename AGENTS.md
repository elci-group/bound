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

- FILTER: Optional language filter in `[.ext]` (exact extension) or `{.ext}` (extension with dependencies).
- DIRECTORY: Target directory (defaults to `.`).
- OPTIONS:
  - `-t, --token-limit <N>`: Token limit per file.
  - `-s, --size-limit <N>`: Size limit in bytes per file.
  - `-d, --depth-limit <N>`: Depth limit.
  - `--out <FILE>`: Output to file instead of clipboard.
  - `--meta`: Include metadata headers.
  - `--meta-hash`: Include SHA-256 hash in metadata.
  - `--tree`: Include file tree.
  - `--furnace`: Enable Furnace analysis.

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

## Dependencies
- regex: For parsing references.
- arboard: For clipboard support.
- once_cell: For lazy statics.
- clap (with derive): For argument parsing.
- ignore: For directory walking with ignores.
- sha2: For hashing.

## Naming Conventions and Style
- Standard Rust conventions: snake_case for variables/functions, CamelCase for types.
- Modules named in snake_case (e.g., `metadata.rs`).
- Doc comments using `//!` at module level.
- Consistent use of error handling with `Result`.
- Uses `Lazy` for static regex patterns.

## Testing Approach
No test files or #[test] functions observed. Use `cargo test` if tests are added.

## Important Gotchas
- Uses `.boundignore` for custom ignore patterns during directory walking.
- Dependency resolution supports Python, JS/TS, C/C++ import patterns.
- Relative path resolution handles parent directories (`..`).
- Content truncation applies after reading full file; limits are per-file.
- Output defaults to clipboard; specify `--out` for file output.
- Telemetry reports every 10 files or at end.