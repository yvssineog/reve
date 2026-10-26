# Ghidra adapter

This directory contains the Rust protobuf adapter crate and the Ghidra extension
that will export analysis results for it. Both sides generate their message
types from the shared schema at `reve-engine/proto/engine.proto`.

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

Generated Java files, downloaded dependencies, Gradle caches, and the packaged
extension are build artifacts; do not edit or commit them. The next adapter
step will add `Export.java` under `ghidra-extension/ghidra_scripts/` and use the
generated `reve.engine.v1.Engine` message classes.
