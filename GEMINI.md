# Project: casc-cli

## Overview
`casc-cli` is a cross-platform command-line utility written in Rust. Its primary purpose is to extract files from Blizzard CASC archives. It achieves this by acting as a CLI interface and providing a safe Rust FFI wrapper around Ladislav Zezula's C++ `CascLib`.

## Tech Stack
* **Language:** Rust (CLI app and FFI wrapper).
* **Core Dependency:** Zezula's `CascLib` (C++) included as a Git submodule.
* **Build System:** `cargo` with a `build.rs` script to compile the C++ dependency.

## Directory Structure (Locked)
This repository structure is locked to support current source-based development and future distribution models (GitHub Actions, Homebrew, Chocolatey). Do not deviate from this layout.

```text
casc-cli/
├── .github/                    # (Future) CI/CD pipelines for matrix builds and release automation
├── .gitmodules                 # Tracking the CascLib repository commit
├── choco/                      # (Future) Chocolatey package configurations for Windows distribution
├── ext/                        # External dependencies
│   └── CascLib/                # Git submodule pointing to Zezula's CascLib source
├── src/                        # Rust source code
│   ├── main.rs                 # CLI entry point, argument parsing, and application logic
│   ├── casc.rs                 # Safe Rust abstractions wrapping the FFI calls
│   ├── casc/                   # Submodules for the safe wrapper
│   │   ├── casclib.rs          # FFI abstraction layer (trait and implementation)
│   │   └── casclib/            # Submodules for casclib
│   │       └── bindings.rs     # Unsafe Rust FFI bindings to CascLib
│   ├── commands.rs             # CLI subcommand registration
│   └── commands/               # CLI subcommand implementations
│       └── list.rs             # Logic for the 'list' command
├── build.rs                    # Build script to compile CascLib and link it to the Rust binary
├── Cargo.toml                  # Rust package metadata and dependencies
├── LICENSE          
└── README.md
```

## Code Verification
After making any code changes, always perform the following verification steps:
1. **Formatting:** Run `cargo fmt` to ensure the code follows standard Rust styling.
2. **Linting:** Run `cargo clippy` to catch common mistakes and improve code quality.
3. **Testing:** Run `cargo test` to execute unit tests and ensure no regressions.

# CLI API Specification

## Design Philosophy
This CLI adopts a modern subcommand architecture paired with classic single-letter aliases. Because CASC archives are typically stored as a directory structure containing multiple split data and index files (rather than a single monolithic file), the CLI is designed to target the **root directory** of the CASC storage.

By standard Unix convention, successful operations should be silent in scripting environments. The tool should use TTY detection to display transient progress bars for interactive terminal sessions, but output absolute silence or clean final summaries otherwise.

## Global Flags
* `-v, --verbose`: Print detailed output (e.g., list every file path as it is processed). 
* `-q, --quiet`: Suppress all non-error output. When enabled, the CLI will output absolutely zero bytes unless a fatal, execution-halting error occurs (essential for CI/CD pipelines and shell scripting).

---

## Command: List (`list`, `l`)
Lists the contents of the CASC archive. This command is **recursive by default**. If run on the root, it will list every file in the archive. If a `path_prefix` is provided, it recursively lists all files under that directory.

**Syntax:**
`<cli> list <archive_dir> [path_prefix] [flags]`

**Flags:**
* `-d, --depth <N>`: Limit the recursion depth. `--depth 1` prints only immediate children.
* `-t, --tree`: Output the contents in a visual directory tree format instead of a flat list.

**Global Flag Interactions (List):**
* **Verbose (`-v`): Detailed View.** Transforms the output from a flat list of paths into a detailed table including metadata (e.g., File Size, Compressed Size, Hash Keys, File Path) similar to the `ls -l` Unix command.
* **Quiet (`-q`): Validation / Existence Mode.** Suppresses all `stdout`. The command acts purely as a boolean check via the exit code. Useful for scripts to check if an archive is healthy or if a specific file exists without polluting the terminal.

**Examples:**
* `cli l ./Data -v` *(Lists all files with sizes and hash keys)*
* `cli l ./Data data/global/config.ini -q` *(Prints nothing. Returns exit code `0` if `config.ini` exists, `1` if it does not).*

---

## Command: Extract (`extract`, `x`)
Extracts files or directories from the CASC archive. 

**Syntax:**
`<cli> extract <archive_dir> [targets...] [flags]`

**Flags:**
* `-o, --output <dir>`: Extract files into a specific directory. Defaults to the current working directory (`./`).
* `-f, --flatten`: Strip all internal directory structures and extract files directly into the root of the output destination.

---

## Target Resolution & Multi-file Design
Positional arguments provided *after* the archive directory are treated as extraction targets. The parser iterates through these targets and matches them against the CASC archive's internal paths.

1. **Exact Matches:** Matches the specific file path.
2. **Directory Namespaces:** If a target ends with a trailing slash (e.g., `data/global/`), the CLI treats it as a namespace prefix and recursively matches all children.
3. **Glob Patterns:** Native support for standard wildcards (`*`, `?`). *Note: Patterns must be quoted in the terminal (e.g., `'*.txt'`).*

---

## Output Behavior & TTY Detection

To ensure maximum performance and compatibility with Unix pipelines, the CLI handles standard output (`stdout`) and standard error (`stderr`) differently based on the environment and flags:

| Mode | `stdout` (Standard Output) | `stderr` (Standard Error) |
| :--- | :--- | :--- |
| **Interactive TTY (Default)** | Transient progress bar (disappears on completion), followed by a brief final summary (e.g., "Extracted X files in Ys"). | Warnings and fatal errors. |
| **Non-Interactive Script (Default)** | Final summary only. | Warnings and fatal errors. |
| **Verbose (`-v`)** | Every file path processed, final summary. | Warnings and fatal errors. |
| **Quiet (`-q`)** | *Absolutely nothing.* | **Fatal errors only.** |

---

## Core Operations Use Cases

*(In these examples, `./Data` represents the root directory of the split CASC archive on your hard drive).*

### 1. List files (all or folder by folder)
**Command:** `list` (alias: `l`)
* **List all files (recursive dump):** `cli l ./Data`
* **List files inside a specific folder (recursive):** `cli l ./Data data/global/excel/`
* **List only root-level files/folders:** `cli l ./Data --depth 1`
* **View archive structure visually:** `cli l ./Data --tree`

### 2. Extract a single file by name/path
**Command:** `extract` (alias: `x`)
* **Usage:** `cli x ./Data data/global/excel/weapons.txt`

### 3. Extract multiple files by name/path
**Command:** `extract` (alias: `x`)
* **Usage:** `cli x ./Data data/global/excel/weapons.txt data/global/excel/armor.txt`

### 4. Extract all files in a particular directory (with/without mask)
**Command:** `extract` (alias: `x`)
* **Extract entire directory tree:** `cli x ./Data data/global/excel/`
* **Extract specific pattern within a directory:** `cli x ./Data 'data/global/excel/*.txt'`

### 5. Advanced: Flattened Extraction
**Command:** `extract` (alias: `x`) with `--flatten` (alias: `-f`)
* **Usage:** `cli x ./Data 'data/global/excel/*.txt' --flatten -o ./extracted_tables`
  
  *(Behavior: Scans the CASC storage in `./Data` for all `.txt` files within the internal `excel/` path, strips the `data/global/excel/` folder tree, and dumps the raw `.txt` files directly into your local `./extracted_tables` directory).*