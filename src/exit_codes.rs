//! Centralized exit code constants for the application.
//!
//! These codes follow standard Unix conventions where possible.

/// All files processed successfully.
pub const SUCCESS: i32 = 0;

/// No files matched the provided targets.
pub const NO_MATCHES: i32 = 1;

/// One or more files were skipped (e.g., due to flatten conflicts).
pub const WARNING: i32 = 2;

/// At least one file failed to process or another fatal error occurred.
pub const ERROR: i32 = 3;

/// The process was terminated by SIGINT (Ctrl+C).
pub const SIGINT: i32 = 130;

/// The process was terminated by SIGPIPE (e.g., pipe closed).
pub const SIGPIPE: i32 = 141;
