#![allow(unsafe_op_in_unsafe_fn)]
#![allow(non_snake_case)]

//! # FFI Abstraction Layer for CascLib
//!
//! This module provides the `CascLib` trait, which acts as a safe, mockable wrapper
//! around the raw C-FFI bindings for the external `CascLib` C++ library (located in `ext/CascLib`).
//!
//! **CRITICAL:** All interactions with the underlying `CascLib` dependency MUST be routed
//! through this trait and its implementations. Direct access to the `bindings` module from
//! other parts of the application is strictly discouraged to maintain a clean abstraction
//! boundary and enable reliable unit testing through mocking.

mod bindings;

#[cfg(test)]
use mockall::automock;

/// Encapsulated FFI type representing a handle to a CASC storage or search session.
///
/// This type is re-exported from the underlying `bindings` to prevent other modules
/// from needing to depend on the raw FFI module directly.
pub type Handle = bindings::HANDLE;

/// Encapsulated FFI structure containing information about a file found within the archive.
///
/// This type is re-exported from the underlying `bindings` to prevent other modules
/// from needing to depend on the raw FFI module directly.
pub type CascFindData = bindings::CASC_FIND_DATA;

/// A trait abstracting the raw, unsafe FFI calls to the `CascLib` C++ library.
///
/// This trait isolates the C interface so that it can be safely mocked
/// in tests using `MockCascLib`, allowing for verification of archive logic
/// without requiring a physical CASC archive or interacting with the filesystem.
#[cfg_attr(test, automock)]
pub trait CascLib {
    /// Opens a CASC storage directory.
    ///
    /// # Arguments
    /// * `szParams` - A pointer to a C-style string containing the path to the archive.
    /// * `dwLocaleMask` - Bitmask for the locale to be used.
    /// * `phStorage` - A mutable pointer to receive the opened storage handle.
    ///
    /// # Returns
    /// `true` if the storage was opened successfully, `false` otherwise.
    ///
    /// # Safety
    /// This function is unsafe as it dereferences raw pointers provided by the caller.
    unsafe fn casc_open_storage(
        &self,
        szParams: *const std::ffi::c_char,
        dwLocaleMask: u32,
        phStorage: *mut Handle,
    ) -> bool;

    /// Closes a previously opened CASC storage handle.
    ///
    /// # Arguments
    /// * `hStorage` - The handle to the storage to close.
    ///
    /// # Returns
    /// `true` if the storage was closed successfully, `false` otherwise.
    ///
    /// # Safety
    /// This function is unsafe as it performs FFI calls with a raw handle.
    unsafe fn casc_close_storage(&self, hStorage: Handle) -> bool;

    /// Initiates a search for files within a CASC storage.
    ///
    /// # Arguments
    /// * `hStorage` - Handle to an opened CASC storage.
    /// * `szMask` - Search mask (e.g., `*`).
    /// * `pFindData` - Pointer to a `CascFindData` structure to receive the first file's info.
    /// * `szListFile` - Optional path to a listfile for name resolution.
    ///
    /// # Returns
    /// A handle to the search session if successful, or `null` otherwise.
    ///
    /// # Safety
    /// This function is unsafe as it dereferences raw pointers and performs FFI calls.
    unsafe fn casc_find_first_file(
        &self,
        hStorage: Handle,
        szMask: *const std::ffi::c_char,
        pFindData: *mut CascFindData,
        szListFile: *const std::ffi::c_char,
    ) -> Handle;

    /// Continues a search initiated by `casc_find_first_file`.
    ///
    /// # Arguments
    /// * `hFind` - Handle to an active search session.
    /// * `pFindData` - Pointer to a `CascFindData` structure to receive the next file's info.
    ///
    /// # Returns
    /// `true` if another file was found, `false` if the search is complete or failed.
    ///
    /// # Safety
    /// This function is unsafe as it dereferences raw pointers and performs FFI calls.
    unsafe fn casc_find_next_file(&self, hFind: Handle, pFindData: *mut CascFindData) -> bool;

    /// Closes a search handle.
    ///
    /// # Arguments
    /// * `hFind` - The handle to the search session to close.
    ///
    /// # Returns
    /// `true` if the handle was closed successfully, `false` otherwise.
    ///
    /// # Safety
    /// This function is unsafe as it performs FFI calls with a raw handle.
    unsafe fn casc_find_close(&self, hFind: Handle) -> bool;
}

/// The default production implementation of `CascLib` that calls the underlying C-FFI functions.
#[derive(Clone, Default)]
pub struct DefaultCascLib;

impl CascLib for DefaultCascLib {
    unsafe fn casc_open_storage(
        &self,
        szParams: *const std::ffi::c_char,
        dwLocaleMask: u32,
        phStorage: *mut Handle,
    ) -> bool {
        bindings::CascOpenStorage(szParams, dwLocaleMask, phStorage)
    }

    unsafe fn casc_close_storage(&self, hStorage: Handle) -> bool {
        bindings::CascCloseStorage(hStorage)
    }

    unsafe fn casc_find_first_file(
        &self,
        hStorage: Handle,
        szMask: *const std::ffi::c_char,
        pFindData: *mut CascFindData,
        szListFile: *const std::ffi::c_char,
    ) -> Handle {
        bindings::CascFindFirstFile(hStorage, szMask, pFindData, szListFile)
    }

    unsafe fn casc_find_next_file(&self, hFind: Handle, pFindData: *mut CascFindData) -> bool {
        bindings::CascFindNextFile(hFind, pFindData)
    }

    unsafe fn casc_find_close(&self, hFind: Handle) -> bool {
        bindings::CascFindClose(hFind)
    }
}

#[cfg(test)]
impl MockCascLib {
    /// Helper to mock a list of files for the `MockCascLib`.
    ///
    /// It configures the mock to successfully "open" a storage and return the provided list
    /// of files during iteration.
    ///
    /// **Verification:** This helper sets strict expectations (`.times(n)`) for:
    /// - `casc_open_storage` (exactly once)
    /// - `casc_find_first_file` (exactly once)
    /// - `casc_find_next_file` (exactly once per file + once for termination)
    ///
    /// # Arguments
    /// * `files` - A vector of file paths to be "found" in the archive.
    pub fn mock_file_list(&mut self, files: Vec<&'static str>) {
        use std::ptr;

        self.expect_casc_open_storage()
            .times(1)
            .returning(|_, _, handle| unsafe {
                *handle = 1 as Handle;
                true
            });

        if files.is_empty() {
            self.expect_casc_find_first_file()
                .times(1)
                .returning(|_, _, _, _| ptr::null_mut() as Handle);
            return;
        }

        let num_files = files.len();
        let first_file = files[0];
        self.expect_casc_find_first_file()
            .times(1)
            .returning(move |_, _, pFindData, _| unsafe {
                let bytes = first_file.as_bytes();
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    (*pFindData).szFileName.as_mut_ptr() as *mut u8,
                    bytes.len(),
                );
                (*pFindData).szFileName[bytes.len()] = 0;
                2 as Handle
            });

        let next_files = files[1..].to_vec();
        let mut next_call_count = 0;
        self.expect_casc_find_next_file()
            .times(num_files)
            .returning(move |_, pFindData| unsafe {
                if next_call_count < next_files.len() {
                    let bytes = next_files[next_call_count].as_bytes();
                    std::ptr::copy_nonoverlapping(
                        bytes.as_ptr(),
                        (*pFindData).szFileName.as_mut_ptr() as *mut u8,
                        bytes.len(),
                    );
                    (*pFindData).szFileName[bytes.len()] = 0;
                    next_call_count += 1;
                    true
                } else {
                    false
                }
            });
    }
}
