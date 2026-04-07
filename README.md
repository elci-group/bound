# bound

**bound** is a Rust-based CLI utility for recursively aggregating file contents from directories, with optional filtering by language, dependencies, token/size/depth limits, and clipboard or file output. It features **estimated bounding time (EBT)**, **progress reporting**, and telemetry data for large-scale processing.

---

## Features

- Recursive directory traversal
- Language filtering:
  - `[.ext]` — fetch files with a specific extension
  - `{.ext}` — fetch files with extension and any referenced dependencies
- Output:
  - Clipboard (default)
  - File (`--out <filename>`)
- Optional limits:
  - Token limit (`-t, --token-limit N`)
  - Size limit in bytes (`-s, --size-limit N`)
  - Depth limit (`-d, --depth-limit N`)
- Additional options:
  - `--meta` — Include metadata headers
  - `--meta-hash` — Include SHA-256 hash in metadata
  - `--tree` — Include file tree
  - `--furnace` — Enable Furnace analysis
- Telemetry & progress:
  - Files processed
  - Bytes read
  - Tokens aggregated
  - Estimated bounding time (EBT)

---

## Installation

Requires Rust >= 1.70.

```bash
git clone https://github.com/<your-username>/bound.git
cd bound
cargo build --release
