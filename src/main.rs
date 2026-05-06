//! Entry point for the casc-cli application.
//!
//! This crate provides a command-line interface for interacting with Blizzard
//! CASC archives. It uses `clap` for argument parsing and delegates logic
//! to specialized command modules.

mod casc;
mod commands;
mod targets;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
        /// Full syntax documentation: https://docs.rs/globset/latest/globset/#syntax
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
    },
}

/// Main entry point of the application.
///
/// Parses the command-line arguments and invokes the primary execution handler.
fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

/// Primary execution handler that dispatches work based on the parsed CLI command.
///
/// # Arguments
/// * `cli` - The parsed `Cli` arguments.
///
/// # Returns
/// A `Result` indicating success (`Ok(())`) or a `String` error message.
///
/// # Errors
/// Returns an error if the requested subcommand fails to execute.
fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Commands::List {
            archive_dir,
            targets,
        } => commands::list::execute(&archive_dir, &targets),
        Commands::Extract {
            archive_dir,
            targets,
        } => commands::extract::execute(&archive_dir, &targets),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

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
            } => {
                assert_eq!(archive_dir, PathBuf::from("/path/to/archive"));
                assert_eq!(targets, vec!["target1"]);
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
