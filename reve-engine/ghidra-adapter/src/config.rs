//! Configuration and validation for the local Ghidra Headless installation.

use std::{env, error::Error, fmt, path::PathBuf, time::Duration};

/// Names of environment variables used by [`GhidraConfig::from_environment`].
pub const GHIDRA_HEADLESS_ENV: &str = "GHIDRA_HEADLESS";
pub const GHIDRA_PROJECTS_ENV: &str = "REVE_GHIDRA_PROJECTS";

/// Paths and execution limits needed by the Ghidra adapter.
#[derive(Clone, Debug)]
pub struct GhidraConfig {
    /// Path to Ghidra's `analyzeHeadless` launcher (or `.bat` on Windows).
    pub analyze_headless: PathBuf,
    /// Directory where Ghidra stores its analysis projects.
    ///
    /// The orchestrator will create this directory if it does not exist.
    pub projects_dir: PathBuf,
    /// Maximum time allowed for one Ghidra analysis.
    pub timeout: Duration,
}

impl GhidraConfig {
    /// Create a configuration with a ten-minute analysis timeout.
    pub fn new(analyze_headless: impl Into<PathBuf>, projects_dir: impl Into<PathBuf>) -> Self {
        Self {
            analyze_headless: analyze_headless.into(),
            projects_dir: projects_dir.into(),
            timeout: Duration::from_secs(10 * 60),
        }
    }

    /// Override the default maximum analysis duration.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Read paths from the environment.
    ///
    /// `GHIDRA_HEADLESS` is required. `REVE_GHIDRA_PROJECTS` defaults to
    /// `ghidra-projects` relative to the current working directory.
    pub fn from_environment() -> Result<Self, ConfigError> {
        let analyze_headless = env_path(GHIDRA_HEADLESS_ENV)?;
        let projects_dir = match env::var_os(GHIDRA_PROJECTS_ENV) {
            Some(path) => PathBuf::from(path),
            None => PathBuf::from("ghidra-projects"),
        };

        Ok(Self::new(analyze_headless, projects_dir))
    }

    /// Check the configured paths and timeout before starting an analysis.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let metadata = self
            .analyze_headless
            .metadata()
            .map_err(|_| ConfigError::HeadlessNotFound(self.analyze_headless.clone()))?;
        if !metadata.is_file() {
            return Err(ConfigError::HeadlessNotFile(self.analyze_headless.clone()));
        }

        if self.projects_dir.as_os_str().is_empty() {
            return Err(ConfigError::EmptyProjectsPath);
        }

        if self.projects_dir.exists() && !self.projects_dir.is_dir() {
            return Err(ConfigError::ProjectsPathNotDirectory(
                self.projects_dir.clone(),
            ));
        }

        if self.timeout.is_zero() {
            return Err(ConfigError::ZeroTimeout);
        }

        Ok(())
    }
}

fn env_path(name: &'static str) -> Result<PathBuf, ConfigError> {
    match env::var_os(name) {
        Some(value) if !value.is_empty() => Ok(PathBuf::from(value)),
        Some(_) | None => Err(ConfigError::MissingEnvironmentVariable(name)),
    }
}

/// Invalid or incomplete Ghidra adapter configuration.
#[derive(Debug)]
pub enum ConfigError {
    /// A required environment variable was not set or was empty.
    MissingEnvironmentVariable(&'static str),
    /// The configured launcher path does not exist or cannot be inspected.
    HeadlessNotFound(PathBuf),
    /// The configured launcher exists but is not a regular file.
    HeadlessNotFile(PathBuf),
    /// The configured projects path already exists but is not a directory.
    ProjectsPathNotDirectory(PathBuf),
    /// The projects path was empty.
    EmptyProjectsPath,
    /// A zero timeout would cause analysis to expire immediately.
    ZeroTimeout,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEnvironmentVariable(name) => {
                write!(
                    formatter,
                    "required environment variable {name} is missing or empty"
                )
            }
            Self::HeadlessNotFound(path) => write!(
                formatter,
                "Ghidra Headless launcher does not exist or cannot be accessed: {}",
                path.display()
            ),
            Self::HeadlessNotFile(path) => write!(
                formatter,
                "Ghidra Headless launcher is not a file: {}",
                path.display()
            ),
            Self::ProjectsPathNotDirectory(path) => write!(
                formatter,
                "Ghidra projects path exists but is not a directory: {}",
                path.display()
            ),
            Self::EmptyProjectsPath => write!(formatter, "Ghidra projects path cannot be empty"),
            Self::ZeroTimeout => write!(
                formatter,
                "Ghidra analysis timeout must be greater than zero"
            ),
        }
    }
}

impl Error for ConfigError {}
