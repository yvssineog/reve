//! Process launching and timeout handling for Ghidra Headless.

use std::{
    env,
    ffi::OsStr,
    fs,
    io::{self, Read},
    path::Path,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{config::GhidraConfig, error::ParserError};

const PROJECT_NAME: &str = "ReveAnalysis";
const EXPORT_SCRIPT: &str = "Export.java";
/// Searched before the installed extension, so `Export.java` edits apply without reinstalling it.
const SCRIPTS_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/ghidra-extension/ghidra_scripts"
);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Run Ghidra Headless to import and analyze `input`, then invoke `Export.java`.
///
/// `project_location` is the unique per-analysis directory. `export_path` is
/// passed to the script as its only argument. This function returns only after
/// the Ghidra process exits or the configured timeout expires.
pub(crate) fn run_headless(
    config: &GhidraConfig,
    project_location: &Path,
    input: &Path,
    export_path: &Path,
) -> Result<Output, ParserError> {
    config.validate()?;
    let input =
        fs::canonicalize(input).map_err(|_| ParserError::InputNotFound(input.to_path_buf()))?;

    let args = [
        project_location.as_os_str(),
        OsStr::new(PROJECT_NAME),
        OsStr::new("-import"),
        input.as_os_str(),
        OsStr::new("-scriptPath"),
        OsStr::new(SCRIPTS_DIR),
        OsStr::new("-postScript"),
        OsStr::new(EXPORT_SCRIPT),
        export_path.as_os_str(),
    ];

    let mut command = make_command(&config.analyze_headless, &args);
    configure_java_environment(&mut command, &config.java_home)?;
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command.spawn().map_err(ParserError::ProcessStart)?;
    let stdout = child
        .stdout
        .take()
        .expect("stdout was configured as a pipe");
    let stderr = child
        .stderr
        .take()
        .expect("stderr was configured as a pipe");

    // Drain both streams concurrently so Ghidra cannot block on a full pipe.
    let stdout_reader = thread::spawn(move || read_all(stdout));
    let stderr_reader = thread::spawn(move || read_all(stderr));

    let started_at = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                terminate_process_tree(&mut child);
                let _ = child.wait();
                return Err(ParserError::ProcessStart(error));
            }
        }

        if started_at.elapsed() >= config.timeout {
            terminate_process_tree(&mut child);
            let _ = child.wait();
            // On timeout the pipe readers may still be draining a subprocess
            // that outlived the launcher. Detach them to keep the timeout bound.
            return Err(ParserError::TimedOut(config.timeout));
        }

        thread::sleep(POLL_INTERVAL.min(config.timeout.saturating_sub(started_at.elapsed())));
    };

    let stdout = join_reader(stdout_reader)?;
    let stderr = join_reader(stderr_reader)?;
    let output = Output {
        status,
        stdout,
        stderr,
    };

    if !output.status.success() {
        return Err(ParserError::GhidraFailed {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    Ok(output)
}

fn configure_java_environment(command: &mut Command, java_home: &Path) -> Result<(), ParserError> {
    // `canonicalize` returns Windows verbatim paths (`\\?\C:\...`). Ghidra's
    // batch launchers do not handle those reliably, so pass normal DOS paths
    // across the process boundary.
    let java_home = std::path::PathBuf::from(child_path_arg(java_home.as_os_str()));
    let java_bin = java_home.join("bin");
    let existing_path = env::var_os("PATH").unwrap_or_default();
    let path = env::join_paths(std::iter::once(java_bin).chain(env::split_paths(&existing_path)))
        .map_err(|error| {
        ParserError::ProcessStart(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("could not prepare the project JDK PATH: {error}"),
        ))
    })?;

    command.env("JAVA_HOME", java_home).env("PATH", path);
    Ok(())
}

fn read_all(mut reader: impl Read) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    reader.read_to_end(&mut output)?;
    Ok(output)
}

fn join_reader(reader: thread::JoinHandle<io::Result<Vec<u8>>>) -> Result<Vec<u8>, ParserError> {
    reader
        .join()
        .map_err(|_| {
            ParserError::ProcessOutput(io::Error::other("Ghidra output reader thread panicked"))
        })?
        .map_err(ParserError::ProcessOutput)
}

#[cfg(not(windows))]
fn make_command(launcher: &Path, args: &[&OsStr]) -> Command {
    let mut command = Command::new(launcher);
    command.args(args);
    command
}

#[cfg(windows)]
fn make_command(launcher: &Path, args: &[&OsStr]) -> Command {
    use std::os::windows::process::CommandExt;

    if !matches!(
        launcher.extension().and_then(OsStr::to_str),
        Some(extension) if extension.eq_ignore_ascii_case("bat") || extension.eq_ignore_ascii_case("cmd")
    ) {
        let mut command = Command::new(launcher);
        command.args(args);
        return command;
    }

    // Windows batch launchers must be run through cmd.exe. /S /C strips only
    // the outer quotes; the embedded quotes preserve spaces in each argument.
    let launcher = child_path_arg(launcher.as_os_str());
    let mut command_line = format!("\"\"{}\"", launcher.to_string_lossy());
    for argument in args {
        command_line.push(' ');
        command_line.push('"');
        command_line.push_str(&child_path_arg(argument).to_string_lossy());
        command_line.push('"');
    }
    command_line.push('"');

    // Keep the command string in a single OS argument for cmd.exe /C.
    let mut command = Command::new("cmd.exe");
    command.args(["/D", "/S", "/C"]);
    command.raw_arg(format!(" {command_line}"));
    command
}

#[cfg(windows)]
fn child_path_arg(value: &OsStr) -> std::ffi::OsString {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    const EXTENDED_PREFIX: &[u16] = &[92, 92, 63, 92];
    const EXTENDED_UNC_PREFIX: &[u16] = &[92, 92, 63, 92, 85, 78, 67, 92];
    let value = value.encode_wide().collect::<Vec<_>>();

    if value.starts_with(EXTENDED_UNC_PREFIX) {
        let mut normal_path = vec![92, 92];
        normal_path.extend_from_slice(&value[EXTENDED_UNC_PREFIX.len()..]);
        std::ffi::OsString::from_wide(&normal_path)
    } else if value.starts_with(EXTENDED_PREFIX) && value.get(5) == Some(&(b':' as u16)) {
        std::ffi::OsString::from_wide(&value[EXTENDED_PREFIX.len()..])
    } else {
        std::ffi::OsString::from_wide(&value)
    }
}

#[cfg(not(windows))]
fn child_path_arg(value: &OsStr) -> std::ffi::OsString {
    value.to_os_string()
}

#[cfg(windows)]
fn terminate_process_tree(child: &mut std::process::Child) {
    // analyzeHeadless.bat starts Java as a child of cmd.exe. Kill the process
    // tree so a timeout does not leave the Java analysis running in background.
    let pid = child.id().to_string();
    let _ = Command::new("taskkill.exe")
        .args(["/PID", pid.as_str(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
}

#[cfg(not(windows))]
fn terminate_process_tree(child: &mut std::process::Child) {
    let _ = child.kill();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_dir_holds_the_export_script() {
        assert!(Path::new(SCRIPTS_DIR).join(EXPORT_SCRIPT).is_file());
    }

    #[test]
    fn command_uses_the_configured_jdk_and_puts_it_first_on_path() {
        let java_home = Path::new("/reve/.tools/java/temurin-25");
        let mut command = Command::new("placeholder");

        configure_java_environment(&mut command, java_home).unwrap();

        let environment = command
            .get_envs()
            .filter_map(|(key, value)| value.map(|value| (key, value)))
            .collect::<Vec<_>>();
        assert!(environment.iter().any(|(key, value)| {
            *key == OsStr::new("JAVA_HOME") && *value == java_home.as_os_str()
        }));
        let path = environment
            .iter()
            .find(|(key, _)| *key == OsStr::new("PATH"))
            .map(|(_, value)| env::split_paths(value).collect::<Vec<_>>())
            .unwrap();
        assert_eq!(path.first(), Some(&java_home.join("bin")));
    }

    #[cfg(windows)]
    #[test]
    fn windows_child_paths_drop_extended_length_prefixes() {
        assert_eq!(
            child_path_arg(OsStr::new(r"\\?\D:\reve\.tools\ghidra")),
            OsStr::new(r"D:\reve\.tools\ghidra")
        );
        assert_eq!(
            child_path_arg(OsStr::new(r"\\?\UNC\server\share\reve")),
            OsStr::new(r"\\server\share\reve")
        );
        assert_eq!(
            child_path_arg(OsStr::new(r"D:\reve\.tools\ghidra")),
            OsStr::new(r"D:\reve\.tools\ghidra")
        );
    }
}
