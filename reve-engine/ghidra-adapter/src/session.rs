//! Ghidra kept running on one binary's project, making the user's edits to it.
//!
//! rêve listens on a loopback port and starts Ghidra with `Serve.java`, which
//! connects back and sends a token only this process knows. Edits and the
//! changes Ghidra answers with are length-delimited protobuf messages.

use std::{
    ffi::OsStr,
    fmt, fs,
    fs::File,
    io::{self, Read, Write},
    net::{Ipv4Addr, Shutdown, TcpListener, TcpStream},
    path::Path,
    process::{Child, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use prost::Message;
use reve_model::{Changes, Edit};

use crate::{
    CancellationToken,
    config::GhidraConfig,
    error::ParserError,
    export::changes_into_model,
    headless::{self, POLL_INTERVAL, PROJECT_NAME, SCRIPTS_DIR},
    proto::reve::engine::v1 as wire,
};

const SERVE_SCRIPT: &str = "Serve.java";
/// Ghidra's output for the session, beside the project it opens.
const LOG: &str = "session.log";
/// How long Ghidra gets to exit once the session ends, before it is killed.
const EXIT_GRACE: Duration = Duration::from_secs(30);
/// How long `Serve.java` gets to send its token once connected.
const GREETING: Duration = Duration::from_secs(5);
/// Lines of Ghidra's log a failed start reports.
const LOG_TAIL: usize = 40;

/// Ghidra running on one binary's project, making edits until the session is
/// dropped or closed. Every edit is saved to the project before it returns.
#[derive(Debug)]
pub struct GhidraSession {
    stream: TcpStream,
    // Taken when the session ends.
    child: Option<Child>,
}

/// Why an edit wasn't made.
#[derive(Debug)]
pub enum EditError {
    /// Ghidra refused the edit, saying why. Nothing changed.
    Refused(String),
    /// The connection to Ghidra broke, so the session can make no more edits.
    Lost(io::Error),
}

impl fmt::Display for EditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(why) => formatter.write_str(why),
            Self::Lost(error) => write!(formatter, "lost the connection to Ghidra: {error}"),
        }
    }
}

impl std::error::Error for EditError {}

impl GhidraSession {
    /// Start Ghidra on the analyzed project in `project` and return once
    /// `Serve.java` has connected back.
    pub(crate) fn start(
        config: &GhidraConfig,
        project: &Path,
        cancellation: &CancellationToken,
    ) -> Result<Self, ParserError> {
        let listener =
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(ParserError::ProcessStart)?;
        listener
            .set_nonblocking(true)
            .map_err(ParserError::ProcessStart)?;
        let port = listener
            .local_addr()
            .map_err(ParserError::ProcessStart)?
            .port()
            .to_string();
        let token = token().map_err(ParserError::ProcessStart)?;
        let log = File::create(project.join(LOG)).map_err(ParserError::ProcessStart)?;
        let output = log.try_clone().map_err(ParserError::ProcessStart)?;

        let args = [
            project.as_os_str(),
            OsStr::new(PROJECT_NAME),
            OsStr::new("-process"),
            OsStr::new("-noanalysis"),
            OsStr::new("-scriptPath"),
            OsStr::new(SCRIPTS_DIR),
            OsStr::new("-postScript"),
            OsStr::new(SERVE_SCRIPT),
            OsStr::new(&port),
            OsStr::new(&token),
        ];
        let mut command = headless::make_command(&config.analyze_headless, &args);
        headless::configure_java_environment(&mut command, &config.java_home)?;
        command.stdin(Stdio::null()).stdout(output).stderr(log);
        let mut child = command.spawn().map_err(ParserError::ProcessStart)?;

        let started_at = Instant::now();
        let failure = loop {
            match listener.accept() {
                // Anything else that connects is dropped without a word.
                Ok((stream, _)) => {
                    if let Ok(stream) = greeted(stream, &token) {
                        return Ok(Self {
                            stream,
                            child: Some(child),
                        });
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => break ParserError::ProcessStart(error),
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    return Err(ParserError::GhidraFailed {
                        status,
                        stdout: tail(&project.join(LOG)),
                        stderr: String::new(),
                    });
                }
                Ok(None) => {}
                Err(error) => break ParserError::ProcessStart(error),
            }
            if cancellation.is_cancelled() {
                break ParserError::Cancelled;
            }
            if started_at.elapsed() >= config.timeout {
                break ParserError::TimedOut(config.timeout);
            }
            thread::sleep(POLL_INTERVAL);
        };
        headless::terminate_process_tree(&mut child);
        let _ = child.wait();
        Err(failure)
    }

    /// Ask Ghidra to make `edit`, and return what changed once Ghidra has saved it.
    pub fn edit(&mut self, edit: &Edit) -> Result<Changes, EditError> {
        self.stream
            .write_all(&edit_into_wire(edit).encode_length_delimited_to_vec())
            .map_err(EditError::Lost)?;
        let changes = read_delimited(&mut self.stream).map_err(EditError::Lost)?;
        changes_into_model(changes).map_err(EditError::Refused)
    }

    /// End the session and wait for Ghidra to exit, killing it if it takes
    /// longer than half a minute.
    pub fn close(mut self) -> io::Result<ExitStatus> {
        let _ = self.stream.shutdown(Shutdown::Both);
        let child = self
            .child
            .take()
            .expect("only close and drop take the child");
        wait(child)
    }
}

impl Drop for GhidraSession {
    /// Closing the connection ends `Serve.java`, and Ghidra exits. The wait for
    /// that happens on its own thread, so dropping a session never blocks.
    fn drop(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
        if let Some(child) = self.child.take() {
            thread::spawn(move || wait(child));
        }
    }
}

/// `stream` once it has sent `token` and a newline, ready for edits.
fn greeted(stream: TcpStream, token: &str) -> io::Result<TcpStream> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(GREETING))?;
    let mut greeting = vec![0; token.len() + 1];
    (&stream).read_exact(&mut greeting)?;
    if greeting != format!("{token}\n").as_bytes() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "wrong token",
        ));
    }
    stream.set_read_timeout(None)?;
    stream.set_nodelay(true)?;
    Ok(stream)
}

/// 128 random bits in hex, which `Serve.java` sends back to prove it is the
/// Ghidra this process started.
fn token() -> io::Result<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|error| io::Error::other(error.to_string()))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// The last lines of the log at `path`.
fn tail(path: &Path) -> String {
    let log = fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<_> = log.lines().collect();
    lines[lines.len().saturating_sub(LOG_TAIL)..].join("\n")
}

/// Wait for `child` to exit, killing its process tree after [`EXIT_GRACE`].
fn wait(mut child: Child) -> io::Result<ExitStatus> {
    let started_at = Instant::now();
    while started_at.elapsed() < EXIT_GRACE {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        thread::sleep(POLL_INTERVAL);
    }
    headless::terminate_process_tree(&mut child);
    child.wait()
}

/// Read one message written with a varint length before it, as protobuf's
/// `writeDelimitedTo` does.
fn read_delimited<M: Message + Default>(reader: &mut impl Read) -> io::Result<M> {
    let mut length = 0;
    for shift in (0..64).step_by(7) {
        let mut byte = [0];
        reader.read_exact(&mut byte)?;
        length |= u64::from(byte[0] & 0x7f) << shift;
        if byte[0] & 0x80 == 0 {
            let mut message = vec![0; length as usize];
            reader.read_exact(&mut message)?;
            return M::decode(message.as_slice())
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error));
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "message length runs past 64 bits",
    ))
}

fn edit_into_wire(edit: &Edit) -> wire::Edit {
    use wire::edit::Kind;
    let kind = match edit {
        Edit::Rename { entry, name } => Kind::Rename(wire::Rename {
            entry: entry.0,
            name: name.clone(),
        }),
        Edit::Comment { address, text } => Kind::Comment(wire::Comment {
            address: address.0,
            text: text.clone(),
        }),
        Edit::Bookmark { address, on } => Kind::Bookmark(wire::Bookmark {
            address: address.0,
            on: *on,
        }),
    };
    wire::Edit { kind: Some(kind) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        GhidraAdapter,
        test_support::{TempDirectory, fake_headless, fake_jdk},
    };
    use reve_model::{Address, TokenKind};
    use std::path::PathBuf;

    fn setup(mode: &str) -> (TempDirectory, GhidraConfig, PathBuf) {
        let temp = TempDirectory::new(&format!("session-{mode}")).unwrap();
        let launcher = fake_headless(temp.path(), "fake headless", mode).unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        let config = GhidraConfig::new(launcher, temp.path())
            .with_java_home(fake_jdk(temp.path()).unwrap())
            .with_timeout(Duration::from_secs(20));
        (temp, config, project)
    }

    /// Plays `Serve.java` for the fake launcher in `directory`, after a
    /// stranger without the token tries first: connects with the launch's port
    /// and token, answers each edit with the next of `answers`, then stops the
    /// launcher once rêve hangs up. Returns the edits it read.
    fn serve(
        directory: PathBuf,
        answers: Vec<wire::Changes>,
    ) -> thread::JoinHandle<Vec<wire::Edit>> {
        thread::spawn(move || {
            let args = loop {
                let args = fs::read_to_string(directory.join("args")).unwrap_or_default();
                let args: Vec<String> = args.lines().map(str::to_owned).collect();
                if args.last().is_some_and(|token| token.len() == 32) {
                    break args;
                }
                thread::sleep(Duration::from_millis(20));
            };
            let (port, token) = (
                args[args.len() - 2].parse::<u16>().unwrap(),
                &args[args.len() - 1],
            );

            let mut stranger = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
            stranger
                .write_all(format!("{}\n", "0".repeat(32)).as_bytes())
                .unwrap();
            let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
            stream.write_all(format!("{token}\n").as_bytes()).unwrap();
            let mut edits = Vec::new();
            for answer in answers {
                edits.push(read_delimited(&mut stream).unwrap());
                stream
                    .write_all(&answer.encode_length_delimited_to_vec())
                    .unwrap();
            }
            assert!(
                read_delimited::<wire::Edit>(&mut stream).is_err(),
                "rêve hangs up when the session closes"
            );
            fs::write(directory.join("stop"), "").unwrap();
            edits
        })
    }

    #[test]
    fn edits_go_to_ghidra_and_its_changes_come_back() {
        let (temp, config, project) = setup("serve");
        let rename = Edit::Rename {
            entry: Address(0x1000),
            name: "parse".into(),
        };
        let ghidra = serve(
            temp.path().to_path_buf(),
            vec![
                wire::Changes {
                    functions: vec![wire::Function {
                        name: "parse".into(),
                        entry: 0x1000,
                        ..wire::Function::default()
                    }],
                    ..wire::Changes::default()
                },
                wire::Changes {
                    error: Some("No instruction at 00001002.".into()),
                    ..wire::Changes::default()
                },
            ],
        );

        let mut session =
            GhidraSession::start(&config, &project, &CancellationToken::new()).unwrap();
        let renamed = session.edit(&rename).unwrap();
        let refused = session.edit(&Edit::Comment {
            address: Address(0x1002),
            text: "checks the magic".into(),
        });
        let status = session.close().unwrap();

        assert_eq!(renamed.functions[0].entry, Address(0x1000));
        assert_eq!(renamed.functions[0].name, "parse");
        assert!(
            matches!(&refused, Err(EditError::Refused(why)) if why == "No instruction at 00001002."),
            "{refused:?}"
        );
        assert!(status.success(), "Ghidra exits once the session closes");
        let edits = ghidra.join().unwrap();
        assert_eq!(edits[0], edit_into_wire(&rename));
        let args = fs::read_to_string(temp.path().join("args")).unwrap();
        let args: Vec<_> = args.lines().collect();
        assert_eq!(
            args[2..8],
            [
                "-process",
                "-noanalysis",
                "-scriptPath",
                SCRIPTS_DIR,
                "-postScript",
                "Serve.java"
            ]
        );
    }

    #[test]
    fn start_reports_a_ghidra_that_exits_before_connecting() {
        let (_temp, config, project) = setup("fail");

        let error = GhidraSession::start(&config, &project, &CancellationToken::new()).unwrap_err();

        match error {
            ParserError::GhidraFailed { status, stdout, .. } => {
                assert_eq!(status.code(), Some(23));
                assert!(stdout.contains("fake stdout"), "the log's tail: {stdout}");
            }
            other => panic!("expected GhidraFailed, got {other:?}"),
        }
    }

    #[test]
    fn a_binary_never_analyzed_has_no_session() {
        let (temp, config, _project) = setup("serve");
        let input = temp.path().join("never analyzed.elf");
        fs::write(&input, b"fake ELF input").unwrap();

        let error = GhidraAdapter::new(config)
            .session(&input, &CancellationToken::new())
            .unwrap_err();

        assert!(matches!(error, ParserError::NotAnalyzed(_)), "{error:?}");
    }

    /// Edits a real binary with real Ghidra, then checks a fresh export from
    /// the project holds them.
    #[test]
    #[ignore = "runs the Ghidra from scripts/bootstrap.py for about a minute"]
    fn real_ghidra_makes_and_saves_edits() {
        let temp = TempDirectory::new("session-real").unwrap();
        let mut config = GhidraConfig::from_environment();
        config.projects_dir = temp.path().join("projects");
        config.cache_dir = None;
        let input = Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../.tools/ghidra/ghidra_12.2_PUBLIC/GPL/DemanglerGnu/os/linux_x86_64/demangler_gnu_v2_41",
        );
        let adapter = GhidraAdapter::new(config);
        let program = adapter.analyze(&input).unwrap();

        // A function another function's C calls by name, with more code than a thunk's jump.
        let calls = |caller: &reve_model::Function, name: &str| {
            caller
                .decompilation
                .iter()
                .flat_map(|d| &d.lines)
                .flat_map(|l| &l.tokens)
                .any(|token| token.kind == TokenKind::Function && token.text == name)
        };
        let (callee, caller) = program
            .functions
            .iter()
            .filter(|function| function.instructions.len() > 3)
            .find_map(|callee| {
                let caller = program
                    .functions
                    .iter()
                    .find(|caller| caller.entry != callee.entry && calls(caller, &callee.name))?;
                Some((callee, caller))
            })
            .expect("some function has a caller");
        let at = callee.instructions[0].address;

        let mut session = adapter.session(&input, &CancellationToken::new()).unwrap();
        let renamed = session
            .edit(&Edit::Rename {
                entry: callee.entry,
                name: "reve_renamed".into(),
            })
            .unwrap();
        let shown = renamed
            .functions
            .iter()
            .find(|change| change.entry == caller.entry)
            .expect("the caller is decompiled again");
        let caller_now = reve_model::Function {
            decompilation: shown.decompilation.clone(),
            ..caller.clone()
        };
        assert!(
            calls(&caller_now, "reve_renamed"),
            "the caller's C shows the new name"
        );

        let refused = session.edit(&Edit::Rename {
            entry: callee.entry,
            name: "has space".into(),
        });
        assert!(matches!(refused, Err(EditError::Refused(_))), "{refused:?}");

        let restored = session
            .edit(&Edit::Rename {
                entry: callee.entry,
                name: String::new(),
            })
            .unwrap();
        assert_eq!(
            restored.functions[0].name,
            format!("FUN_{:08x}", callee.entry.0),
            "an empty name is Ghidra's default"
        );
        let commented = session
            .edit(&Edit::Comment {
                address: at,
                text: "checks the magic".into(),
            })
            .unwrap();
        assert_eq!(commented.comments, [(at, Some("checks the magic".into()))]);
        let marked = session
            .edit(&Edit::Bookmark {
                address: at,
                on: true,
            })
            .unwrap();
        assert_eq!(marked.bookmarks, [(at, true)]);
        session
            .edit(&Edit::Rename {
                entry: callee.entry,
                name: "reve_saved".into(),
            })
            .unwrap();
        assert!(session.close().unwrap().success());

        let saved = adapter.analyze(&input).unwrap();
        let function = saved
            .functions
            .iter()
            .find(|function| function.entry == callee.entry)
            .unwrap();
        assert_eq!(function.name, "reve_saved");
        assert_eq!(
            function.instructions[0].comment.as_deref(),
            Some("checks the magic")
        );
        assert!(saved.bookmarks.contains(&at));
    }
}
