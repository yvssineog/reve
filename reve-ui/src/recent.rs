//! Persistent, app-wide list of recently selected binaries.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use gpui_kit::Global;
use serde::{Deserialize, Serialize};

const SETTINGS_VERSION: u32 = 1;
const MAX_RECENT_FILES: usize = 10;

/// One path in the recent-file list, with presentation details computed on read.
#[allow(dead_code)] // The home-screen list is added in phase 4.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentFile {
    /// Canonical path when the selected file still exists.
    pub path: PathBuf,
    /// File name shown in the UI, falling back to the full path if needed.
    pub name: String,
    /// Whether the binary is still present on disk.
    pub available: bool,
}

/// Shared recent-file state for every window in the application.
pub struct RecentStore {
    settings_path: Option<PathBuf>,
    paths: Vec<PathBuf>,
}

impl Global for RecentStore {}

impl RecentStore {
    /// Load recent files from the user's app configuration directory.
    ///
    /// Missing, unreadable, malformed, or unsupported settings start with an
    /// empty list so they never prevent the UI from launching.
    pub fn load() -> Self {
        let settings_path = settings_path();
        let paths = match settings_path.as_deref().map(read_paths) {
            Some(Ok(paths)) => paths,
            Some(Err(error)) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Some(Err(error)) => {
                eprintln!("could not load recent binaries: {error}");
                Vec::new()
            }
            None => Vec::new(),
        };

        Self {
            settings_path,
            paths: normalize_paths(paths),
        }
    }

    /// Return a snapshot suitable for displaying in the UI.
    #[allow(dead_code)] // The home-screen list is added in phase 4.
    pub fn entries(&self) -> Vec<RecentFile> {
        self.paths
            .iter()
            .map(|path| RecentFile {
                path: path.clone(),
                name: path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string()),
                available: path.is_file(),
            })
            .collect()
    }

    /// Put a selected binary first, remove any duplicate, and persist the list.
    ///
    /// Existing files are canonicalized so selecting the same binary through a
    /// different relative path or symlink does not add a duplicate. If the
    /// file has disappeared already, its selected path is still retained.
    pub fn record(&mut self, selected_path: impl AsRef<Path>) -> io::Result<()> {
        let selected_path = selected_path.as_ref();
        let path = fs::canonicalize(selected_path).unwrap_or_else(|_| selected_path.to_path_buf());
        self.paths.retain(|existing| !same_path(existing, &path));
        self.paths.insert(0, path);
        self.paths.truncate(MAX_RECENT_FILES);
        self.persist()
    }

    /// Remove a path from the list, including entries whose file is missing.
    #[allow(dead_code)] // The recent-list controls are added in phase 4.
    pub fn remove(&mut self, selected_path: impl AsRef<Path>) -> io::Result<()> {
        let selected_path = selected_path.as_ref();
        let path = fs::canonicalize(selected_path).unwrap_or_else(|_| selected_path.to_path_buf());
        self.paths.retain(|existing| !same_path(existing, &path));
        self.persist()
    }

    /// Remove all recent binaries.
    #[allow(dead_code)] // The recent-list controls are added in phase 4.
    pub fn clear(&mut self) -> io::Result<()> {
        self.paths.clear();
        self.persist()
    }

    fn persist(&self) -> io::Result<()> {
        let Some(settings_path) = &self.settings_path else {
            // Keep the list usable in this session even on systems without a
            // conventional per-user configuration directory.
            return Ok(());
        };
        let parent = settings_path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "settings path has no parent")
        })?;
        fs::create_dir_all(parent)?;

        let settings = SettingsFile {
            version: SETTINGS_VERSION,
            paths: &self.paths,
        };
        let bytes = serde_json::to_vec_pretty(&settings).map_err(io::Error::other)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(settings_path)
            .map(|_| ())
            .map_err(|error| error.error)
    }
}

#[derive(Serialize)]
struct SettingsFile<'a> {
    version: u32,
    paths: &'a [PathBuf],
}

#[derive(Deserialize)]
struct LoadedSettings {
    version: u32,
    paths: Vec<PathBuf>,
}

fn settings_path() -> Option<PathBuf> {
    dirs::config_dir().map(|directory| directory.join("rêve").join("settings.json"))
}

fn read_paths(path: &Path) -> io::Result<Vec<PathBuf>> {
    let bytes = fs::read(path)?;
    let settings: LoadedSettings = serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if settings.version != SETTINGS_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported settings version {}", settings.version),
        ));
    }
    Ok(settings.paths)
}

fn normalize_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut normalized: Vec<PathBuf> = Vec::with_capacity(paths.len().min(MAX_RECENT_FILES));
    for path in paths {
        if normalized
            .iter()
            .all(|existing| !same_path(existing, &path))
        {
            normalized.push(path);
            if normalized.len() == MAX_RECENT_FILES {
                break;
            }
        }
    }
    normalized
}

#[cfg(windows)]
fn same_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

#[cfg(not(windows))]
fn same_path(left: &Path, right: &Path) -> bool {
    left == right
}
