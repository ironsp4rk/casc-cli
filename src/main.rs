//! Entry point for the casc-cli application.
//!
//! This crate provides a command-line interface for interacting with Blizzard
//! CASC archives. It uses `clap` for argument parsing and delegates logic
//! to specialized command modules.

mod casc;
mod commands;
mod targets;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// Internal debug flag to enable verbose error output during development.
const DEBUG: bool = false;

/// Application-specific error types.
#[derive(Debug)]
pub enum AppError {
    /// Operation was cancelled by the user (e.g., via Ctrl+C).
    Cancelled(&'static str),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Cancelled(op) => write!(f, "{} cancelled by user", op),
        }
    }
}

impl std::error::Error for AppError {}

/// Global cancellation flag. Set to true on SIGINT (Ctrl+C).
pub static CANCELLED: AtomicBool = AtomicBool::new(false);

/// Command-line argument structure for the `casc-cli` application.
#[derive(Parser)]
#[command(
    name = "casc-cli",
    about = "Cross-platform CLI tool for Blizzard CASC archives"
)]
struct Cli {
    /// The subcommand to execute.
    #[command(subcommand)]
    command: Commands,
}

/// The set of available subcommands for `casc-cli`.
#[derive(Subcommand, Debug, PartialEq)]
enum Commands {
    /// List the contents of the CASC archive.
    #[command(alias = "l")]
    List {
        /// Path to the CASC archive directory on the local filesystem.
        archive_dir: PathBuf,

        /// Optional targets to filter the listed files.
        ///
        /// Targets can be exact paths, directory namespaces (ending in / or \), or glob patterns.
        /// The glob syntax is identical to the one used by `ripgrep` and `gitignore` files.
        ///
        /// Note: CASC archives often use namespace prefixes (e.g., `data:`). You can omit these prefixes
        /// in your targets, and the tool will automatically attempt to match the path without it.
        ///
        /// Full syntax documentation: <https://docs.rs/globset/latest/globset/#syntax>
        ///
        /// Examples:
        ///   casc-cli list ./Data                           (List all files)
        ///   casc-cli list ./Data data/global/              (List everything in data/global/)
        ///   casc-cli list ./Data '*.txt'                   (List all text files anywhere)
        ///   casc-cli list ./Data 'data/global/**/*.txt'    (List all text files in data/global/ and subdirectories)
        #[arg(verbatim_doc_comment)]
        targets: Vec<String>,
    },

    /// Extract files from the CASC archive.
    #[command(alias = "x")]
    Extract {
        /// Path to the CASC archive directory on the local filesystem.
        archive_dir: PathBuf,

        /// Targets to extract from the archive.
        ///
        /// Targets can be exact paths, directory namespaces (ending in / or \), or glob patterns.
        ///
        /// Examples:
        ///   casc-cli extract ./Data data/global/excel/weapons.txt
        ///   casc-cli extract ./Data data/global/excel/
        ///   casc-cli extract ./Data '*.txt'
        #[arg(verbatim_doc_comment)]
        targets: Vec<String>,

        /// Output directory where files will be extracted.
        #[arg(short = 'o', long = "output", default_value = ".")]
        output: PathBuf,

        /// Strip internal directory structures and extract all files directly into the root of the output directory.
        #[arg(short = 'f', long = "flatten")]
        flatten: bool,
    },
}

/// Main entry point of the application.
///
/// Parses the command-line arguments and invokes the primary execution handler.
fn main() {
    // Set up the global signal handler for Ctrl+C.
    ctrlc::set_handler(move || {
        CANCELLED.store(true, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        std::process::exit(handle_error(e));
    }
}

/// Handles an application error and returns the appropriate exit code.
///
/// Broken pipe errors and user cancellations are handled gracefully by returning
/// an exit code of 0. All other errors result in an exit code of 1.
fn handle_error(e: anyhow::Error) -> i32 {
    // Attempt to downcast the error to an `std::io::Error` to check for BrokenPipe.
    if let Some(io_err) = e.downcast_ref::<std::io::Error>()
        && io_err.kind() == std::io::ErrorKind::BrokenPipe
    {
        return 0;
    }

    // Check for structured application errors (e.g., Cancellation).
    if let Some(app_err) = e.downcast_ref::<AppError>() {
        match app_err {
            AppError::Cancelled(_) => {
                if DEBUG {
                    eprintln!("Debug: {}", e);
                }
                return 0;
            }
        }
    }

    eprintln!("Error: {}", e);
    1
}

/// Primary execution handler that dispatches work based on the parsed CLI command.
///
/// # Arguments
/// * `cli` - The parsed `Cli` arguments.
///
/// # Returns
/// A `Result` indicating success (`Ok(())`) or an error.
///
/// # Errors
/// Returns an error if the requested subcommand fails to execute.
fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::List {
            archive_dir,
            targets,
        } => commands::list::execute(&archive_dir, &targets),
        Commands::Extract {
            archive_dir,
            targets,
            output,
            flatten,
        } => commands::extract::execute(&archive_dir, &targets, &output, flatten),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use clap::CommandFactory;
    use std::sync::Mutex;

    /// Global mutex to synchronize tests that interact with the `CANCELLED` flag.
    pub static CANCEL_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_handle_error_broken_pipe() {
        let err = anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "Broken pipe",
        ));
        assert_eq!(handle_error(err), 0);
    }

    #[test]
    fn test_handle_error_other() {
        let err = anyhow::Error::msg("Some other error");
        assert_eq!(handle_error(err), 1);
    }

    #[test]
    fn test_handle_error_cancellation() {
        let err = anyhow::Error::new(AppError::Cancelled("Listing"));
        assert_eq!(handle_error(err), 0);
        let err = anyhow::Error::new(AppError::Cancelled("Extraction"));
        assert_eq!(handle_error(err), 0);
    }

    #[test]
    fn test_cli_parsing_list() {
        let cli = Cli::parse_from(["casc-cli", "list", "/path/to/archive", "target1", "target2"]);
        match cli.command {
            Commands::List {
                archive_dir,
                targets,
            } => {
                assert_eq!(archive_dir, PathBuf::from("/path/to/archive"));
                assert_eq!(targets, vec!["target1", "target2"]);
            }
            _ => panic!("Expected List subcommand"),
        }
    }

    #[test]
    fn test_cli_parsing_alias_l() {
        let cli = Cli::parse_from(["casc-cli", "l", "/path/to/archive"]);
        match cli.command {
            Commands::List {
                archive_dir,
                targets,
            } => {
                assert_eq!(archive_dir, PathBuf::from("/path/to/archive"));
                assert!(targets.is_empty());
            }
            _ => panic!("Expected List subcommand"),
        }
    }

    #[test]
    fn test_cli_parsing_extract() {
        let cli = Cli::parse_from(["casc-cli", "extract", "/path/to/archive", "target1"]);
        match cli.command {
            Commands::Extract {
                archive_dir,
                targets,
                output,
                flatten,
            } => {
                assert_eq!(archive_dir, PathBuf::from("/path/to/archive"));
                assert_eq!(targets, vec!["target1"]);
                assert_eq!(output, PathBuf::from("."));
                assert!(!flatten);
            }
            _ => panic!("Expected Extract subcommand"),
        }
    }

    #[test]
    fn test_cli_missing_arg() {
        let res = Cli::try_parse_from(["casc-cli", "list"]);
        assert!(res.is_err());
    }

    #[test]
    fn test_cli_invalid_subcommand() {
        let res = Cli::try_parse_from(["casc-cli", "invalid"]);
        assert!(res.is_err());
    }

    #[test]
    fn test_cli_help() {
        Cli::command().debug_assert();
    }
}
