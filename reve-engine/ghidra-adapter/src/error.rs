//! Errors produced while validating or running a Ghidra analysis.

use std::{error::Error, fmt, io, path::PathBuf, process::ExitStatus, time::Duration};

use crate::{config::ConfigError, export::ExportError};

/// Top-level error returned by the Ghidra adapter.
#[derive(Debug)]
pub enum ParserError {
    /// The input binary does not exist or cannot be inspected.
    InputNotFound(PathBuf),
    /// The input exists but is not a regular file.
    InputNotFile(PathBuf),
    /// The Ghidra configuration is invalid.
    Configuration(ConfigError),
    /// Creating or preparing a per-analysis directory failed.
    JobDirectory { path: PathBuf, source: io::Error },
    /// The Ghidra process could not be started.
    ProcessStart(io::Error),
    /// Ghidra reported a non-success exit status.
    GhidraFailed {
        status: ExitStatus,
        stdout: String,
        stderr: String,
    },
    /// Ghidra did not finish before the configured timeout.
    TimedOut(Duration),
    /// Ghidra exited successfully but did not create its expected export.
    ExportMissing(PathBuf),
    /// Reading or decoding Ghidra's protobuf export failed.
    Export(ExportError),
}

impl fmt::Display for ParserError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputNotFound(path) => {
                write!(
                    formatter,
                    "input binary does not exist or cannot be accessed: {}",
                    path.display()
                )
            }
            Self::InputNotFile(path) => {
                write!(formatter, "input binary is not a file: {}", path.display())
            }
            Self::Configuration(error) => write!(formatter, "invalid Ghidra configuration: {error}"),
            Self::JobDirectory { path, source } => write!(
                formatter,
                "could not create analysis directory {}: {source}",
                path.display()
            ),
            Self::ProcessStart(error) => {
                write!(formatter, "could not start Ghidra Headless: {error}")
            }
            Self::GhidraFailed {
                status,
                stdout,
                stderr,
            } => write!(
                formatter,
                "Ghidra Headless exited with {status}; stdout: {}; stderr: {}",
                stdout.trim(),
                stderr.trim()
            ),
            Self::TimedOut(timeout) => write!(
                formatter,
                "Ghidra analysis exceeded the timeout of {} seconds",
                timeout.as_secs_f64()
            ),
            Self::ExportMissing(path) => write!(
                formatter,
                "Ghidra completed without creating the expected protobuf export: {}",
                path.display()
            ),
            Self::Export(error) => write!(formatter, "could not read Ghidra export: {error}"),
        }
    }
}

impl Error for ParserError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Configuration(error) => Some(error),
            Self::JobDirectory { source, .. } | Self::ProcessStart(source) => Some(source),
            Self::Export(error) => Some(error),
            Self::InputNotFound(_)
            | Self::InputNotFile(_)
            | Self::GhidraFailed { .. }
            | Self::TimedOut(_)
            | Self::ExportMissing(_) => None,
        }
    }
}

impl From<ConfigError> for ParserError {
    fn from(error: ConfigError) -> Self {
        Self::Configuration(error)
    }
}

impl From<ExportError> for ParserError {
    fn from(error: ExportError) -> Self {
        Self::Export(error)
    }
}
