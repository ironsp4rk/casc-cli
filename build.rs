use std::env;
use std::path::PathBuf;

fn main() {
    // 1. Compile CascLib using CMake
    let dst = cmake::Config::new("ext/CascLib")
        .define("CASC_BUILD_STATIC_LIB", "ON")
        .define("CASC_BUILD_SHARED_LIB", "OFF")
        .define("CMAKE_POLICY_VERSION_MINIMUM", "3.5")
        // Force CascLib to use its own internal zlib instead of looking for a system shared library.
        // This ensures our final CLI binary is self-contained and statically linked.
        .define("CMAKE_DISABLE_FIND_PACKAGE_ZLIB", "TRUE")
        .build();

    // 2. Link the compiled library
    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-lib=static=casc");

    // Link C++ standard library
    if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    } else if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=dylib=c++");
    }

    // 3. Generate bindings using bindgen
    let mut builder = bindgen::Builder::default()
        .header("ext/CascLib/src/CascLib.h")
        // CascLib uses some Windows types even on Linux (via CascPort.h)
        .clang_arg("-Iext/CascLib/src")
        // Blocklist IPPORT_RESERVED because it's redefined in multiple headers
        .blocklist_item("IPPORT_RESERVED")
        // Disable layout tests (size assertions) because they can fail on some
        // systems with complex glibc/Clang interactions.
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    // On macOS, we need to provide the SDK path to bindgen so it can find standard headers
    // when using a non-system libclang (e.g. from Homebrew).
    let target = env::var("TARGET").unwrap_or_default();
    if target.contains("apple-darwin") {
        if let Ok(output) = std::process::Command::new("xcrun")
            .args(["--show-sdk-path"])
            .output()
        {
            if output.status.success() {
                let sdk_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                builder = builder.clang_arg(format!("-isysroot{}", sdk_path));
            }
        }
    }

    let bindings = builder
        .generate()
        .expect("Unable to generate bindings");

    // 4. Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
