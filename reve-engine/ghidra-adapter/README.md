# Ghidra adapter

This directory contains the Rust protobuf adapter crate and the Ghidra extension
that exports analysis results. Both sides generate their message types from the
shared schema at `reve-engine/proto/engine.proto`.

## Build the Ghidra extension

Run from the repository root:

```sh
python scripts/bootstrap.py
```

The script installs the pinned Ghidra and Temurin JDK under `.tools/` and runs
the extension's `installExtension` Gradle task with them. Gradle generates Java
sources from `reve-engine/proto/engine.proto`, compiles them into the extension
JAR, and packages `protobuf-java` beside that JAR so Ghidra's extension
classloader can load it.

The installable archive is written to:

```text
reve-engine/ghidra-adapter/ghidra-extension/dist/
```

`installExtension` also unpacks the extension into the project-local Ghidra
installation at `.tools/ghidra/ghidra_12.2_PUBLIC/Ghidra/Extensions/`. This
makes the generated Java classes, protobuf runtime, and `Export.java` available
to headless Ghidra without opening the GUI. Re-run `scripts/bootstrap.py` after
changing `build.gradle` or `engine.proto`. The Rust adapter passes this
directory's `ghidra_scripts/` as `-scriptPath`, so Ghidra runs the repo's
`Export.java` ahead of the installed copy and script edits need no reinstall.
The script also configures Ghidra to
use the project-local JDK at `.tools/java/temurin-25`; the Rust parser also sets
that JDK for each headless process, so no global `JAVA_HOME` change is needed.

## Export an analyzed program

Create a project directory whose path does not begin with a dot (Ghidra rejects
hidden path components), then run the headless analyzer on an ELF. Ghidra
imports and analyzes the file, then runs `Export.java` and writes one binary
Protocol Buffers `Program` message to the requested output path:

```sh
mkdir -p ghidra-projects
GHIDRA_DIR=.tools/ghidra/ghidra_12.2_PUBLIC
"$GHIDRA_DIR/support/analyzeHeadless" "$PWD/ghidra-projects" ReveExport \
  -import /path/to/input.elf \
  -postScript Export.java /absolute/path/to/result.pb
```

In PowerShell, the equivalent command is:

```powershell
New-Item -ItemType Directory -Force ghidra-projects | Out-Null
$ghidraDir = ".tools\ghidra\ghidra_12.2_PUBLIC"
& "$ghidraDir\support\analyzeHeadless.bat" "$PWD\ghidra-projects" ReveExport `
  -import "C:\path\to\input.elf" `
  -postScript Export.java "C:\path\to\result.pb"
```

The output contains program metadata, Ghidra-recognized functions, and each
function's instructions with addresses, rendered operands, and original
instruction bytes. `ProtoClasspathCheck.java` checks that the generated types
and protobuf runtime can be loaded by Ghidra.

## Run the Rust parser demo

The `reve-parse` binary runs the complete local flow: it invokes Ghidra,
decodes the protobuf export, and prints a short summary of the shared Rust
`Program`. Run `python scripts/bootstrap.py` once first; it
installs the extension into the local Ghidra distribution for headless use.

On Windows PowerShell:

```powershell
$env:GHIDRA_HEADLESS = (Resolve-Path ".tools\ghidra\ghidra_12.2_PUBLIC\support\analyzeHeadless.bat").Path
$env:REVE_GHIDRA_PROJECTS = (Join-Path $PWD "ghidra-projects")
cargo run -p reve-ghidra-adapter --bin reve-parse -- "C:\path\to\input.elf"
```

On Linux or macOS:

```sh
export GHIDRA_HEADLESS="$PWD/.tools/ghidra/ghidra_12.2_PUBLIC/support/analyzeHeadless"
export REVE_GHIDRA_PROJECTS="$PWD/ghidra-projects"
cargo run -p reve-ghidra-adapter --bin reve-parse -- /path/to/input.elf
```

`GHIDRA_HEADLESS` is optional and defaults to the project-local launcher under
`.tools/ghidra/`. `REVE_GHIDRA_PROJECTS` is optional and defaults to
`rêve/projects` in the user's data directory
(`~/Library/Application Support/rêve/projects/` on macOS). On Linux it
defaults to `~/rêve/projects/`, since Ghidra refuses project paths through
hidden directories like `~/.local/share`.

That directory holds one Ghidra project per analyzed binary, named after the
SHA-256 of its bytes. A binary's first run imports and analyzes it in a unique
job directory, then moves the project into place. Later runs export that
project without analyzing it again. The job directory, with the protobuf
export, is removed when parsing finishes.

Each decoded export is also saved in the user's cache directory
(`~/Library/Caches/rêve/analyses/` on macOS). A binary analyzed before loads
from there without running Ghidra. The key hashes the binary's name and bytes,
`Export.java`, `DecompilerPool.java`, `engine.proto`, the installed extension jar and the Ghidra
version, so changing the exporter or schema, reinstalling the extension, or
upgrading Ghidra exports the binary's project again. Delete that directory to
force a fresh export, and the binary's project to force a fresh analysis.

## Edit a program

`GhidraAdapter::session` starts Ghidra on a binary's kept project with
`Serve.java` and returns a `GhidraSession` once Ghidra is ready, a few seconds
later. Each `edit` renames a function, sets an end-of-line comment, or adds or
removes a Note bookmark. Ghidra makes the edit in its own transaction, saves the
project, decompiles again every function whose C shows a new name, and answers
with what changed, or with its reason for refusing the edit. The adapter writes
those changes into the binary's saved export too, so reopening it loads the
edited program without Ghidra. Dropping or closing
the session closes its connection, and Ghidra exits. Ghidra's output for the
session goes to `session.log` beside the project.

rêve listens on a loopback port that `Serve.java` connects back to. Before
anything else, the script sends a random token only the adapter knows, so no
other local program can drive Ghidra.

`cargo test -p reve-ghidra-adapter -- --ignored` runs a session on a real
binary with the Ghidra that `scripts/bootstrap.py` installs. It takes about a
minute.

## Test the exporter

`developer_scripts/` isn't packaged into the extension. Run these commands from
`ghidra-extension/`, with a JDK on your `PATH`.

`DecompilerPoolTest` needs no Ghidra, and CI runs it:

```sh
javac -d /tmp/decompiler-pool ghidra_scripts/DecompilerPool.java developer_scripts/DecompilerPoolTest.java
java -cp /tmp/decompiler-pool DecompilerPoolTest
```

`ExportFaultTest` makes one decompilation throw, then checks that the export
keeps every other function and all instructions. It needs real Ghidra, so CI
doesn't run it. Ghidra compiles each script directory on its own, so the test
needs a directory that also holds the scripts it extends:

```sh
scripts=$(mktemp -d)
cp ghidra_scripts/Export.java ghidra_scripts/DecompilerPool.java developer_scripts/ExportFaultTest.java "$scripts"
"$GHIDRA_INSTALL_DIR/support/analyzeHeadless" "$(mktemp -d)" Test -import <binary> \
  -scriptPath "$scripts" -postScript ExportFaultTest.java "$(mktemp -d)/out.pb"
```

It passes when the log prints `EXPORT_FAULT_OK`.

Generated Java files, downloaded dependencies, Gradle caches, and the packaged
extension are build artifacts; do not edit or commit them. The Rust adapter
will decode the `.pb` file and convert it into the shared `reve_model::Program`
types.
