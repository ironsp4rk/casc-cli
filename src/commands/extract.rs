//! Implements the `extract` command logic.

#[cfg(not(test))]
use crate::casc::Archive;
#[cfg(test)]
use crate::casc::mock::MockArchive as Archive;

use crate::targets::TargetMatcher;
use anyhow::{Result, anyhow};
use std::collections::HashSet;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::Ordering;

/// Executes the extract command for a given CASC archive directory.
///
/// This function opens the CASC archive located at `archive_dir`, matches internal
/// file paths against the provided `targets`, and extracts matching files to the
/// `output_dir`. If `flatten` is false, it preserves their internal path structure;
/// otherwise, it extracts them directly into the root of `output_dir`.
///
/// # Arguments
/// * `archive_dir` - A reference to the `Path` of the CASC archive directory.
/// * `targets` - A slice of target patterns to filter the files for extraction.
/// * `output_dir` - The base local directory where files should be extracted.
/// * `flatten` - If true, strips internal directory structure and extracts all files to the root of `output_dir`.
///
/// # Returns
/// A `Result` containing the exit code or an error message.
pub fn execute(
    archive_dir: &Path,
    targets: &[String],
    output_dir: &Path,
    flatten: bool,
) -> Result<i32> {
    let archive = Archive::open(archive_dir).map_err(|e| anyhow!(e))?;
    execute_internal(
        &archive,
        targets,
        output_dir,
        &mut io::stdout(),
        &mut io::stderr(),
        flatten,
    )
}

/// Internal execution handler allowing dependency injection for testing.
///
/// See [`execute`] for general behavior and all arguments.
///
/// # Arguments
/// * `archive` - A reference to the `Archive` instance (or its mock).
/// * `stdout` - A mutable reference to a type implementing `io::Write` (e.g., `stdout` or a `Vec<u8>`).
/// * `stderr` - A mutable reference to a type implementing `io::Write` (e.g., `stderr` or a `Vec<u8>`).
///
/// # Returns
/// A `Result` containing the exit code or an error message.
fn execute_internal<W1: io::Write, W2: io::Write>(
    archive: &Archive,
    targets: &[String],
    output_dir: &Path,
    stdout: &mut W1,
    stderr: &mut W2,
    flatten: bool,
) -> Result<i32> {
    let matcher = TargetMatcher::new(targets).map_err(|e| anyhow!(e))?;
    let mut extractor = Extractor::new(archive, output_dir, flatten);

    for path in archive.files() {
        if crate::CANCELLED.load(Ordering::Relaxed) {
            return Err(anyhow!(crate::AppError::Cancelled(
                /* op= */ "Extraction"
            )));
        }

        if matcher.is_match(&path) {
            writeln!(stdout, "Extracting {}", path)?;

            match extractor.extract_path(&path, stderr) {
                Ok(_) => {}
                Err(e) => {
                    if let Some(app_err) = e.downcast_ref::<crate::AppError>()
                        && matches!(app_err, crate::AppError::Cancelled(_))
                    {
                        return Err(e);
                    }
                    writeln!(stderr, "{}", e)?;
                }
            }
        }
    }

    if extractor.extracted_count == 0 && extractor.skipped_count == 0 && extractor.failed_count == 0
    {
        writeln!(stdout, "No matches.")?;
    } else {
        write!(stdout, "Extracted {} files", extractor.extracted_count)?;
        if extractor.skipped_count > 0 || extractor.failed_count > 0 {
            write!(stdout, " (")?;
            if extractor.skipped_count > 0 {
                write!(stdout, "{} skipped", extractor.skipped_count)?;
                if extractor.failed_count > 0 {
                    write!(stdout, ", ")?;
                }
            }
            if extractor.failed_count > 0 {
                write!(stdout, "{} failed", extractor.failed_count)?;
            }
            write!(stdout, ")")?;
        }
        writeln!(stdout, ".")?;
    }

    if extractor.failed_count > 0 {
        Ok(crate::exit_codes::ERROR)
    } else if extractor.skipped_count > 0 {
        Ok(crate::exit_codes::WARNING)
    } else if extractor.extracted_count == 0 && !targets.is_empty() {
        Ok(crate::exit_codes::NO_MATCHES)
    } else {
        Ok(crate::exit_codes::SUCCESS)
    }
}

/// Encapsulates the state and logic for extracting files from a CASC archive.
///
/// The `Extractor` manages the session state, including the set of files already extracted
/// (to prevent collisions when flattening) and the statistics counters. It also owns
/// a reusable 64KB I/O buffer to minimize allocations during the extraction process.
struct Extractor<'a> {
    /// Reference to the opened CASC archive.
    archive: &'a Archive,
    /// The base directory for extraction on the local filesystem.
    output_dir: &'a Path,
    /// Whether to strip internal directory structures.
    flatten: bool,
    /// Total number of files successfully extracted in this session.
    extracted_count: usize,
    /// Total number of files skipped due to intra-session conflicts (flatten collisions).
    skipped_count: usize,
    /// Total number of files that failed to be extracted.
    failed_count: usize,
    /// Set of filenames (base names) already extracted, used for collision detection when flattening.
    extracted_filenames: HashSet<String>,
    /// Reusable 64KB buffer for chunked reading and writing.
    buffer: [u8; 64 * 1024],
}

impl<'a> Extractor<'a> {
    /// Creates a new `Extractor` instance for the given archive and output directory.
    fn new(archive: &'a Archive, output_dir: &'a Path, flatten: bool) -> Self {
        Self {
            archive,
            output_dir,
            flatten,
            extracted_count: 0,
            skipped_count: 0,
            failed_count: 0,
            extracted_filenames: HashSet::new(),
            buffer: [0u8; 64 * 1024],
        }
    }

    /// Processes a single archive path, matching it against targets and extracting it if necessary.
    ///
    /// This method handles path normalization, collision detection (when flattening),
    /// and directory creation. If an extraction error occurs, it is logged to `stderr`
    /// and any partially written file is cleaned up, but the loop continues unless
    /// the error is a user cancellation.
    ///
    /// # Arguments
    /// * `path` - The internal path of the file within the CASC archive.
    /// * `stderr` - A mutable reference to a writer for error messages.
    ///
    /// # Returns
    /// A `Result` indicating success (extraction finished or skipped) or a fatal error (e.g., cancellation).
    fn extract_path<W: io::Write>(&mut self, path: &str, stderr: &mut W) -> Result<()> {
        // Strip any namespace prefix (e.g., "data:") for local file creation
        let local_path_str = if let Some(colon_idx) = path.find(':') {
            &path[colon_idx + 1..]
        } else {
            path
        };

        // Normalize slashes for the local filesystem
        let local_path_normalized = local_path_str.replace('\\', "/");
        let local_path_relative = Path::new(&local_path_normalized);

        let local_path = if self.flatten {
            let filename = match local_path_relative.file_name().and_then(|f| f.to_str()) {
                Some(f) => f,
                None => {
                    writeln!(
                        stderr,
                        "ERROR: Failed to extract filename from path: {}",
                        path
                    )?;
                    self.failed_count += 1;
                    return Ok(());
                }
            };

            if self.extracted_filenames.contains(filename) {
                writeln!(stderr, "WARN: Skipped '{}' (Conflict/Exists)", path)?;
                self.skipped_count += 1;
                return Ok(());
            }

            self.extracted_filenames.insert(filename.to_string());
            self.output_dir.join(filename)
        } else {
            self.output_dir.join(local_path_relative)
        };

        // Create parent directories if they don't exist
        if let Some(parent) = local_path.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            writeln!(
                stderr,
                "ERROR: Failed to create directory '{}': {}",
                parent.display(),
                e
            )?;
            self.failed_count += 1;
            return Ok(());
        }

        // Extract the actual file
        match self.extract_file(path, &local_path) {
            Ok(_) => {
                self.extracted_count += 1;
                Ok(())
            }
            Err(e) => {
                // If it's a cancellation, we pass it up to stop the entire loop
                if let Some(app_err) = e.downcast_ref::<crate::AppError>()
                    && matches!(app_err, crate::AppError::Cancelled(_))
                {
                    return Err(e);
                }
                // Otherwise, log and continue
                self.failed_count += 1;
                let _ = fs::remove_file(&local_path);
                Err(e)
            }
        }
    }

    /// Performs the low-level extraction of a single file.
    ///
    /// Opens the file in the archive, creates the local file, and copies the data
    /// in chunks. Checks for user cancellation between each chunk.
    ///
    /// # Arguments
    /// * `path` - The internal path of the file in the archive.
    /// * `local_path` - The absolute or relative path where the file should be written locally.
    ///
    /// # Returns
    /// `Ok(())` on success, or an `Err` if opening, reading, or writing fails.
    fn extract_file(&mut self, path: &str, local_path: &Path) -> Result<()> {
        let mut archive_file = self.archive.open_file(path).map_err(|e| {
            let code = self.archive.get_error();
            anyhow!(
                "Extraction failure (code: {}) for '{}' (open): {}",
                code,
                path,
                e
            )
        })?;

        let mut out_file = fs::File::create(local_path)
            .map_err(|e| anyhow!("Extraction failure for '{}' (create): {}", path, e))?;

        // Chunked read/write loop to allow cancellation mid-file
        loop {
            if crate::CANCELLED.load(Ordering::Relaxed) {
                // Clean up partially written file
                drop(out_file);
                let _ = fs::remove_file(local_path);
                return Err(anyhow!(crate::AppError::Cancelled(
                    /* op= */ "Extraction"
                )));
            }

            let bytes_read = archive_file.read(&mut self.buffer).map_err(|e| {
                let code = self.archive.get_error();
                anyhow!(
                    "Extraction failure (code: {}) for '{}' (read): {}",
                    code,
                    path,
                    e
                )
            })?;

            if bytes_read == 0 {
                break; // EOF
            }

            out_file
                .write_all(&self.buffer[..bytes_read])
                .map_err(|e| anyhow!("Extraction failure for '{}' (write): {}", path, e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::casc::mock::{MockArchiveFile, TEST_MUTEX};
    use crate::tests::CANCEL_MUTEX;
    use mockall::predicate::eq;
    use std::fs;
    use std::path::Path;
    use std::sync::Mutex;

    /// Helper to create a `MockArchiveFile` that returns specific data on the first read.
    fn mock_file(data: Vec<u8>) -> MockArchiveFile {
        let mut f = MockArchiveFile::default();
        let content = Mutex::new(Some(data));
        f.expect_read().returning(move |buf| {
            let mut lock = content.lock().unwrap();
            if let Some(data) = lock.take() {
                let len = std::cmp::min(data.len(), buf.len());
                buf[..len].copy_from_slice(&data[..len]);
                Ok(len)
            } else {
                Ok(0)
            }
        });
        f
    }

    #[test]
    fn test_execute_internal_happy_path() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive
            .expect_files()
            .times(1)
            .returning(|| Box::new(vec!["test.txt".to_string()].into_iter()));
        archive
            .expect_open_file()
            .with(eq("test.txt"))
            .times(1)
            .returning(|_| Ok(mock_file(b"hello".to_vec())));

        let temp_dir = Path::new("test_extract_happy");
        fs::create_dir_all(temp_dir).unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = execute_internal(
            &archive,
            /* targets= */ &[],
            temp_dir,
            &mut stdout,
            &mut stderr,
            /* flatten= */ false,
        );
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), crate::exit_codes::SUCCESS);

        let extracted_file = temp_dir.join("test.txt");
        assert!(extracted_file.exists());
        assert_eq!(fs::read_to_string(extracted_file).unwrap(), "hello");

        let output_str = String::from_utf8(stdout).unwrap();
        assert!(output_str.contains("Extracting test.txt"));
        assert!(output_str.contains("Extracted 1 files."));

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_execute_internal_with_prefix() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive
            .expect_files()
            .times(1)
            .returning(|| Box::new(vec!["data:folder/file.dat".to_string()].into_iter()));
        archive
            .expect_open_file()
            .with(eq("data:folder/file.dat"))
            .times(1)
            .returning(|_| Ok(mock_file(vec![1, 2, 3])));

        let temp_dir = Path::new("test_extract_prefix");
        fs::create_dir_all(temp_dir).unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = execute_internal(
            &archive,
            /* targets= */ &[],
            temp_dir,
            &mut stdout,
            &mut stderr,
            /* flatten= */ false,
        );
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), crate::exit_codes::SUCCESS);

        let extracted_file = temp_dir.join("folder/file.dat");
        assert!(extracted_file.exists());
        assert_eq!(fs::read(extracted_file).unwrap(), vec![1, 2, 3]);

        let output_str = String::from_utf8(stdout).unwrap();
        assert!(output_str.contains("Extracting data:folder/file.dat"));

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_execute_internal_no_match() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive
            .expect_files()
            .times(1)
            .returning(|| Box::new(vec!["other.txt".to_string()].into_iter()));

        let temp_dir = Path::new("test_extract_no_match");
        fs::create_dir_all(temp_dir).unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = execute_internal(
            &archive,
            /* targets= */ &["matching.txt".to_string()],
            temp_dir,
            &mut stdout,
            &mut stderr,
            /* flatten= */ false,
        );
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), crate::exit_codes::NO_MATCHES);

        assert!(!temp_dir.join("other.txt").exists());

        let output_str = String::from_utf8(stdout).unwrap();
        assert!(output_str.contains("No matches."));

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_execute_internal_multiple_matches() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive.expect_files().times(1).returning(|| {
            Box::new(
                vec![
                    "a.txt".to_string(),
                    "b.txt".to_string(),
                    "c.dat".to_string(),
                ]
                .into_iter(),
            )
        });

        archive
            .expect_open_file()
            .with(eq("a.txt"))
            .times(1)
            .returning(|_| Ok(mock_file(b"a".to_vec())));

        archive
            .expect_open_file()
            .with(eq("b.txt"))
            .times(1)
            .returning(|_| Ok(mock_file(b"b".to_vec())));

        let temp_dir = Path::new("test_extract_multiple");
        fs::create_dir_all(temp_dir).unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = execute_internal(
            &archive,
            /* targets= */ &["*.txt".to_string()],
            temp_dir,
            &mut stdout,
            &mut stderr,
            /* flatten= */ false,
        );
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), crate::exit_codes::SUCCESS);

        assert_eq!(fs::read_to_string(temp_dir.join("a.txt")).unwrap(), "a");
        assert_eq!(fs::read_to_string(temp_dir.join("b.txt")).unwrap(), "b");
        assert!(!temp_dir.join("c.dat").exists());

        let output_str = String::from_utf8(stdout).unwrap();
        assert!(output_str.contains("Extracting a.txt"));
        assert!(output_str.contains("Extracting b.txt"));
        assert!(output_str.contains("Extracted 2 files."));

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_execute_internal_backslash_path() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive
            .expect_files()
            .times(1)
            .returning(|| Box::new(vec!["data\\sub\\file.txt".to_string()].into_iter()));
        archive
            .expect_open_file()
            .with(eq("data\\sub\\file.txt"))
            .times(1)
            .returning(|_| Ok(mock_file(b"content".to_vec())));

        let temp_dir = Path::new("test_extract_backslash");
        fs::create_dir_all(temp_dir).unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = execute_internal(
            &archive,
            /* targets= */ &[],
            temp_dir,
            &mut stdout,
            &mut stderr,
            /* flatten= */ false,
        );
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), crate::exit_codes::SUCCESS);

        let extracted_file = temp_dir.join("data/sub/file.txt");
        assert!(extracted_file.exists());
        assert_eq!(fs::read_to_string(extracted_file).unwrap(), "content");

        let output_str = String::from_utf8(stdout).unwrap();
        assert!(output_str.contains("Extracting data\\sub\\file.txt"));

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_execute_open_failure() {
        let _cancel_lock = CANCEL_MUTEX.lock().unwrap();
        let _test_lock = TEST_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let path = Path::new("/dummy/path");
        let ctx = Archive::open_context();
        ctx.expect()
            .with(eq(path))
            .times(1)
            .returning(|_| Err("Mock open failure".to_string()));

        let res = execute(
            path,
            /* targets= */ &[],
            Path::new("."),
            /* flatten= */ false,
        );
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().to_string(), "Mock open failure");
    }

    #[test]
    fn test_execute_internal_invalid_output_dir() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive
            .expect_files()
            .returning(|| Box::new(vec!["test.txt".to_string()].into_iter()));

        // Create a file where a directory should be
        let temp_file = Path::new("test_extract_invalid_dir");
        fs::write(temp_file, "not a directory").unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = execute_internal(
            &archive,
            /* targets= */ &[],
            temp_file,
            &mut stdout,
            &mut stderr,
            /* flatten= */ false,
        );
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), crate::exit_codes::ERROR); // Failed count should be > 0

        let stderr_str = String::from_utf8(stderr).unwrap();
        assert!(
            stderr_str.contains("ERROR: Failed to create directory")
                || stderr_str.contains("Not a directory")
        );

        fs::remove_file(temp_file).unwrap();
    }

    #[test]
    fn test_execute_internal_empty_output_dir() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive
            .expect_files()
            .returning(|| Box::new(vec!["test.txt".to_string()].into_iter()));
        archive
            .expect_open_file()
            .returning(|_| Ok(mock_file(b"hello".to_vec())));

        // Empty path should join to the relative file path, effectively extracting to CWD
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = execute_internal(
            &archive,
            /* targets= */ &[],
            Path::new(""),
            &mut stdout,
            &mut stderr,
            /* flatten= */ false,
        );
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), crate::exit_codes::SUCCESS);

        assert!(Path::new("test.txt").exists());
        fs::remove_file("test.txt").unwrap();
    }

    #[test]
    fn test_execute_internal_extraction_failure_with_code() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive
            .expect_files()
            .times(1)
            .returning(|| Box::new(vec!["fail.txt".to_string()].into_iter()));

        archive
            .expect_open_file()
            .with(eq("fail.txt"))
            .times(1)
            .returning(|_| Err("Mock open failure".to_string()));

        archive.expect_get_error().times(1).returning(|| 12345);

        let temp_dir = Path::new("test_extract_fail_code");
        fs::create_dir_all(temp_dir).unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = execute_internal(
            &archive,
            /* targets= */ &[],
            temp_dir,
            &mut stdout,
            &mut stderr,
            /* flatten= */ false,
        );
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), crate::exit_codes::ERROR);

        let stderr_str = String::from_utf8(stderr).unwrap();
        assert!(stderr_str.contains("Extraction failure (code: 12345) for 'fail.txt' (open)"));

        let output_str = String::from_utf8(stdout).unwrap();
        assert!(output_str.contains("Extracted 0 files (1 failed)."));

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_execute_internal_mixed_run() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        // Alphabetical order for deterministic processing
        archive.expect_files().times(1).returning(|| {
            Box::new(
                vec![
                    "a_success.txt".to_string(),
                    "b_skip/file.txt".to_string(),
                    "c_fail.txt".to_string(),
                    "d_collision/file.txt".to_string(),
                ]
                .into_iter(),
            )
        });

        // a_success.txt: Success
        archive
            .expect_open_file()
            .with(eq("a_success.txt"))
            .times(1)
            .returning(|_| Ok(mock_file(b"ok".to_vec())));

        // b_skip/file.txt: Success
        archive
            .expect_open_file()
            .with(eq("b_skip/file.txt"))
            .times(1)
            .returning(|_| Ok(mock_file(b"first".to_vec())));

        // d_collision/file.txt: Skip (collision with b_skip/file.txt)
        archive
            .expect_open_file()
            .with(eq("d_collision/file.txt"))
            .times(0);

        // c_fail.txt: Failure
        archive
            .expect_open_file()
            .with(eq("c_fail.txt"))
            .times(1)
            .returning(|_| Err("Open error".to_string()));
        archive.expect_get_error().returning(|| 555);

        let temp_dir = Path::new("test_extract_mixed");
        fs::create_dir_all(temp_dir).unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = execute_internal(
            &archive,
            /* targets= */ &[],
            temp_dir,
            &mut stdout,
            &mut stderr,
            /* flatten= */ true,
        );
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), crate::exit_codes::ERROR); // Failure takes precedence

        let output_str = String::from_utf8(stdout).unwrap();
        let error_str = String::from_utf8(stderr).unwrap();

        assert!(output_str.contains("Extracting a_success.txt"));
        assert!(output_str.contains("Extracting b_skip/file.txt"));
        assert!(output_str.contains("Extracting c_fail.txt"));
        assert!(output_str.contains("Extracting d_collision/file.txt"));
        assert!(output_str.contains("Extracted 2 files (1 skipped, 1 failed)."));

        assert!(error_str.contains("WARN: Skipped 'd_collision/file.txt' (Conflict/Exists)"));
        assert!(error_str.contains("Extraction failure (code: 555) for 'c_fail.txt' (open)"));

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_execute_internal_read_failure_mid_file() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive
            .expect_files()
            .times(1)
            .returning(|| Box::new(vec!["bad_read.bin".to_string()].into_iter()));

        let mut mock_file = MockArchiveFile::default();
        mock_file
            .expect_read()
            .times(1)
            .returning(|_| Err(std::io::Error::other("Read error")));

        let mock_file_opt = Mutex::new(Some(mock_file));
        archive
            .expect_open_file()
            .with(eq("bad_read.bin"))
            .times(1)
            .returning(move |_| {
                Ok(mock_file_opt
                    .lock()
                    .unwrap()
                    .take()
                    .expect("Called open_file twice"))
            });

        archive.expect_get_error().returning(|| 999);

        let temp_dir = Path::new("test_extract_read_fail");
        fs::create_dir_all(temp_dir).unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = execute_internal(
            &archive,
            /* targets= */ &[],
            temp_dir,
            &mut stdout,
            &mut stderr,
            /* flatten= */ false,
        );
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), crate::exit_codes::ERROR);

        let error_str = String::from_utf8(stderr).unwrap();
        assert!(error_str.contains("Extraction failure (code: 999) for 'bad_read.bin' (read)"));

        // Ensure partial file is deleted
        assert!(!temp_dir.join("bad_read.bin").exists());

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_execute_internal_write_failure() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive
            .expect_files()
            .times(1)
            .returning(|| Box::new(vec!["fail_write.txt".to_string()].into_iter()));

        archive
            .expect_open_file()
            .returning(|_| Ok(mock_file(b"some data".to_vec())));

        let temp_dir = Path::new("test_extract_write_fail");
        fs::create_dir_all(temp_dir).unwrap();
        let target_file = temp_dir.join("fail_write.txt");

        // We can't easily make fs::File::create or write_all fail without messing with permissions
        // or using a mock filesystem. But we can create a directory where the file should be.
        fs::create_dir(&target_file).unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = execute_internal(
            &archive,
            /* targets= */ &[],
            temp_dir,
            &mut stdout,
            &mut stderr,
            /* flatten= */ false,
        );
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), crate::exit_codes::ERROR);

        let error_str = String::from_utf8(stderr).unwrap();
        // Since we created a directory at fail_write.txt, File::create should fail.
        assert!(error_str.contains("Extraction failure for 'fail_write.txt' (create)"));

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_execute_internal_filename_failure() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        // A path that has no filename (terminates in .. or is root)
        archive
            .expect_files()
            .times(1)
            .returning(|| Box::new(vec!["..".to_string()].into_iter()));

        let temp_dir = Path::new("test_extract_filename_fail");
        fs::create_dir_all(temp_dir).unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = execute_internal(
            &archive,
            /* targets= */ &[],
            temp_dir,
            &mut stdout,
            &mut stderr,
            /* flatten= */ true,
        );
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), crate::exit_codes::ERROR);

        let error_str = String::from_utf8(stderr).unwrap();
        assert!(error_str.contains("ERROR: Failed to extract filename from path: .."));

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_execute_internal_skip_only_no_match() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive.expect_files().times(1).returning(|| {
            Box::new(vec!["a/file.txt".to_string(), "b/file.txt".to_string()].into_iter())
        });

        // "a/file.txt" -> Success
        archive
            .expect_open_file()
            .returning(|_| Ok(mock_file(b"data".to_vec())));

        let temp_dir = Path::new("test_extract_skip_only");
        fs::create_dir_all(temp_dir).unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = execute_internal(
            &archive,
            /* targets= */ &["*.txt".to_string()],
            temp_dir,
            &mut stdout,
            &mut stderr,
            /* flatten= */ true,
        );
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), crate::exit_codes::WARNING); // Skip exists

        let output_str = String::from_utf8(stdout).unwrap();
        assert!(output_str.contains("Extracted 1 files (1 skipped)."));

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_execute_internal_cancelled_before() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(true, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive
            .expect_files()
            .times(1)
            .returning(|| Box::new(vec!["test.txt".to_string()].into_iter()));

        let temp_dir = Path::new("test_extract_cancel_before");
        fs::create_dir_all(temp_dir).unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = execute_internal(
            &archive,
            /* targets= */ &[],
            temp_dir,
            &mut stdout,
            &mut stderr,
            /* flatten= */ false,
        );
        assert!(res.is_err());
        let err = res.unwrap_err();
        if let Some(app_err) = err.downcast_ref::<crate::AppError>() {
            match app_err {
                crate::AppError::Cancelled(op) => assert_eq!(*op, "Extraction"),
            }
        } else {
            panic!("Expected AppError::Cancelled, got: {:?}", err);
        }

        assert!(stdout.is_empty());
        assert!(!temp_dir.join("test.txt").exists());

        crate::CANCELLED.store(false, Ordering::SeqCst);
        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_execute_internal_cancelled_mid_file() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive
            .expect_files()
            .times(1)
            .returning(|| Box::new(vec!["bigfile.bin".to_string()].into_iter()));

        let mut mock_file = MockArchiveFile::default();
        // Return some data on first read, then we'll cancel
        mock_file.expect_read().times(1).returning(|buf| {
            let data = vec![0u8; 1024];
            buf[..1024].copy_from_slice(&data);
            crate::CANCELLED.store(true, Ordering::SeqCst);
            Ok(1024)
        });

        let mock_file_opt = Mutex::new(Some(mock_file));
        archive
            .expect_open_file()
            .with(eq("bigfile.bin"))
            .times(1)
            .returning(move |_| {
                Ok(mock_file_opt
                    .lock()
                    .unwrap()
                    .take()
                    .expect("Called open_file twice"))
            });

        let temp_dir = Path::new("test_extract_cancel_mid");
        fs::create_dir_all(temp_dir).unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = execute_internal(
            &archive,
            /* targets= */ &[],
            temp_dir,
            &mut stdout,
            &mut stderr,
            /* flatten= */ false,
        );
        assert!(res.is_err());
        let err = res.unwrap_err();
        if let Some(app_err) = err.downcast_ref::<crate::AppError>() {
            match app_err {
                crate::AppError::Cancelled(op) => assert_eq!(*op, "Extraction"),
            }
        } else {
            panic!("Expected AppError::Cancelled, got: {:?}", err);
        }

        // Partial file should have been deleted
        assert!(!temp_dir.join("bigfile.bin").exists());

        crate::CANCELLED.store(false, Ordering::SeqCst);
        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_execute_internal_flatten_no_collision() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive.expect_files().times(1).returning(|| {
            Box::new(
                vec![
                    "a/test.txt".to_string(),
                    "b/other.txt".to_string(),
                    "c/d/deep.txt".to_string(),
                ]
                .into_iter(),
            )
        });

        archive
            .expect_open_file()
            .with(eq("a/test.txt"))
            .times(1)
            .returning(|_| Ok(mock_file(b"a".to_vec())));

        archive
            .expect_open_file()
            .with(eq("b/other.txt"))
            .times(1)
            .returning(|_| Ok(mock_file(b"b".to_vec())));

        archive
            .expect_open_file()
            .with(eq("c/d/deep.txt"))
            .times(1)
            .returning(|_| Ok(mock_file(b"deep".to_vec())));

        let temp_dir = Path::new("test_extract_flatten_no_collision");
        fs::create_dir_all(temp_dir).unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = execute_internal(
            &archive,
            /* targets= */ &[],
            temp_dir,
            &mut stdout,
            &mut stderr,
            /* flatten= */ true,
        );
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), crate::exit_codes::SUCCESS);

        // Files should be in the root of temp_dir
        assert_eq!(fs::read_to_string(temp_dir.join("test.txt")).unwrap(), "a");
        assert_eq!(fs::read_to_string(temp_dir.join("other.txt")).unwrap(), "b");
        assert_eq!(
            fs::read_to_string(temp_dir.join("deep.txt")).unwrap(),
            "deep"
        );

        // Subdirectories should NOT exist
        assert!(!temp_dir.join("a").exists());
        assert!(!temp_dir.join("b").exists());
        assert!(!temp_dir.join("c").exists());

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_execute_internal_flatten_collision() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive.expect_files().times(1).returning(|| {
            Box::new(vec!["a/file.txt".to_string(), "b/file.txt".to_string()].into_iter())
        });

        // First one wins
        archive
            .expect_open_file()
            .with(eq("a/file.txt"))
            .times(1)
            .returning(|_| Ok(mock_file(b"first".to_vec())));

        // Second one is skipped
        archive.expect_open_file().with(eq("b/file.txt")).times(0);

        let temp_dir = Path::new("test_extract_flatten_collision");
        fs::create_dir_all(temp_dir).unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = execute_internal(
            &archive,
            /* targets= */ &[],
            temp_dir,
            &mut stdout,
            &mut stderr,
            /* flatten= */ true,
        );
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), crate::exit_codes::WARNING);

        // Only the first file should exist
        assert_eq!(
            fs::read_to_string(temp_dir.join("file.txt")).unwrap(),
            "first"
        );

        let stderr_str = String::from_utf8(stderr).unwrap();
        assert!(stderr_str.contains("WARN: Skipped 'b/file.txt' (Conflict/Exists)"));

        let output_str = String::from_utf8(stdout).unwrap();
        assert!(output_str.contains("Extracting a/file.txt"));
        assert!(output_str.contains("Extracted 1 files (1 skipped)."));

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_execute_internal_complex_structure() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive.expect_files().times(1).returning(|| {
            Box::new(
                vec![
                    "a/file.txt".to_string(),
                    "b/file.txt".to_string(),
                    "c/d/file.txt".to_string(),
                    "e/d/file.txt".to_string(),
                ]
                .into_iter(),
            )
        });

        archive
            .expect_open_file()
            .with(eq("a/file.txt"))
            .returning(|_| Ok(mock_file(b"a".to_vec())));
        archive
            .expect_open_file()
            .with(eq("b/file.txt"))
            .returning(|_| Ok(mock_file(b"b".to_vec())));
        archive
            .expect_open_file()
            .with(eq("c/d/file.txt"))
            .returning(|_| Ok(mock_file(b"cd".to_vec())));
        archive
            .expect_open_file()
            .with(eq("e/d/file.txt"))
            .returning(|_| Ok(mock_file(b"ed".to_vec())));

        let temp_dir = Path::new("test_extract_complex");
        fs::create_dir_all(temp_dir).unwrap();

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let res = execute_internal(
            &archive,
            /* targets= */ &[],
            temp_dir,
            &mut stdout,
            &mut stderr,
            /* flatten= */ false,
        );
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), crate::exit_codes::SUCCESS);

        assert_eq!(
            fs::read_to_string(temp_dir.join("a/file.txt")).unwrap(),
            "a"
        );
        assert_eq!(
            fs::read_to_string(temp_dir.join("b/file.txt")).unwrap(),
            "b"
        );
        assert_eq!(
            fs::read_to_string(temp_dir.join("c/d/file.txt")).unwrap(),
            "cd"
        );
        assert_eq!(
            fs::read_to_string(temp_dir.join("e/d/file.txt")).unwrap(),
            "ed"
        );

        fs::remove_dir_all(temp_dir).unwrap();
    }
}
