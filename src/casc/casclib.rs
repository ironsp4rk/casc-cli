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

    /// Opens a file within a CASC storage.
    ///
    /// # Arguments
    /// * `hStorage` - Handle to an opened CASC storage.
    /// * `szFileName` - Name of the file to open.
    /// * `dwLocaleFlags` - Locale flags for the file.
    /// * `dwOpenFlags` - Open flags for the file.
    /// * `phFile` - Pointer to receive the opened file handle.
    ///
    /// # Returns
    /// `true` if the file was opened successfully, `false` otherwise.
    ///
    /// # Safety
    /// This function is unsafe as it dereferences raw pointers and performs FFI calls.
    unsafe fn casc_open_file(
        &self,
        hStorage: Handle,
        szFileName: *const std::ffi::c_char,
        dwLocaleFlags: u32,
        dwOpenFlags: u32,
        phFile: *mut Handle,
    ) -> bool;

    /// Reads data from a file.
    ///
    /// # Arguments
    /// * `hFile` - Handle to the opened file.
    /// * `lpBuffer` - Pointer to the buffer to receive the data.
    /// * `dwToRead` - Number of bytes to read.
    /// * `pdwRead` - Pointer to receive the number of bytes actually read.
    ///
    /// # Returns
    /// `true` if successful, `false` otherwise.
    ///
    /// # Safety
    /// This function is unsafe as it dereferences raw pointers and performs FFI calls.
    unsafe fn casc_read_file(
        &self,
        hFile: Handle,
        lpBuffer: *mut std::ffi::c_void,
        dwToRead: u32,
        pdwRead: *mut u32,
    ) -> bool;

    /// Closes an opened file handle.
    ///
    /// # Arguments
    /// * `hFile` - Handle to the file to close.
    ///
    /// # Returns
    /// `true` if the handle was closed successfully, `false` otherwise.
    ///
    /// # Safety
    /// This function is unsafe as it performs FFI calls with a raw handle.
    unsafe fn casc_close_file(&self, hFile: Handle) -> bool;
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

    unsafe fn casc_open_file(
        &self,
        hStorage: Handle,
        szFileName: *const std::ffi::c_char,
        dwLocaleFlags: u32,
        dwOpenFlags: u32,
        phFile: *mut Handle,
    ) -> bool {
        bindings::CascOpenFile(
            hStorage,
            szFileName as *const std::ffi::c_void,
            dwLocaleFlags,
            dwOpenFlags,
            phFile,
        )
    }

    unsafe fn casc_read_file(
        &self,
        hFile: Handle,
        lpBuffer: *mut std::ffi::c_void,
        dwToRead: u32,
        pdwRead: *mut u32,
    ) -> bool {
        bindings::CascReadFile(hFile, lpBuffer, dwToRead, pdwRead)
    }

    unsafe fn casc_close_file(&self, hFile: Handle) -> bool {
        bindings::CascCloseFile(hFile)
    }
}

#[cfg(test)]
impl MockCascLib {
    /// Helper to mock opening a storage for the `MockCascLib`.
    ///
    /// **Verification:** This helper sets a strict expectation (`.times(1)`) for `casc_open_storage`.
    pub fn mock_open(&mut self) {
        self.expect_casc_open_storage()
            .times(1)
            .returning(|_, _, handle| unsafe {
                *handle = 1 as Handle;
                true
            });
    }

    /// Helper to mock a list of files for the `MockCascLib`.
    ///
    /// It configures the mock to successfully "open" a storage and return the provided list
    /// of files during iteration.
    ///
    /// **Verification:** This helper sets strict expectations (`.times(n)`) for:
    /// - `casc_find_first_file` (exactly once)
    /// - `casc_find_next_file` (exactly once per file + once for termination)
    ///
    /// # Arguments
    /// * `files` - A vector of file paths to be "found" in the archive.
    pub fn mock_file_list(&mut self, files: Vec<&'static str>) {
        use std::ptr;

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

    /// Helper to mock opening and reading a file for the `MockCascLib`.
    ///
    /// This function sets up expectations for:
    /// * `CascOpenFile`: Verifies the filename matches `name` and is called exactly once.
    /// * `CascReadFile`: Verifies it is called with the correct file handle.
    /// * `CascCloseFile`: Verifies it is called with the correct file handle and exactly once.
    ///
    /// # Arguments
    /// * `name` - The name of the file to mock.
    /// * `content` - The content of the file.
    /// * `handle` - A unique handle for this file.
    pub fn mock_file_read(&mut self, name: &'static str, content: Vec<u8>, handle: usize) {
        self.expect_casc_open_file()
            .withf(move |_, filename, _, _, _| unsafe {
                std::ffi::CStr::from_ptr(*filename).to_str().unwrap() == name
            })
            .times(1)
            .returning(move |_, _, _, _, phFile| unsafe {
                *phFile = handle as Handle;
                true
            });

        let mut remaining_content = content;
        self.expect_casc_read_file()
            .withf(move |hFile, _, _, _| *hFile == handle as Handle)
            .returning(move |_, lpBuffer, dwToRead, pdwRead| unsafe {
                let to_read = std::cmp::min(dwToRead as usize, remaining_content.len());
                std::ptr::copy_nonoverlapping(
                    remaining_content.as_ptr(),
                    lpBuffer as *mut u8,
                    to_read,
                );
                *pdwRead = to_read as u32;
                remaining_content.drain(0..to_read);
                true
            });

        self.expect_casc_close_file()
            .withf(move |hFile| *hFile == handle as Handle)
            .times(1)
            .return_const(true);
    }
}
