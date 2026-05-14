# casc-cli

A command-line utility to extract files from Blizzard CASC archives.

## Building

### Prerequisites

* **Rust:** Standard Rust toolchain (cargo).
* **CMake:** Required to build the `CascLib` C++ dependency.
* **Make:** Required by CMake to generate build files.
* **LLVM/Clang:** Required by `bindgen` to generate Rust bindings.

On systems using Homebrew, you can install these with:
```bash
brew install cmake make llvm pkg-config linux-headers
```

### Build Configuration

On systems where Homebrew is not in a standard path, you must provide the location of system headers during the build process so `bindgen` can find them:

```bash
export BINDGEN_EXTRA_CLANG_ARGS="-I$(brew --prefix glibc)/include -I$(brew --prefix linux-headers)/include"
cargo build
```

## Usage

### Commands

#### List (`list`, `l`)
Lists the contents of the CASC archive.

**Syntax:**
`casc list <archive_dir> [targets...]`

**Examples:**
* `casc l ./Data` *(List everything)*
* `casc l ./Data data/global/excel/` *(List everything inside the excel folder)*
* `casc l ./Data '*.txt'` *(List all text files anywhere)*

#### Extract (`extract`, `x`)
Extracts files from the CASC archive, preserving their internal directory structure in the current working directory.

**Syntax:**
`casc extract <archive_dir> [targets...]`

**Examples:**
* `casc x ./Data data/global/excel/weapons.txt` *(Extract a single file)*
* `casc x ./Data data/global/excel/weapons.txt data/global/excel/armor.txt` *(Extract multiple specific files)*
* `casc x ./Data data/global/excel/` *(Extract an entire directory)*
* `casc x ./Data '*.txt'` *(Extract all text files anywhere)*
* `casc x ./Data '*.txt' data/global/config.ini` *(Extract multiple targets: a glob pattern and a specific file)*

### Target Resolution
Both the `list` and `extract` commands rely on **Targets**. Positional arguments provided after the archive directory are treated as targets.

A target can be:
1. **Exact Matches:** Matches a specific, literal file path (e.g., `data/global/excel/weapons.txt`).
2. **Directory Namespaces:** If a target ends with a trailing slash (`/` or `\`), it is treated as a recursive directory prefix.
3. **Glob Patterns:** Standard wildcards (`*`, `?`, `**`) are supported. The syntax is identical to the one used by [ripgrep](https://github.com/BurntSushi/ripgrep) and `.gitignore` files. See the [full syntax documentation](https://docs.rs/globset/latest/globset/#syntax) for details.

#### Namespace Prefix Omission
CASC archives often use namespace prefixes (e.g., `data:`). `casc-cli` implements a "dual-matching" strategy that allows you to omit these prefixes in your targets.

For example, a file named `data:locales\data\zhtw\ui\tradestash.dc6` can be matched using any of the following:
* `data:locales/data/zhtw/ui/tradestash.dc6` (Exact match)
* `locales/data/zhtw/ui/tradestash.dc6` (Omitted prefix)
* `locales/data/**/*.dc6` (Glob pattern with omitted prefix)

*Note: Both `\` and `/` are treated identically in target patterns.*

## Credits & Licensing

`casc-cli` is licensed under the [MIT License](LICENSE).

This project includes and wraps [CascLib](https://github.com/ladislav-zezula/CascLib), which is Copyright (c) 2014 Ladislav Zezula and also licensed under the MIT License.
