# casc-cli (casc)
A cross-platform CLI tool for Blizzard CASC archives.

`casc-cli` (binary `casc`) is a command-line utility for listing and extracting files from Blizzard CASC archives. It provides a safe, idiomatic Rust interface over Ladislav Zezula's `CascLib` C++ library, ensuring both performance and safety.

While specifically developed and tested against **Diablo II: Resurrected**, it is designed to work with any modern Blizzard game using the CASC storage format.

[![Crate](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

### Documentation quick links
*   [Quick Start](#quick-start)
*   [Why use casc-cli?](#why-use-casc-cli)
*   [Installation](#installation)
*   [Command Reference](#command-reference)
*   [Target Resolution & Globbing](#target-resolution--globbing)
*   [Building & Testing](#building--testing)
*   [Credits & Licensing](#credits--licensing)

---

## Quick Start
Extract all `.txt` files from an archive into a local folder:
```bash
casc extract -o ./extracted_files ./Data '*.txt'
```

List everything inside the `data/global/excel/` namespace:
```bash
casc list ./Data data/global/excel/
```

Check if a specific file exists (via exit code):
```bash
casc list ./Data data/global/config.ini
```

---

## Why use casc-cli?
*   **Safe & Idiomatic Rust:** Built with modern Rust practices, providing a safe wrapper around the core C++ logic.
*   **Flexible Matching:** Support for exact paths, directory namespaces, and powerful glob patterns.
*   **Dual-Matching Strategy:** Automatically handles common CASC namespace prefixes (e.g., `data:`), allowing you to omit them in your queries.
*   **Cross-Platform:** Designed to run seamlessly on Linux, macOS, and Windows.
*   **Scripting Friendly:** Follows Unix philosophy with meaningful exit codes and silent success for data piping.

---

## Installation
Currently, `casc-cli` must be built from source. Detailed instructions, including prerequisites for various platforms, can be found in the [Building & Testing](#building--testing) section.

---

## Command Reference

### List (`list`, `l`)
Lists the contents of the CASC archive.

**Syntax:**
`casc list <archive_dir> [targets...]`

**Examples:**
*   `casc l ./Data` *(Recursive dump of everything)*
*   `casc l ./Data data/global/excel/` *(List everything inside the excel folder)*
*   `casc l ./Data '*.txt'` *(List all text files anywhere in the archive)*
*   `casc l ./Data 'data/global/**/*.txt'` *(List all text files in data/global/ and subdirectories)*
*   `casc l ./Data data/global/config.ini locales/enus/` *(List multiple specific targets)*

**Exit Codes:**
*   `0`: Success (At least one match found, or no targets provided).
*   `1`: No Matches (Targets provided but none matched).
*   `3`: Fatal Error (Archive failed to open, etc.).

### Extract (`extract`, `x`)
Extracts files from the CASC archive to the local filesystem.

**Syntax:**
`casc extract [options] <archive_dir> [targets...]`

**Options:**
*   `-o, --output <dir>`: The base local directory where files should be extracted. Defaults to the current working directory (`.`).
*   `-f, --flatten`: Strip all internal directory structures and extract all matching files directly into the root of the output directory.

**Examples:**
*   `casc x ./Data data/global/excel/weapons.txt` *(Extract single file)*
*   `casc x --flatten ./Data data/global/excel/weapons.txt data/global/excel/armor.txt` *(Extract multiple explicit files into the current directory with a flattened structure)*
*   `casc x ./Data data/global/excel/` *(Extract entire directory tree)*
*   `casc x ./Data '*.txt'` *(Extract all text files anywhere)*
*   `casc x -o ./out ./Data '*.txt' data/global/config.ini` *(Extract multiple varied targets to a specific folder)*

**Exit Codes:**
*   `0`: Success (All matching files processed successfully).
*   `1`: No Matches (No files matched the provided targets).
*   `2`: Warning (One or more files were skipped, e.g., due to name collisions during a flattened extraction).
*   `3`: Fatal Error (At least one file failed to process).

---

## Target Resolution & Globbing
Both the `list` and `extract` commands rely on **Targets**. Positional arguments provided after the archive directory are treated as targets.

A target can be:
1.  **Exact Matches:** Matches a specific, literal file path (e.g., `data/global/excel/weapons.txt`).
2.  **Directory Namespaces:** If a target ends with a trailing slash (`/` or `\`), it is treated as a recursive directory prefix.
3.  **Glob Patterns:** Standard wildcards (`*`, `?`, `**`) are supported. The syntax is identical to the one used by [ripgrep](https://github.com/BurntSushi/ripgrep) and `.gitignore` files. See the [full syntax documentation](https://docs.rs/globset/latest/globset/#syntax) for details.

### Namespace Prefix Omission
CASC archives often use namespace prefixes (e.g., `data:`). `casc-cli` implements a "dual-matching" strategy that allows you to omit these prefixes in your targets.

For example, a file named `data:locales\data\zhtw\ui\tradestash.dc6` can be matched using any of the following:
*   `data:locales/data/zhtw/ui/tradestash.dc6` (Exact match)
*   `locales/data/zhtw/ui/tradestash.dc6` (Omitted prefix)
*   `locales/data/**/*.dc6` (Glob pattern with omitted prefix)

*Note: Both `\` and `/` are treated identically in target patterns.*

---

## Building & Testing

### Prerequisites
To build `casc-cli` from source, you will need the following installed on your system:

*   **Rust:** Standard Rust toolchain (cargo).
*   **CMake:** Required to build the `CascLib` C++ dependency.
*   **Make:** Required by CMake to generate build files.
*   **LLVM/Clang:** Required by `bindgen` to generate Rust FFI bindings.

On systems using **Homebrew**, you can install these dependencies with:
```bash
brew install cmake make llvm pkg-config linux-headers
```

On systems where Homebrew is not in a standard path, you must provide the location of system headers during the build process so `bindgen` can find them:

```bash
export BINDGEN_EXTRA_CLANG_ARGS="-I$(brew --prefix glibc)/include -I$(brew --prefix linux-headers)/include"
cargo build
```

### Building
```bash
git clone https://github.com/ironsp4rk/casc-cli
cd casc-cli
cargo build --release
./target/release/casc --version
casc 0.1.0
```

### Verification
To ensure the project follows all coding standards and passes all tests:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features
cargo test
```

---

## Credits & Licensing
`casc-cli` is licensed under the [MIT License](LICENSE).

This project includes and wraps [CascLib](https://github.com/ladislav-zezula/CascLib), which is Copyright (c) 2014 Ladislav Zezula and also licensed under the MIT License.
