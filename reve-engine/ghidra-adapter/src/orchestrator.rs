//! High-level operation that runs Ghidra and returns the shared Rust model.

use std::{fs, path::Path};

use reve_model::Program;

use crate::{
    config::GhidraConfig, error::ParserError, export, headless::run_headless, job::JobDirectory,
};

/// Coordinates one complete Ghidra analysis and protobuf import.
#[derive(Clone, Debug)]
pub struct GhidraAdapter {
    config: GhidraConfig,
}

impl GhidraAdapter {
    /// Create an adapter using the supplied Ghidra installation and settings.
    ///
    /// This only stores the configuration; it does not launch Ghidra.
    pub fn new(config: GhidraConfig) -> Self {
        Self { config }
    }

    /// Analyze an input binary and return the shared `reve_model::Program`.
    ///
    /// The analysis uses a unique temporary Ghidra project directory. Ghidra
    /// imports and analyzes the input, then `Export.java` writes a protobuf
    /// message there. Rust decodes that message before the temporary directory
    /// is removed.
    pub fn analyze(&self, input: impl AsRef<Path>) -> Result<Program, ParserError> {
        self.config.validate()?;

        let input = input.as_ref();
        let metadata =
            fs::metadata(input).map_err(|_| ParserError::InputNotFound(input.to_path_buf()))?;
        if !metadata.is_file() {
            return Err(ParserError::InputNotFile(input.to_path_buf()));
        }

        let input =
            fs::canonicalize(input).map_err(|_| ParserError::InputNotFound(input.to_path_buf()))?;
        let job = JobDirectory::create(&self.config.projects_dir)?;
        let export_path = job.export_path();

        // `run_headless` returns only after Ghidra exits successfully. A
        // nonzero exit status and timeout are already represented as errors.
        let _output = run_headless(&self.config, job.path(), &input, &export_path)?;

        if !export_path.is_file() {
            return Err(ParserError::ExportMissing(export_path));
        }

        export::read_export(&export_path).map_err(ParserError::from)
    }
}
