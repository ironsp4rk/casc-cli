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
* `-q, --quiet`: Suppress all non-error output. When enabled, the CLI will output absolutely zero bytes unless a fatal, execution-halting error occurs.

---

## Target Resolution (`[targets...]`)
Both the `list` and `extract` commands rely on **Targets**. Positional arguments provided *after* the archive directory are treated as targets. The parser iterates through these targets and matches them against the CASC archive's internal paths.

A target can be:
1. **Exact Matches:** Matches a specific, literal file path (e.g., `data/global/excel/weapons.txt`).
2. **Directory Namespaces:** If a target ends with a trailing slash (e.g., `data/global/`), the CLI treats it as a namespace prefix and recursively matches all children.
3. **Glob Patterns:** Native support for standard wildcards (`*`, `?`, `**`). 
   * *Note: Patterns must be quoted in the terminal (e.g., `'*.txt'`) to prevent the user's shell from expanding them locally before the CLI runs.*

---

## Command: List (`list`, `l`)
Lists the contents of the CASC archive. This command is **recursive by default**. If run without targets, it will list every file in the archive. If targets are provided, it filters the output based on those targets.

**Syntax:**
`<cli> list <archive_dir> [targets...] [flags]`

**Flags:**
* `-d, --depth <N>`: Limit the recursion depth. `--depth 1` prints only immediate children.
* `-t, --tree`: Output the contents in a visual directory tree format instead of a flat list.

**Global Flag Interactions (List):**
* **Verbose (`-v`): Detailed View.** Transforms the output from a flat list of paths into a detailed table including metadata (e.g., File Size, Compressed Size, Hash Keys, File Path) similar to the `ls -l` Unix command.
* **Quiet (`-q`): Validation / Existence Mode.** Suppresses all `stdout`. Acts purely as a boolean check via the exit code `0` (found/healthy) or `1` (missing/corrupted).

**Examples:**
* `cli l ./Data` *(Recursive dump of everything)*
* `cli l ./Data data/global/excel/` *(List everything inside the excel folder)*
* `cli l ./Data 'data/global/prefix/*'` *(List using a prefix wildcard)*
* `cli l ./Data '*.txt'` *(List all text files anywhere in the archive)*
* `cli l ./Data 'data/global/*/more/*.txt'` *(Complex mid-path wildcard matching)*
* `cli l ./Data data/global/config.ini -q` *(Silent existence check)*
* `cli l ./Data --depth 1` *(List only root-level files/folders)*
* `cli l ./Data --tree` *(View archive structure visually)*

---

## Command: Extract (`extract`, `x`)
Extracts files or directories from the CASC archive. 

**Syntax:**
`<cli> extract <archive_dir> [targets...] [flags]`

**Flags:**
* `-o, --output <dir>`: Extract files into a specific directory. Defaults to the current working directory (`./`).
* `-f, --flatten`: Strip all internal directory structures and extract files directly into the root of the output destination.

**Examples:**
* `cli x ./Data data/global/excel/weapons.txt` *(Extract single file)*
* `cli x ./Data data/global/excel/weapons.txt data/global/excel/armor.txt` *(Extract multiple explicit files)*
* `cli x ./Data data/global/excel/` *(Extract entire directory tree)*
* `cli x ./Data 'data/global/excel/*.txt'` *(Extract specific pattern)*
* `cli x ./Data 'data/global/excel/*.txt' --flatten -o ./extracted_tables` *(Extract pattern, strip internal paths, output to specific local folder)*

---

## Output Behavior & TTY Detection (Matrix)

| Mode | `stdout` (Standard Output) | `stderr` (Standard Error) |
| :--- | :--- | :--- |
| **Interactive TTY (Default)** | Transient progress bar, followed by a brief final summary (e.g., "Extracted X files in Ys"). | Warnings and fatal errors. |
| **Non-Interactive Script (Default)** | Final summary only. | Warnings and fatal errors. |
| **Verbose (`-v`)** | Every file path processed (or detailed table for `list`), final summary. | Warnings and fatal errors. |
| **Quiet (`-q`)** | *Absolutely nothing.* | **Fatal errors only.** |