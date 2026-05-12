mod casclib;

use self::casclib::{CascFindData, CascLib, DefaultCascLib, Handle};
use std::path::Path;
use std::ptr;

/// A safe, idiomatic Rust wrapper around a CASC archive.
///
/// This struct manages the lifecycle of an underlying `Handle` by leveraging
/// a `CascLib` implementation. It provides high-level methods to interact
/// with the archive contents.
pub struct Archive<L: CascLib = DefaultCascLib> {
    handle: Handle,
    lib: L,
}

impl Archive<DefaultCascLib> {
    /// Opens a CASC archive at the specified path using the default CASC library wrapper.
    ///
    /// # Arguments
    /// * `path` - A reference to a `Path` where the CASC archive is located.
    ///
    /// # Returns
    /// A `Result` containing the `Archive` instance if successful, or a `String` error message.
    ///
    /// # Errors
    /// Returns an error if the path contains invalid encoding or if the underlying
    /// library fails to open the storage.
    #[allow(dead_code)]
    pub fn open(path: &Path) -> Result<Self, String> {
        Self::open_with_lib(path, DefaultCascLib)
    }
}

impl<L: CascLib> Archive<L> {
    /// Opens a CASC archive at the specified path using a provided `CascLib` implementation.
    ///
    /// This method is primarily used for dependency injection in tests where a
    /// `MockCascLib` is required.
    ///
    /// # Arguments
    /// * `path` - The path to the CASC archive.
    /// * `lib` - An implementation of the `CascLib` trait.
    ///
    /// # Returns
    /// A `Result` containing the `Archive` instance if successful, or a `String` error message.
    ///
    /// # Errors
    /// Returns an error if the path contains invalid encoding or if the library
    /// fails to open the storage.
    pub fn open_with_lib<P: AsRef<Path>>(path: P, lib: L) -> Result<Self, String> {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| "Invalid path encoding".to_string())?;
        let c_path = std::ffi::CString::new(path_str).map_err(|e| e.to_string())?;

        let mut handle: Handle = ptr::null_mut();
        unsafe {
            if lib.casc_open_storage(c_path.as_ptr(), /* dwLocaleMask= */ 0, &mut handle) {
                Ok(Archive { handle, lib })
            } else {
                Err(format!(
                    "Failed to open CASC storage at {:?}",
                    path.as_ref()
                ))
            }
        }
    }

    /// Returns an iterator over all file paths contained within the CASC archive.
    ///
    /// The iterator yields `String` paths representing every file in the storage.
    ///
    /// # Returns
    /// An `ArchiveFileIterator` tied to the lifecycle of this archive.
    pub fn files(&self) -> ArchiveFileIterator<'_, L> {
        ArchiveFileIterator::new(self.handle, &self.lib)
    }

    /// Opens a file within the CASC archive.
    ///
    /// # Arguments
    /// * `name` - The internal path of the file to open.
    ///
    /// # Returns
    /// A `Result` containing the `ArchiveFile` if successful, or a `String` error message.
    ///
    /// # Errors
    /// Returns an error if the file name contains invalid characters or if the
    /// library fails to open the file.
    pub fn open_file(&self, name: &str) -> Result<ArchiveFile<'_, L>, String> {
        let c_name = std::ffi::CString::new(name).map_err(|e| e.to_string())?;
        let mut file_handle: Handle = ptr::null_mut();

        unsafe {
            if self.lib.casc_open_file(
                self.handle,
                c_name.as_ptr(),
                /* dwLocaleFlags= */ 0,
                /* dwOpenFlags= */ 0,
                &mut file_handle,
            ) {
                Ok(ArchiveFile {
                    handle: file_handle,
                    lib: &self.lib,
                })
            } else {
                Err(format!("Failed to open file '{}' in archive", name))
            }
        }
    }

    /// Returns the last error code from the underlying library.
    pub fn get_error(&self) -> u32 {
        unsafe { self.lib.get_casc_error() }
    }
}

impl<L: CascLib> Drop for Archive<L> {
    /// Automatically closes the CASC storage handle when the `Archive` goes out of scope.
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_null() {
                self.lib.casc_close_storage(self.handle);
            }
        }
    }
}

#[cfg(test)]
/// Test mocks for the `Archive` struct.
pub mod mock {
    use std::path::Path;
    use std::sync::Mutex;

    /// Global mutex to synchronize tests that use the static `MockArchive` context.
    pub static TEST_MUTEX: Mutex<()> = Mutex::new(());

    mockall::mock! {
        /// A mock implementation of the `ArchiveFile` struct for unit testing.
        pub ArchiveFile {}
        impl std::io::Read for ArchiveFile {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize>;
        }
    }

    mockall::mock! {
        /// A mock implementation of the `Archive` struct for unit testing.
        pub Archive {
            /// Mock for the `open` method.
            pub fn open(path: &Path) -> Result<Self, String>;
            /// Mock for the `files` method.
            pub fn files<'a>(&'a self) -> Box<dyn Iterator<Item = String> + 'a>;
            /// Mock for the `open_file` method.
            pub fn open_file(&self, name: &str) -> Result<MockArchiveFile, String>;
            /// Mock for the `get_error` method.
            pub fn get_error(&self) -> u32;
        }
    }
}

/// A safe, idiomatic Rust wrapper for an opened file within a CASC archive.
///
/// `ArchiveFile` implements `std::io::Read`, allowing it to be used with
/// any standard Rust I/O utilities. It ensures the underlying file handle
/// is closed when it goes out of scope.
pub struct ArchiveFile<'a, L: CascLib = DefaultCascLib> {
    handle: Handle,
    lib: &'a L,
}

impl<'a, L: CascLib> ArchiveFile<'a, L> {}

impl<'a, L: CascLib> std::io::Read for ArchiveFile<'a, L> {
    /// Reads data from the file into the provided buffer.
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut bytes_read: u32 = 0;
        unsafe {
            if self.lib.casc_read_file(
                self.handle,
                buf.as_mut_ptr() as *mut std::ffi::c_void,
                buf.len() as u32,
                &mut bytes_read,
            ) {
                Ok(bytes_read as usize)
            } else {
                Err(std::io::Error::other("Failed to read from CASC file"))
            }
        }
    }
}

impl<'a, L: CascLib> Drop for ArchiveFile<'a, L> {
    /// Automatically closes the file handle when the `ArchiveFile` goes out of scope.
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_null() {
                self.lib.casc_close_file(self.handle);
            }
        }
    }
}

/// An iterator that yields the paths of all files in a CASC archive.
///
/// This struct manages the `CascFindData` search state and ensures that the
/// search handle is properly closed via its `Drop` implementation.
pub struct ArchiveFileIterator<'a, L: CascLib = DefaultCascLib> {
    find_handle: Handle,
    find_data: CascFindData,
    first: bool,
    done: bool,
    lib: &'a L,
}

impl<'a, L: CascLib> ArchiveFileIterator<'a, L> {
    /// Creates a new `ArchiveFileIterator` for the given storage handle.
    ///
    /// # Arguments
    /// * `storage_handle` - The handle to the opened CASC storage.
    /// * `lib` - A reference to the `CascLib` implementation used for search calls.
    fn new(storage_handle: Handle, lib: &'a L) -> Self {
        let mut find_data: CascFindData = unsafe { std::mem::zeroed() };
        let mask = std::ffi::CString::new("*").unwrap();
        let find_handle = unsafe {
            lib.casc_find_first_file(
                storage_handle,
                mask.as_ptr(),
                &mut find_data,
                /* szListFile= */ std::ptr::null(),
            )
        };

        if find_handle.is_null() {
            ArchiveFileIterator {
                find_handle: std::ptr::null_mut(),
                find_data,
                first: false,
                done: true,
                lib,
            }
        } else {
            ArchiveFileIterator {
                find_handle,
                find_data,
                first: true,
                done: false,
                lib,
            }
        }
    }

    /// Extracts the file name from the current `CascFindData` state.
    ///
    /// # Returns
    /// A `String` containing the file path.
    fn extract_name(&self) -> String {
        unsafe {
            std::ffi::CStr::from_ptr(self.find_data.szFileName.as_ptr())
                .to_string_lossy()
                .into_owned()
        }
    }
}

impl<'a, L: CascLib> Iterator for ArchiveFileIterator<'a, L> {
    type Item = String;

    /// Advances the iterator and returns the next file path.
    ///
    /// # Returns
    /// `Some(String)` if a file is found, `None` if the iteration is complete.
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        if self.first {
            self.first = false;
            return Some(self.extract_name());
        }

        unsafe {
            if self
                .lib
                .casc_find_next_file(self.find_handle, &mut self.find_data)
            {
                Some(self.extract_name())
            } else {
                self.done = true;
                None
            }
        }
    }
}

impl<'a, L: CascLib> Drop for ArchiveFileIterator<'a, L> {
    /// Automatically closes the search handle when the iterator goes out of scope.
    fn drop(&mut self) {
        unsafe {
            if !self.find_handle.is_null() {
                self.lib.casc_find_close(self.find_handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::casclib::MockCascLib;
    use crate::casc::Archive;

    #[test]
    fn test_open_non_existent_path() {
        let mut lib = MockCascLib::new();
        lib.expect_casc_open_storage().times(1).return_const(false);

        let res = Archive::open_with_lib("/non/existent/path", lib);
        match res {
            Err(e) => assert!(e.contains("Failed to open CASC storage")),
            Ok(_) => panic!("Should have failed"),
        }
    }

    #[test]
    fn test_iterate_empty_list() {
        let mut lib = MockCascLib::new();
        lib.mock_open();
        lib.mock_file_list(vec![]);
        lib.expect_casc_close_storage().times(1).return_const(true);

        let archive = Archive::open_with_lib("/dummy/path", lib).unwrap();
        let mut files = archive.files();
        assert_eq!(files.next(), None);
    }

    #[test]
    fn test_iterate_one_file() {
        let mut lib = MockCascLib::new();
        lib.mock_open();
        lib.mock_file_list(vec!["file1.txt"]);
        lib.expect_casc_close_storage().times(1).return_const(true);
        lib.expect_casc_find_close().times(1).return_const(true);

        let archive = Archive::open_with_lib("/dummy/path", lib).unwrap();
        let mut files = archive.files();

        assert_eq!(files.next(), Some("file1.txt".to_string()));
        assert_eq!(files.next(), None);
    }

    #[test]
    fn test_iterate_many_files() {
        let mut lib = MockCascLib::new();
        lib.mock_open();
        lib.mock_file_list(vec!["file1.txt", "dir/file2.dat", "another.txt"]);
        lib.expect_casc_close_storage().times(1).return_const(true);
        lib.expect_casc_find_close().times(1).return_const(true);

        let archive = Archive::open_with_lib("/dummy/path", lib).unwrap();
        let files = archive.files();

        let extracted: Vec<String> = files.collect();
        assert_eq!(
            extracted,
            vec![
                "file1.txt".to_string(),
                "dir/file2.dat".to_string(),
                "another.txt".to_string()
            ]
        );
    }

    #[test]
    fn test_read_file_success() {
        use super::casclib::MockCascLib;
        use std::io::Read;

        let mut lib = MockCascLib::new();
        lib.mock_open();
        let content = b"Hello, CASC!".to_vec();
        lib.mock_file_read(
            /* name= */ "test.txt",
            content.clone(),
            /* handle= */ 100,
        );
        lib.expect_casc_close_storage().times(1).return_const(true);

        let archive = Archive::open_with_lib("/dummy/path", lib).unwrap();
        let mut file = archive.open_file("test.txt").unwrap();

        let mut buf = Vec::new();
        file.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, content);
    }

    #[test]
    fn test_read_file_chunks() {
        use super::casclib::MockCascLib;
        use std::io::Read;

        let mut lib = MockCascLib::new();
        lib.mock_open();
        let content = vec![0u8; 100];
        lib.mock_file_read(
            /* name= */ "large.bin",
            content.clone(),
            /* handle= */ 101,
        );
        lib.expect_casc_close_storage().times(1).return_const(true);

        let archive = Archive::open_with_lib("/dummy/path", lib).unwrap();
        let mut file = archive.open_file("large.bin").unwrap();

        let mut buf = [0u8; 30];
        assert_eq!(file.read(&mut buf).unwrap(), 30);
        assert_eq!(file.read(&mut buf).unwrap(), 30);
        assert_eq!(file.read(&mut buf).unwrap(), 30);
        assert_eq!(file.read(&mut buf).unwrap(), 10);
        assert_eq!(file.read(&mut buf).unwrap(), 0);
    }

    #[test]
    fn test_archive_get_error() {
        use super::casclib::{Handle, MockCascLib};
        let mut lib = MockCascLib::default();
        lib.expect_get_casc_error().return_const(12345u32);
        lib.expect_casc_close_storage()
            .withf(|&h| h == 1 as Handle)
            .times(1)
            .return_const(true);

        let archive = Archive {
            handle: 1 as Handle,
            lib,
        };
        assert_eq!(archive.get_error(), 12345);
    }
}
