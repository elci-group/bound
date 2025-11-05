Here is the content formatted as a single `.md` file.

````markdown
# bound 📦✨

**bound** is a Rust CLI tool for recursively aggregating file contents from directories.  
Supports language filtering, dependency resolution, token/size/depth limits, clipboard or file output, and real-time telemetry with **Estimated Bounding Time (EBT)** ⏱️ and progress reporting 📊.

---

## Features 🌟

- **Recursive Aggregation** 🔄: Walk directories and concatenate file contents.
- **Language Filtering** 📝:
  - `[.ext]` — include only files with a specific extension.
  - `{.ext}` — include files **and their dependencies**.
- **Limits** ⚖️:
  - Token limit (`-tl <N>`) 🧮
  - Size limit in bytes (`-sl <N>`) 💾
  - Depth limit (`-dl <N>`) 🏞️
- **Output Options** 📤:
  - Clipboard (default) 📋
  - File output (`--out <file>`) 💿
- **Telemetry & Progress** 📊:
  - Files processed 📂
  - Bytes read 📏
  - Tokens aggregated 📝
  - Estimated Bounding Time (EBT) ⏱️
  - Progress bars ▓▓▓

---

## Installation 🛠️

Requires **Rust >= 1.70**.

```bash
git clone [https://github.com/](https://github.com/)<your-username>/bound.git
cd bound
cargo build --release
````

Binary will be available at `target/release/bound`.

-----

## Usage 🚀

### Basic

Aggregate all files in a directory:

```bash
bound ~/myproject
```

### Language Filtering 🐍📜

  * Fetch all Python files:

<!-- end list -->

```bash
bound [.py] ~/projects/mycode
```

  * Fetch Python files **and dependencies**:

<!-- end list -->

```bash
bound {.py} ~/projects/mycode
```

### Limits ⚡

  * Token-limited (max 1000 tokens):

<!-- end list -->

```bash
bound ~/myproject -tl 1000
```

  * Size-limited (max 10 KB):

<!-- end list -->

```bash
bound ~/myproject -sl 10240
```

  * Depth-limited (max recursion depth 3):

<!-- end list -->

```bash
bound ~/myproject -dl 3
```

### Output 🖨️

  * **Clipboard (default)** 📋
  * **File output** 💾:

<!-- end list -->

```bash
bound ~/myproject --out output.txt
```

-----

## Examples 🔍

Aggregate Python scripts and dependencies in `~/AcidPlayer`, limit to 5000 tokens, 20 KB, depth 5, and save to file:

```bash
bound {.py} ~/AcidPlayer -tl 5000 -sl 20480 -dl 5 --out aggregate.txt
```

-----

## Telemetry & Progress ⏱️📊

Progress example:

```
[ 45% | Files: 90 | Bytes: 23456 | Tokens: 4567 | EBT: 12.4s ]
```

  * **%** — Progress
  * **Files** — Processed files 📂
  * **Bytes** — Bytes read 💾
  * **Tokens** — Aggregated words/terms 📝
  * **EBT** — Estimated time remaining ⏳

Updates every 10 files or at the end of processing.

-----

## Contributing 🤝

Fork the repo, make your changes, and submit a pull request. Contributions welcome\! ✨

-----

## License 📝

MIT License © 2025

```
```
