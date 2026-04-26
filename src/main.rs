//! Entry point for the casc-cli application.
//!
//! This crate provides a command-line interface for interacting with Blizzard
//! CASC archives. It uses `clap` for argument parsing and delegates logic
//! to specialized command modules.

mod casc;
mod commands;

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
        Commands::List { archive_dir } => commands::list::execute(&archive_dir),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_parsing_list() {
        let cli = Cli::parse_from(["casc-cli", "list", "/path/to/archive"]);
        match cli.command {
            Commands::List { archive_dir } => {
                assert_eq!(archive_dir, PathBuf::from("/path/to/archive"));
            }
        }
    }

    #[test]
    fn test_cli_parsing_alias_l() {
        let cli = Cli::parse_from(["casc-cli", "l", "/path/to/archive"]);
        match cli.command {
            Commands::List { archive_dir } => {
                assert_eq!(archive_dir, PathBuf::from("/path/to/archive"));
            }
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
