//! Implements the `list` command logic.

#[cfg(not(test))]
use crate::casc::Archive;
#[cfg(test)]
use crate::casc::mock::MockArchive as Archive;
use crate::targets::TargetMatcher;

use anyhow::{Result, anyhow};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::Ordering;

/// Executes the list command for a given CASC archive directory.
///
/// This function opens the CASC archive located at `archive_dir`, iterates
/// through all contained files, and prints their internal paths to standard output,
/// optionally filtered by `targets`.
///
/// # Arguments
/// * `archive_dir` - A reference to the `Path` of the CASC archive directory.
/// * `targets` - A slice of target patterns to filter the output.
///
/// # Returns
/// A `Result` indicating success (`Ok(())`) or an error if opening
/// the archive or printing fails.
///
/// # Errors
/// Returns an error if the archive at `archive_dir` cannot be opened or if
/// there is an issue writing to standard output.
pub fn execute(archive_dir: &Path, targets: &[String]) -> Result<()> {
    let archive = Archive::open(archive_dir).map_err(|e| anyhow!(e))?;
    execute_internal(&archive, targets, &mut std::io::stdout())
}

/// Internal execution handler allowing injection of the output writer for testing.
///
/// This separation allows unit tests to verify the output without interacting
/// with the real `stdout`.
///
/// # Arguments
/// * `archive` - A reference to the `Archive` instance (or its mock).
/// * `targets` - A slice of target patterns to filter the output.
/// * `writer` - A mutable reference to a type implementing `Write` (e.g., `stdout` or a `Vec<u8>`).
///
/// # Returns
/// A `Result` indicating success or an error message.
fn execute_internal<W: Write>(archive: &Archive, targets: &[String], writer: &mut W) -> Result<()> {
    let matcher = TargetMatcher::new(targets).map_err(|e| anyhow!(e))?;

    for file in archive.files() {
        // Exit early if the user pressed Ctrl+C
        if crate::CANCELLED.load(Ordering::Relaxed) {
            return Err(anyhow!(crate::AppError::Cancelled(
                /* op= */ "Listing"
            )));
        }

        if !matcher.is_match(&file) {
            continue;
        }

        writeln!(writer, "{}", file)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::casc::mock::TEST_MUTEX;
    use crate::tests::CANCEL_MUTEX;
    use mockall::predicate::eq;
    use std::path::Path;
    use std::sync::atomic::Ordering;

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

        let res = execute(path, /* targets= */ &[]);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().to_string(), "Mock open failure");
    }

    #[test]
    fn test_execute_open_success() {
        let _cancel_lock = CANCEL_MUTEX.lock().unwrap();
        let _test_lock = TEST_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let path = Path::new("/dummy/path");
        let ctx = Archive::open_context();
        ctx.expect().with(eq(path)).times(1).returning(|_| {
            let mut a = Archive::default();
            a.expect_files()
                .times(1)
                .returning(|| Box::new(vec!["test.txt".to_string()].into_iter()));
            Ok(a)
        });

        let res = execute(path, /* targets= */ &[]);
        assert!(res.is_ok());
    }

    #[test]
    fn test_execute_empty_archive() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive
            .expect_files()
            .times(1)
            .returning(|| Box::new(std::iter::empty()));

        let mut output = Vec::new();
        let res = execute_internal(&archive, /* targets= */ &[], &mut output);

        assert!(res.is_ok());
        assert!(output.is_empty());
    }

    #[test]
    fn test_execute_one_file() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive
            .expect_files()
            .times(1)
            .returning(|| Box::new(vec!["only_one.txt".to_string()].into_iter()));

        let mut output = Vec::new();
        let res = execute_internal(&archive, /* targets= */ &[], &mut output);

        assert!(res.is_ok());
        assert_eq!(String::from_utf8(output).unwrap(), "only_one.txt\n");
    }

    #[test]
    fn test_execute_filtering_comprehensive() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive.expect_files().times(1).returning(|| {
            Box::new(
                vec![
                    "data:config.ini".to_string(),
                    "data:locales/enus/main.txt".to_string(),
                    "data:locales/zhtw/main.dc6".to_string(),
                    "root.txt".to_string(),
                    "other/file.dat".to_string(),
                ]
                .into_iter(),
            )
        });

        let mut output = Vec::new();
        let targets = vec!["*.txt".to_string(), "locales/zhtw/".to_string()];
        let res = execute_internal(&archive, &targets, &mut output);

        assert!(res.is_ok());
        let result_str = String::from_utf8(output).unwrap();
        assert_eq!(
            result_str,
            "data:locales/enus/main.txt\ndata:locales/zhtw/main.dc6\nroot.txt\n"
        );
    }

    #[test]
    fn test_execute_multiple_files() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        let mut archive = Archive::default();
        archive.expect_files().times(1).returning(|| {
            Box::new(vec!["file1.txt".to_string(), "dir/file2.dat".to_string()].into_iter())
        });

        let mut output = Vec::new();
        let res = execute_internal(&archive, /* targets= */ &[], &mut output);

        assert!(res.is_ok());
        let result_str = String::from_utf8(output).unwrap();
        assert_eq!(result_str, "file1.txt\ndir/file2.dat\n");
    }

    #[test]
    fn test_execute_broken_pipe() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        struct BrokenPipeWriter;
        impl Write for BrokenPipeWriter {
            fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "Broken pipe",
                ))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let mut archive = Archive::default();
        archive
            .expect_files()
            .times(1)
            .returning(|| Box::new(vec!["file1.txt".to_string()].into_iter()));

        let mut output = BrokenPipeWriter;
        let res = execute_internal(&archive, /* targets= */ &[], &mut output);

        // BrokenPipe error will now be returned up the chain.
        assert!(res.is_err());
        let err_ref = res.as_ref().unwrap_err();
        if let Some(io_err) = err_ref.downcast_ref::<std::io::Error>() {
            assert_eq!(io_err.kind(), std::io::ErrorKind::BrokenPipe);
        } else {
            panic!("Expected BrokenPipe error");
        }
    }

    #[test]
    fn test_execute_write_failure() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        crate::CANCELLED.store(false, Ordering::SeqCst);
        struct FailingWriter;
        impl Write for FailingWriter {
            fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Some other error",
                ))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let mut archive = Archive::default();
        archive
            .expect_files()
            .times(1)
            .returning(|| Box::new(vec!["file1.txt".to_string()].into_iter()));

        let mut output = FailingWriter;
        let res = execute_internal(&archive, /* targets= */ &[], &mut output);

        assert!(res.is_err());
        assert_eq!(res.unwrap_err().to_string(), "Some other error");
    }

    #[test]
    fn test_execute_internal_cancelled() {
        let _lock = CANCEL_MUTEX.lock().unwrap();
        let mut archive = Archive::default();
        archive
            .expect_files()
            .times(1)
            .returning(|| Box::new(vec!["file1.txt".to_string()].into_iter()));

        crate::CANCELLED.store(true, Ordering::SeqCst);
        let mut output = Vec::new();
        let res = execute_internal(&archive, /* targets= */ &[], &mut output);

        assert!(res.is_err());
        let err = res.unwrap_err();
        if let Some(app_err) = err.downcast_ref::<crate::AppError>() {
            match app_err {
                crate::AppError::Cancelled(op) => assert_eq!(*op, "Listing"),
            }
        } else {
            panic!("Expected AppError::Cancelled");
        }

        assert!(output.is_empty());

        crate::CANCELLED.store(false, Ordering::SeqCst);
    }
}
