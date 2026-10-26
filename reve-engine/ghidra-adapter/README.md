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

The resulting installable archive is written to:

```text
reve-engine/ghidra-adapter/ghidra-extension/dist/
```

Install that ZIP from Ghidra's **File → Install Extensions** dialog, then
restart Ghidra. This makes the generated Java classes, protobuf runtime, and
scripts available to Ghidra. On the first launch, select the JDK installed at
`.tools/java/temurin-25` if Ghidra asks which JDK to use. For headless-only use,
set `JAVA_HOME_OVERRIDE` in `support/launch.properties` to that JDK path.

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

Generated Java files, downloaded dependencies, Gradle caches, and the packaged
extension are build artifacts; do not edit or commit them. The Rust adapter
will decode the `.pb` file and convert it into the shared `reve_model::Program`
types.
