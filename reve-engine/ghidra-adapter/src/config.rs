//! Configuration and validation for the local Ghidra Headless installation.

use std::{env, error::Error, fmt, path::PathBuf, time::Duration};

/// Names of environment variables used by [`GhidraConfig::from_environment`].
pub const GHIDRA_HEADLESS_ENV: &str = "GHIDRA_HEADLESS";
pub const GHIDRA_PROJECTS_ENV: &str = "REVE_GHIDRA_PROJECTS";
const BUNDLED_JDK_RELATIVE_PATH: &str = "../../.tools/java/temurin-25";
const BUNDLED_HEADLESS_RELATIVE_PATH: &str = if cfg!(windows) {
    "../../.tools/ghidra/ghidra_12.2_PUBLIC/support/analyzeHeadless.bat"
} else {
    "../../.tools/ghidra/ghidra_12.2_PUBLIC/support/analyzeHeadless"
};

/// Paths and execution limits needed by the Ghidra adapter.
#[derive(Clone, Debug)]
pub struct GhidraConfig {
    /// Path to Ghidra's `analyzeHeadless` launcher (or `.bat` on Windows).
    pub analyze_headless: PathBuf,
    /// Project-local JDK used to launch Ghidra.
    pub java_home: PathBuf,
    /// Directory where Ghidra stores its analysis projects.
    ///
    /// The orchestrator will create this directory if it does not exist.
    pub projects_dir: PathBuf,
    /// Maximum time allowed for one Ghidra analysis.
    pub timeout: Duration,
}

impl GhidraConfig {
    /// Create a configuration with the project-local JDK and a ten-minute timeout.
    pub fn new(analyze_headless: impl Into<PathBuf>, projects_dir: impl Into<PathBuf>) -> Self {
        Self {
            analyze_headless: analyze_headless.into(),
            java_home: bundled(BUNDLED_JDK_RELATIVE_PATH),
            projects_dir: projects_dir.into(),
            timeout: Duration::from_secs(10 * 60),
        }
    }

    /// Override the project-local JDK path, mainly for embedding applications.
    pub fn with_java_home(mut self, java_home: impl Into<PathBuf>) -> Self {
        self.java_home = java_home.into();
        self
    }

    /// Override the default maximum analysis duration.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Read paths from the environment.
    ///
    /// `GHIDRA_HEADLESS` defaults to the project-local launcher under
    /// `.tools/ghidra/` when unset or empty. `REVE_GHIDRA_PROJECTS` defaults to
    /// `ghidra-projects` relative to the current working directory.
    pub fn from_environment() -> Self {
        let analyze_headless = match env::var_os(GHIDRA_HEADLESS_ENV) {
            Some(path) if !path.is_empty() => PathBuf::from(path),
            _ => bundled(BUNDLED_HEADLESS_RELATIVE_PATH),
        };
        let projects_dir = match env::var_os(GHIDRA_PROJECTS_ENV) {
            Some(path) => PathBuf::from(path),
            None => PathBuf::from("ghidra-projects"),
        };

        Self::new(analyze_headless, projects_dir)
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

        let java_executable =
            self.java_home
                .join("bin")
                .join(if cfg!(windows) { "java.exe" } else { "java" });
        if !java_executable.is_file() {
            return Err(ConfigError::JdkNotFound(self.java_home.clone()));
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

/// Invalid or incomplete Ghidra adapter configuration.
#[derive(Debug)]
pub enum ConfigError {
    /// The configured launcher path does not exist or cannot be inspected.
    HeadlessNotFound(PathBuf),
    /// The configured launcher exists but is not a regular file.
    HeadlessNotFile(PathBuf),
    /// The bundled or configured JDK does not contain its Java executable.
    JdkNotFound(PathBuf),
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
            Self::JdkNotFound(path) => write!(
                formatter,
                "project JDK was not found at {}; run scripts/bootstrap.py to install it",
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

/// Resolve a project-local tool path relative to this crate's source directory.
fn bundled(relative: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    path.canonicalize().unwrap_or(path)
}

impl Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDirectory;
    use std::fs;

    #[test]
    fn valid_launcher_and_not_yet_created_projects_directory_are_accepted() {
        let temp = TempDirectory::new("config-valid").unwrap();
        let launcher = temp.path().join("analyzeHeadless");
        fs::write(&launcher, "placeholder").unwrap();
        let java_home = crate::test_support::fake_jdk(temp.path()).unwrap();
        let projects = temp.path().join("projects that do not exist yet");

        let config = GhidraConfig::new(&launcher, &projects).with_java_home(java_home);
        assert!(config.validate().is_ok());
        assert_eq!(config.timeout, Duration::from_secs(600));
    }

    #[test]
    fn missing_launcher_is_reported() {
        let temp = TempDirectory::new("config-missing-launcher").unwrap();
        let config = GhidraConfig::new(temp.path().join("missing"), temp.path());

        assert!(matches!(
            config.validate(),
            Err(ConfigError::HeadlessNotFound(_))
        ));
    }

    #[test]
    fn launcher_directory_is_rejected() {
        let temp = TempDirectory::new("config-launcher-directory").unwrap();
        let directory = temp.path().join("not-a-launcher");
        fs::create_dir(&directory).unwrap();
        let config = GhidraConfig::new(directory, temp.path());

        assert!(matches!(
            config.validate(),
            Err(ConfigError::HeadlessNotFile(_))
        ));
    }

    #[test]
    fn missing_jdk_is_reported_with_setup_instructions() {
        let temp = TempDirectory::new("config-missing-jdk").unwrap();
        let launcher = temp.path().join("analyzeHeadless");
        fs::write(&launcher, "placeholder").unwrap();
        let missing_jdk = temp.path().join("missing-jdk");

        let config = GhidraConfig::new(&launcher, temp.path()).with_java_home(&missing_jdk);
        assert!(
            matches!(config.validate(), Err(ConfigError::JdkNotFound(path)) if path == missing_jdk)
        );
    }

    #[test]
    fn projects_path_that_is_a_file_is_rejected() {
        let temp = TempDirectory::new("config-projects-file").unwrap();
        let launcher = temp.path().join("analyzeHeadless");
        let projects = temp.path().join("projects-file");
        fs::write(&launcher, "placeholder").unwrap();
        let java_home = crate::test_support::fake_jdk(temp.path()).unwrap();
        fs::write(&projects, "not a directory").unwrap();

        let config = GhidraConfig::new(launcher, &projects).with_java_home(java_home);
        assert!(matches!(
            config.validate(),
            Err(ConfigError::ProjectsPathNotDirectory(_))
        ));
    }

    #[test]
    fn empty_projects_path_and_zero_timeout_are_rejected() {
        let temp = TempDirectory::new("config-boundaries").unwrap();
        let launcher = temp.path().join("analyzeHeadless");
        fs::write(&launcher, "placeholder").unwrap();
        let java_home = crate::test_support::fake_jdk(temp.path()).unwrap();

        let empty_projects = GhidraConfig::new(&launcher, "").with_java_home(&java_home);
        assert!(matches!(
            empty_projects.validate(),
            Err(ConfigError::EmptyProjectsPath)
        ));

        let zero_timeout = GhidraConfig::new(&launcher, temp.path())
            .with_java_home(java_home)
            .with_timeout(Duration::ZERO);
        assert!(matches!(
            zero_timeout.validate(),
            Err(ConfigError::ZeroTimeout)
        ));
    }
}
