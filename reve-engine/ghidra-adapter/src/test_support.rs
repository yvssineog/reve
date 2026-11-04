//! Helpers shared by adapter unit tests.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub(crate) struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    pub(crate) fn new(label: &str) -> io::Result<Self> {
        let root = std::env::temp_dir();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        for _ in 0..100 {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = root.join(format!(
                "reve adapter tests {label} {} {timestamp} {id}",
                std::process::id()
            ));

            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }

        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique test directory",
        ))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Create the placeholder Java executable needed by configuration tests.
pub(crate) fn fake_jdk(directory: &Path) -> io::Result<PathBuf> {
    let java_home = directory.join("fake JDK");
    let bin = java_home.join("bin");
    fs::create_dir_all(&bin)?;
    let executable = bin.join(if cfg!(windows) { "java.exe" } else { "java" });
    fs::write(executable, "test placeholder")?;
    Ok(java_home)
}

/// Create a fake `analyzeHeadless` launcher for an end-to-end adapter test.
///
/// The `mode` is one of `write`, `missing`, `invalid`, `fail`, `timeout`, or
/// `serve`. `serve` writes its arguments to `args` beside the script, one per
/// line, then runs until a `stop` file appears there.
/// Every launch appends its third argument, `-import` or `-process`, as a line
/// to `launches` beside the script. An import creates the project's files in
/// the project location, its first argument, as Ghidra does.
pub(crate) fn fake_headless(directory: &Path, name: &str, mode: &str) -> io::Result<PathBuf> {
    let (extension, script) = if cfg!(windows) {
        let body = match mode {
            "write" => {
                "powershell.exe -NoProfile -NonInteractive -Command \"[IO.File]::WriteAllBytes('%last%', [Convert]::FromBase64String('CgRkZW1v'))\"\r\necho fake stdout\r\necho fake stderr 1>&2\r\nexit /b 0\r\n"
            }
            "missing" => "echo fake stdout\r\nexit /b 0\r\n",
            "invalid" => {
                "powershell.exe -NoProfile -NonInteractive -Command \"[IO.File]::WriteAllBytes('%last%', [Convert]::FromBase64String('CgRk'))\"\r\nexit /b 0\r\n"
            }
            "fail" => "echo fake stdout\r\necho fake stderr 1>&2\r\nexit /b 23\r\n",
            "timeout" => {
                "powershell.exe -NoProfile -NonInteractive -Command \"Start-Sleep -Seconds 10\"\r\nexit /b 0\r\n"
            }
            "serve" => {
                "(for %%a in (%*) do @echo %%~a)>\"%~dp0args\"\r\npowershell.exe -NoProfile -NonInteractive -Command \"while (!(Test-Path '%~dp0stop')) { Start-Sleep -Milliseconds 100 }\"\r\nexit /b 0\r\n"
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "unknown fake mode",
                ));
            }
        };
        (
            "bat",
            format!(
                "@echo off\r\necho %~3>>\"%~dp0launches\"\r\nif \"%~3\"==\"-import\" (mkdir \"%~1\\ReveAnalysis.rep\" & type nul > \"%~1\\ReveAnalysis.gpr\")\r\n:args\r\nif not \"%~1\"==\"\" (set \"last=%~1\" & shift & goto args)\r\n{body}"
            ),
        )
    } else {
        let body = match mode {
            "write" => {
                "last=\"\"\nfor arg in \"$@\"; do last=\"$arg\"; done\nprintf '\\012\\004\\144\\145\\155\\157' > \"$last\"\necho fake stdout\necho fake stderr >&2\n"
            }
            "missing" => "echo fake stdout\necho fake stderr >&2\n",
            "invalid" => {
                "last=\"\"\nfor arg in \"$@\"; do last=\"$arg\"; done\nprintf '\\012\\004\\144' > \"$last\"\n"
            }
            "fail" => "echo fake stdout\necho fake stderr >&2\nexit 23\n",
            // Like Ghidra's launch.sh, start the long-running child without
            // `exec`, and record its pid so a test can check it was killed.
            "timeout" => "sleep 30 &\necho $! > \"$(dirname \"$0\")/launcher-child.pid\"\nwait\n",
            "serve" => {
                "printf '%s\\n' \"$@\" > \"$(dirname \"$0\")/args\"\nwhile [ ! -e \"$(dirname \"$0\")/stop\" ]; do sleep 0.1; done\n"
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "unknown fake mode",
                ));
            }
        };
        (
            "sh",
            format!(
                "#!/bin/sh\n[ -n \"$REVE_FAKE_WARM_UP\" ] && exit 0\necho \"$3\" >> \"$(dirname \"$0\")/launches\"\n[ \"$3\" = -import ] && mkdir -p \"$1/ReveAnalysis.rep\" && : > \"$1/ReveAnalysis.gpr\"\n{body}"
            ),
        )
    };

    let path = directory.join(format!("{name}.{extension}"));
    fs::write(&path, script)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions)?;

        // macOS checks a new executable on its first exec, which can outlast
        // a test's timeout. Pay that cost here, before any test times it.
        std::process::Command::new(&path)
            .env("REVE_FAKE_WARM_UP", "1")
            .status()?;
    }

    Ok(path)
}
