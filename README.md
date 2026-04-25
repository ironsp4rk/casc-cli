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

## Credits & Licensing

`casc-cli` is licensed under the [MIT License](LICENSE).

This project includes and wraps [CascLib](https://github.com/ladislav-zezula/CascLib), which is Copyright (c) 2014 Ladislav Zezula and also licensed under the MIT License.
