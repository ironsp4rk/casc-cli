mod bindings;

use std::ffi::CString;
use std::ptr;

fn main() {
    println!("CascLib Hello World");

    // Hardcoded path to a CASC archive
    let path = "/your/path/to/Diablo II Resurrected/Data";
    let c_path = CString::new(path).expect("CString::new failed");

    let mut storage_handle: bindings::HANDLE = ptr::null_mut();

    unsafe {
        // CascOpenStorage returns bool (true for success)
        if bindings::CascOpenStorage(
            c_path.as_ptr(),
            /* dwLocaleMask=*/ 0,
            &mut storage_handle,
        ) {
            println!("Successfully opened storage: {}", path);

            // Close the storage
            if bindings::CascCloseStorage(storage_handle) {
                println!("Successfully closed storage.");
            } else {
                eprintln!("Failed to close storage.");
            }
        } else {
            eprintln!("Failed to open storage: {}", path);
        }
    }
}
