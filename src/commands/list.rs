//! Implements the `list` command logic.

#[cfg(not(test))]
use crate::casc::Archive;
#[cfg(test)]
use crate::casc::mock::MockArchive as Archive;
use crate::targets::TargetMatcher;

use std::io::Write;
use std::path::Path;

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
/// A `Result` indicating success (`Ok(())`) or a `String` error message if opening
/// the archive or printing fails.
///
/// # Errors
/// Returns an error if the archive at `archive_dir` cannot be opened or if
/// there is an issue writing to standard output.
pub fn execute(archive_dir: &Path, targets: &[String]) -> Result<(), String> {
    let archive = Archive::open(archive_dir)?;
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
fn execute_internal<W: Write>(
    archive: &Archive,
    targets: &[String],
    writer: &mut W,
) -> Result<(), String> {
    let matcher = TargetMatcher::new(targets)?;

    for file in archive.files() {
        if !matcher.is_match(&file) {
            continue;
        }

        if let Err(e) = writeln!(writer, "{}", file) {
            if e.kind() == std::io::ErrorKind::BrokenPipe {
                return Ok(());
            }
            return Err(e.to_string());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::casc::mock::TEST_MUTEX;
    use mockall::predicate::eq;
    use std::path::Path;

    #[test]
    fn test_execute_open_failure() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let path = Path::new("/dummy/path");
        let ctx = Archive::open_context();
        ctx.expect()
            .with(eq(path))
            .times(1)
            .returning(|_| Err("Mock open failure".to_string()));

        let res = execute(path, &[]);
        assert_eq!(res, Err("Mock open failure".to_string()));
    }

    #[test]
    fn test_execute_open_success() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let path = Path::new("/dummy/path");
        let ctx = Archive::open_context();
        ctx.expect().with(eq(path)).times(1).returning(|_| {
            let mut a = Archive::default();
            a.expect_files()
                .times(1)
                .returning(|| Box::new(vec!["test.txt".to_string()].into_iter()));
            Ok(a)
        });

        let res = execute(path, &[]);
        assert!(res.is_ok());
    }

    #[test]
    fn test_execute_empty_archive() {
        let mut archive = Archive::default();
        archive
            .expect_files()
            .times(1)
            .returning(|| Box::new(std::iter::empty()));

        let mut output = Vec::new();
        let res = execute_internal(&archive, &[], &mut output);

        assert!(res.is_ok());
        assert!(output.is_empty());
    }

    #[test]
    fn test_execute_one_file() {
        let mut archive = Archive::default();
        archive
            .expect_files()
            .times(1)
            .returning(|| Box::new(vec!["only_one.txt".to_string()].into_iter()));

        let mut output = Vec::new();
        let res = execute_internal(&archive, &[], &mut output);

        assert!(res.is_ok());
        assert_eq!(String::from_utf8(output).unwrap(), "only_one.txt\n");
    }

    #[test]
    fn test_execute_filtering_comprehensive() {
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
        let mut archive = Archive::default();
        archive.expect_files().times(1).returning(|| {
            Box::new(vec!["file1.txt".to_string(), "dir/file2.dat".to_string()].into_iter())
        });

        let mut output = Vec::new();
        let res = execute_internal(&archive, &[], &mut output);

        assert!(res.is_ok());
        let result_str = String::from_utf8(output).unwrap();
        assert_eq!(result_str, "file1.txt\ndir/file2.dat\n");
    }

    #[test]
    fn test_execute_broken_pipe() {
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
        let res = execute_internal(&archive, &[], &mut output);

        assert!(res.is_ok());
    }

    #[test]
    fn test_execute_write_failure() {
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
        let res = execute_internal(&archive, &[], &mut output);

        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Some other error");
    }
}
