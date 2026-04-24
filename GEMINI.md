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
├── .github/        # (Future) CI/CD pipelines for matrix builds and release automation
├── .gitmodules     # Tracking the CascLib repository commit
├── choco/          # (Future) Chocolatey package configurations for Windows distribution
├── ext/            # External dependencies
│   └── CascLib/    # Git submodule pointing to Zezula's CascLib source
├── src/            # Rust source code
│   ├── main.rs     # CLI entry point, argument parsing, and application logic
│   ├── bindings.rs # Unsafe Rust FFI bindings to CascLib
│   └── casc.rs     # Safe Rust abstractions wrapping the FFI calls
├── build.rs        # Build script to compile CascLib and link it to the Rust binary
├── Cargo.toml      # Rust package metadata and dependencies
├── LICENSE          
└── README.md
```

