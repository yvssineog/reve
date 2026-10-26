//! Unique temporary directories used by individual analysis jobs.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::error::ParserError;

static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(0);

/// A per-analysis directory, removed automatically when it is dropped.
///
/// It contains both the temporary Ghidra project and the `.pb` export, so
/// concurrent analyses cannot overwrite one another's files.
#[derive(Debug)]
pub(crate) struct JobDirectory {
    path: PathBuf,
}

impl JobDirectory {
    /// Create a unique child directory under `projects_root`.
    pub(crate) fn create(projects_root: &Path) -> Result<Self, ParserError> {
        fs::create_dir_all(projects_root).map_err(|source| ParserError::JobDirectory {
            path: projects_root.to_path_buf(),
            source,
        })?;
        let projects_root =
            projects_root
                .canonicalize()
                .map_err(|source| ParserError::JobDirectory {
                    path: projects_root.to_path_buf(),
                    source,
                })?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        for _ in 0..100 {
            let id = NEXT_JOB_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                projects_root.join(format!("reve-job-{}-{timestamp}-{id}", std::process::id()));

            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => return Err(ParserError::JobDirectory { path, source }),
            }
        }

        Err(ParserError::JobDirectory {
            path: projects_root,
            source: io::Error::new(
                io::ErrorKind::AlreadyExists,
                "could not allocate a unique analysis directory after 100 attempts",
            ),
        })
    }

    /// Directory to pass as Ghidra's project location for this analysis.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Path where `Export.java` should write the protobuf message.
    pub(crate) fn export_path(&self) -> PathBuf {
        self.path.join("result.pb")
    }
}

impl Drop for JobDirectory {
    fn drop(&mut self) {
        // Cleanup is best-effort: Drop cannot return an error. The analysis
        // result or primary error must not be replaced by a cleanup failure.
        let _ = fs::remove_dir_all(&self.path);
    }
}
