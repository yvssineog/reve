//! High-level operation that runs Ghidra and returns the shared Rust model.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use reve_model::Program;
use sha2::{Digest, Sha256};

use crate::{
    config::GhidraConfig, error::ParserError, export, headless::run_headless, job::JobDirectory,
};

/// The exporter and schema that shape a saved analysis; editing either
/// changes every cache key, so old entries are never loaded.
const EXPORT_SCRIPT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/ghidra-extension/ghidra_scripts/Export.java"
));
const SCHEMA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../proto/engine.proto"
));

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
    /// A binary analyzed before loads from its saved export without running
    /// Ghidra. Otherwise the analysis uses a unique temporary Ghidra project
    /// directory. Ghidra imports and analyzes the input, then `Export.java`
    /// writes a protobuf message there. Rust decodes that message and saves a
    /// copy before the temporary directory is removed.
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
        let saved = self.saved_export(&input);
        if let Some(saved) = &saved
            && saved.is_file()
        {
            if let Ok(program) = export::read_export(saved) {
                return Ok(program);
            }
            let _ = fs::remove_file(saved);
        }

        let job = JobDirectory::create(&self.config.projects_dir)?;
        let export_path = job.export_path();

        // `run_headless` returns only after Ghidra exits successfully. A
        // nonzero exit status and timeout are already represented as errors.
        let _output = run_headless(&self.config, job.path(), &input, &export_path)?;

        if !export_path.is_file() {
            return Err(ParserError::ExportMissing(export_path));
        }

        let program = export::read_export(&export_path)?;
        if let Some(saved) = &saved {
            // Saving is best-effort: a cache failure must not fail an
            // analysis that succeeded.
            let temporary = saved.with_extension(job.path().file_name().unwrap_or_default());
            if save_export(&export_path, saved, &temporary).is_err() {
                let _ = fs::remove_file(&temporary);
            }
        }
        Ok(program)
    }

    /// Where the saved export for `input` lives, keyed by the binary's bytes,
    /// the exporter, the schema and the Ghidra version. `None` when saved
    /// analyses are disabled or the input can't be read.
    fn saved_export(&self, input: &Path) -> Option<PathBuf> {
        let cache_dir = self.config.cache_dir.as_ref()?;
        let mut binary = Sha256::new();
        io::copy(&mut fs::File::open(input).ok()?, &mut binary).ok()?;
        let key = Sha256::new()
            .chain_update(binary.finalize())
            .chain_update(Sha256::digest(EXPORT_SCRIPT))
            .chain_update(Sha256::digest(SCHEMA))
            .chain_update(ghidra_version(&self.config.analyze_headless))
            .finalize();
        Some(cache_dir.join(format!("{key:x}.pb")))
    }
}

/// `application.version` of the Ghidra install that owns the launcher, or an
/// empty string when it can't be read.
fn ghidra_version(analyze_headless: &Path) -> String {
    let launcher = fs::canonicalize(analyze_headless).unwrap_or(analyze_headless.to_path_buf());
    launcher
        .parent()
        .and_then(Path::parent)
        .and_then(|install| {
            fs::read_to_string(install.join("Ghidra").join("application.properties")).ok()
        })
        .and_then(|properties| {
            properties
                .lines()
                .find_map(|line| line.strip_prefix("application.version="))
                .map(str::to_owned)
        })
        .unwrap_or_default()
}

/// Copy a fresh export to `saved` through `temporary` in the same directory,
/// so a crash or timeout never leaves a partial entry that a later run loads.
fn save_export(export: &Path, saved: &Path, temporary: &Path) -> io::Result<()> {
    if let Some(cache_dir) = saved.parent() {
        fs::create_dir_all(cache_dir)?;
    }
    let mut file = fs::File::create(temporary)?;
    io::copy(&mut fs::File::open(export)?, &mut file)?;
    file.sync_all()?;
    fs::rename(temporary, saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::ConfigError,
        error::ParserError,
        export::ExportError,
        test_support::{TempDirectory, fake_headless},
    };
    use std::{fs, path::PathBuf, time::Duration};

    fn setup(mode: &str) -> (TempDirectory, GhidraConfig, PathBuf) {
        let temp = TempDirectory::new(&format!("orchestrator-{mode}")).unwrap();
        let launcher = fake_headless(temp.path(), "fake headless", mode).unwrap();
        let input = temp.path().join("input binary with spaces.elf");
        fs::write(&input, b"fake ELF input").unwrap();
        let projects = temp.path().join("projects with spaces");

        let java_home = crate::test_support::fake_jdk(temp.path()).unwrap();
        let cache = temp.path().join("cache");
        (
            temp,
            GhidraConfig::new(launcher, projects)
                .with_java_home(java_home)
                .with_cache_dir(cache),
            input,
        )
    }

    fn assert_projects_clean(config: &GhidraConfig) {
        let entries = fs::read_dir(&config.projects_dir)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            entries.is_empty(),
            "temporary analysis files were left behind"
        );
    }

    #[test]
    fn analyze_runs_headless_decodes_program_and_cleans_up() {
        let (_temp, config, input) = setup("write");
        let adapter = GhidraAdapter::new(config.clone());

        let program = adapter.analyze(&input).unwrap();

        assert_eq!(program.name, "demo");
        assert!(program.functions.is_empty());
        assert_projects_clean(&config);
    }

    #[test]
    fn analyze_reuses_the_saved_analysis_until_the_input_changes() {
        let (temp, config, input) = setup("write");
        let adapter = GhidraAdapter::new(config);
        let launches = || {
            fs::read_to_string(temp.path().join("launches"))
                .unwrap_or_default()
                .lines()
                .count()
        };

        assert_eq!(adapter.analyze(&input).unwrap().name, "demo");
        assert_eq!(adapter.analyze(&input).unwrap().name, "demo");
        assert_eq!(
            launches(),
            1,
            "second analysis should load the saved export"
        );

        fs::write(&input, b"edited fake ELF input").unwrap();
        adapter.analyze(&input).unwrap();
        assert_eq!(launches(), 2, "edited input should run Ghidra again");
    }

    #[test]
    fn analyze_rejects_missing_input_before_creating_a_job() {
        let (_temp, config, _input) = setup("write");
        let adapter = GhidraAdapter::new(config.clone());
        let missing = config.projects_dir.join("does-not-exist.elf");

        assert!(matches!(
            adapter.analyze(&missing),
            Err(ParserError::InputNotFound(_))
        ));
        assert!(!config.projects_dir.exists());
    }

    #[test]
    fn analyze_rejects_a_directory_as_input() {
        let (temp, config, _input) = setup("write");
        let adapter = GhidraAdapter::new(config);

        assert!(matches!(
            adapter.analyze(temp.path()),
            Err(ParserError::InputNotFile(_))
        ));
    }

    #[test]
    fn analyze_reports_missing_export_and_cleans_up() {
        let (_temp, config, input) = setup("missing");
        let adapter = GhidraAdapter::new(config.clone());

        assert!(matches!(
            adapter.analyze(input),
            Err(ParserError::ExportMissing(_))
        ));
        assert_projects_clean(&config);
    }

    #[test]
    fn analyze_reports_malformed_export_and_cleans_up() {
        let (_temp, config, input) = setup("invalid");
        let adapter = GhidraAdapter::new(config.clone());

        assert!(matches!(
            adapter.analyze(input),
            Err(ParserError::Export(ExportError::Protobuf(_)))
        ));
        assert_projects_clean(&config);
    }

    #[test]
    fn analyze_preserves_headless_exit_status_and_logs() {
        let (_temp, config, input) = setup("fail");
        let adapter = GhidraAdapter::new(config.clone());
        let error = adapter.analyze(input).unwrap_err();

        match error {
            ParserError::GhidraFailed {
                status,
                stdout,
                stderr,
            } => {
                assert_eq!(status.code(), Some(23));
                assert!(stdout.contains("fake stdout"));
                assert!(stderr.contains("fake stderr"));
            }
            other => panic!("expected GhidraFailed, got {other:?}"),
        }
        assert_projects_clean(&config);
    }

    #[test]
    fn analyze_stops_headless_after_the_configured_timeout() {
        let (_temp, config, input) = setup("timeout");
        let config = config.with_timeout(Duration::from_millis(200));
        let adapter = GhidraAdapter::new(config.clone());

        assert!(matches!(
            adapter.analyze(input),
            Err(ParserError::TimedOut(timeout)) if timeout == Duration::from_millis(200)
        ));
        assert_projects_clean(&config);
    }

    #[cfg(unix)]
    #[test]
    fn analyze_timeout_kills_processes_the_launcher_started() {
        let (temp, config, input) = setup("timeout");
        // macOS can take about a second to first exec a freshly written
        // script, and the fake must record its child's pid before the kill.
        let adapter = GhidraAdapter::new(config.with_timeout(Duration::from_secs(2)));

        assert!(matches!(
            adapter.analyze(input),
            Err(ParserError::TimedOut(_))
        ));

        let pid = fs::read_to_string(temp.path().join("launcher-child.pid"))
            .expect("fake launcher started its child before the timeout");
        let ps = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", pid.trim()])
            .output()
            .unwrap();
        let state = String::from_utf8_lossy(&ps.stdout);
        assert!(
            state.trim().is_empty() || state.trim_start().starts_with('Z'),
            "launcher child {} is still running (state {})",
            pid.trim(),
            state.trim()
        );
    }

    #[test]
    fn analyze_reports_invalid_configuration() {
        let (temp, config, input) = setup("write");
        let bad_config =
            GhidraConfig::new(temp.path().join("missing launcher"), &config.projects_dir);
        let adapter = GhidraAdapter::new(bad_config);

        assert!(matches!(
            adapter.analyze(input),
            Err(ParserError::Configuration(ConfigError::HeadlessNotFound(_)))
        ));
    }
}
