# Ghidra adapter

This directory contains the Rust protobuf adapter crate and the Ghidra extension
that exports analysis results. Both sides generate their message types from the
shared schema at `reve-engine/proto/engine.proto`.

## Build the Ghidra extension

Run from the repository root after installing the pinned local tools:

```sh
python scripts/setup_tools.py
python scripts/build_ghidra_extension.py
```

The build script selects the platform-specific Ghidra Gradle wrapper and uses
the Temurin JDK installed under `.tools/`. Gradle generates Java sources from
`reve-engine/proto/engine.proto`, compiles them into the extension JAR, and
packages `protobuf-java` beside that JAR so Ghidra's extension classloader can
load it.

The installable archive is written to:

```text
reve-engine/ghidra-adapter/ghidra-extension/dist/
```

The build script also extracts the extension into the project-local Ghidra
installation at `.tools/ghidra/ghidra_12.2_PUBLIC/Ghidra/Extensions/`. This
makes the generated Java classes, protobuf runtime, and `Export.java` available
to headless Ghidra without opening the GUI. Re-run the build script after
changing the extension or `engine.proto`. `setup_tools.py` configures Ghidra to
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
`Program`. Build the Ghidra extension once with `python
scripts/build_ghidra_extension.py`; the script installs it into the local Ghidra
distribution for headless use.

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
`ghidra-projects` in the current directory. Each run writes its temporary
Ghidra project and protobuf export in a unique job directory, then removes
that directory when parsing finishes.

Generated Java files, downloaded dependencies, Gradle caches, and the packaged
extension are build artifacts; do not edit or commit them. The Rust adapter
will decode the `.pb` file and convert it into the shared `reve_model::Program`
types.
