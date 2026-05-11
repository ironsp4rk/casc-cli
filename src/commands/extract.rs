//! Implements the `extract` command logic.

#[cfg(not(test))]
use crate::casc::Archive;
#[cfg(test)]
use crate::casc::mock::MockArchive as Archive;

use crate::targets::TargetMatcher;
use anyhow::{Result, anyhow};
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::Ordering;

/// Executes the extract command for a given CASC archive directory.
///
/// This function opens the CASC archive located at `archive_dir`, matches internal
/// file paths against the provided `targets`, and extracts matching files to the
/// current directory while preserving their internal path structure.
///
/// # Arguments
/// * `archive_dir` - A reference to the `Path` of the CASC archive directory.
/// * `targets` - A slice of target patterns to filter the files for extraction.
///
/// # Returns
/// A `Result` indicating success (`Ok(())`) or an error if opening
/// the archive or extracting files fails.
///
/// # Errors
/// Returns an error if the archive at `archive_dir` cannot be opened, if target
/// patterns are invalid, or if any filesystem operation (creating directories,
/// creating files, or writing data) fails.
pub fn execute(archive_dir: &Path, targets: &[String]) -> Result<()> {
    let archive = Archive::open(archive_dir).map_err(|e| anyhow!(e))?;
    execute_internal(&archive, targets, Path::new("."), &mut io::stdout())
}

/// Internal execution handler allowing injection of the output directory and writer for testing.
///
/// This separation allows unit tests to verify the extraction logic and output
/// without interacting with the real `stdout` or the current working directory.
///
/// # Arguments
/// * `archive` - A reference to the `Archive` instance (or its mock).
/// * `targets` - A slice of target patterns to filter the files for extraction.
/// * `output_dir` - The base local directory where files should be extracted.
/// * `writer` - A mutable reference to a type implementing `io::Write` (e.g., `stdout` or a `Vec<u8>`).
///
/// # Returns
/// A `Result` indicating success or an error message.
fn execute_internal<W: io::Write>(
    archive: &Archive,
    targets: &[String],
    output_dir: &Path,
    writer: &mut W,
) -> Result<()> {
    let matcher = TargetMatcher::new(targets).map_err(|e| anyhow!(e))?;

    let mut extracted_count = 0;
    // 64KB buffer for chunked reading
    let mut buffer = [0u8; 64 * 1024];

    for path in archive.files() {
        if crate::CANCELLED.load(Ordering::Relaxed) {
            return Err(anyhow!(crate::AppError::Cancelled("Extraction")));
        }

        if matcher.is_match(&path) {
            // Strip any namespace prefix (e.g., "data:") for local file creation
            let local_path_str = if let Some(colon_idx) = path.find(':') {
                &path[colon_idx + 1..]
            } else {
                &path
            };

            // Normalize slashes for the local filesystem
            let local_path_normalized = local_path_str.replace('\\', "/");
            let local_path_relative = Path::new(&local_path_normalized);
            let local_path = output_dir.join(local_path_relative);

            // Create parent directories if they don't exist
            if let Some(parent) = local_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    anyhow!("Failed to create directory '{}': {}", parent.display(), e)
                })?;
            }

            // Extract the file
            let mut archive_file = archive.open_file(&path).map_err(|e| anyhow!(e))?;
            let mut out_file = fs::File::create(&local_path)
                .map_err(|e| anyhow!("Failed to create file '{}': {}", local_path.display(), e))?;

            // Chunked read/write loop to allow cancellation mid-file
            loop {
                if crate::CANCELLED.load(Ordering::Relaxed) {
                    // Clean up partially written file
                    drop(out_file);
                    let _ = fs::remove_file(&local_path);
                    return Err(anyhow!(crate::AppError::Cancelled("Extraction")));
                }

                let bytes_read = archive_file
                    .read(&mut buffer)
                    .map_err(|e| anyhow!("Failed to read from archive file '{}': {}", path, e))?;

                if bytes_read == 0 {
                    break; // EOF
                }

                out_file.write_all(&buffer[..bytes_read]).map_err(|e| {
                    anyhow!(
                        "Failed to write to local file '{}': {}",
                        local_path.display(),
                        e
                    )
                })?;
            }

            writeln!(writer, "Extracted: {}", path)?;
            extracted_count += 1;
        }
    }

    if extracted_count == 0 && !targets.is_empty() {
        writeln!(writer, "No files matched the provided targets.")?;
    } else if extracted_count > 0 {
        writeln!(
            writer,
            "\nSuccessfully extracted {} files.",
            extracted_count
        )?;
    }

    Ok(())
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

        let mut output = Vec::new();
        let res = execute_internal(&archive, &[], temp_dir, &mut output);
        assert!(res.is_ok());

        let extracted_file = temp_dir.join("test.txt");
        assert!(extracted_file.exists());
        assert_eq!(fs::read_to_string(extracted_file).unwrap(), "hello");

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Extracted: test.txt"));
        assert!(output_str.contains("Successfully extracted 1 files."));

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

        let mut output = Vec::new();
        let res = execute_internal(&archive, &[], temp_dir, &mut output);
        assert!(res.is_ok());

        let extracted_file = temp_dir.join("folder/file.dat");
        assert!(extracted_file.exists());
        assert_eq!(fs::read(extracted_file).unwrap(), vec![1, 2, 3]);

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Extracted: data:folder/file.dat"));

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

        let mut output = Vec::new();
        let res = execute_internal(
            &archive,
            &["matching.txt".to_string()],
            temp_dir,
            &mut output,
        );
        assert!(res.is_ok());

        assert!(!temp_dir.join("other.txt").exists());

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("No files matched the provided targets."));

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

        let mut output = Vec::new();
        let res = execute_internal(&archive, &["*.txt".to_string()], temp_dir, &mut output);
        assert!(res.is_ok());

        assert_eq!(fs::read_to_string(temp_dir.join("a.txt")).unwrap(), "a");
        assert_eq!(fs::read_to_string(temp_dir.join("b.txt")).unwrap(), "b");
        assert!(!temp_dir.join("c.dat").exists());

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Extracted: a.txt"));
        assert!(output_str.contains("Extracted: b.txt"));
        assert!(output_str.contains("Successfully extracted 2 files."));

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

        let mut output = Vec::new();
        let res = execute_internal(&archive, &[], temp_dir, &mut output);
        assert!(res.is_ok());

        let extracted_file = temp_dir.join("data/sub/file.txt");
        assert!(extracted_file.exists());
        assert_eq!(fs::read_to_string(extracted_file).unwrap(), "content");

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Extracted: data\\sub\\file.txt"));

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

        let res = execute(path, &[]);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().to_string(), "Mock open failure");
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

        let mut output = Vec::new();
        let res = execute_internal(&archive, &[], temp_dir, &mut output);
        assert!(res.is_err());
        let err = res.unwrap_err();
        if let Some(app_err) = err.downcast_ref::<crate::AppError>() {
            match app_err {
                crate::AppError::Cancelled(op) => assert_eq!(*op, "Extraction"),
            }
        } else {
            panic!("Expected AppError::Cancelled");
        }

        assert!(output.is_empty());
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

        let mut output = Vec::new();
        let res = execute_internal(&archive, &[], temp_dir, &mut output);
        assert!(res.is_err());
        let err = res.unwrap_err();
        if let Some(app_err) = err.downcast_ref::<crate::AppError>() {
            match app_err {
                crate::AppError::Cancelled(op) => assert_eq!(*op, "Extraction"),
            }
        } else {
            panic!("Expected AppError::Cancelled");
        }

        // Partial file should have been deleted
        assert!(!temp_dir.join("bigfile.bin").exists());

        crate::CANCELLED.store(false, Ordering::SeqCst);
        fs::remove_dir_all(temp_dir).unwrap();
    }
}

