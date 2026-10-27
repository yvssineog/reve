# rêve

Open-source reverse engineering application.

- `reve-engine/model/`: shared engine models.
- `reve-ui/`: GPUI application.

Initial target: Linux ELF x86-64. Windows PE support may follow.

## Development tools

The parser uses **Ghidra 12.2** and **Eclipse Temurin JDK 25 (64-bit)**. 

From the repository root, run it with Python 3.9 or newer (macOS's built-in `python3` works):

```sh
python3 scripts/bootstrap.py
```

It downloads the Ghidra release and the latest Temurin 25 update for the current operating system and architecture, verifies their SHA-256 checksums, and extracts them under `.tools/`. It then builds Ghidra's native decompiler where the release ships none (Apple Silicon) and builds and installs the rêve Ghidra extension. Re-run it after changing `Export.java` or `engine.proto`.

The headless launcher is installed at:

- Windows: `.tools/ghidra/ghidra_12.2_PUBLIC/support/analyzeHeadless.bat`
- Linux/macOS: `.tools/ghidra/ghidra_12.2_PUBLIC/support/analyzeHeadless`

The bundled JDK is under `.tools/java/temurin-25/`. The parser can use these paths directly; no system-wide Java installation is required.

## Run the UI

Run `cargo run -p reve-ui` to start the GPUI application. Opening a binary analyzes it with the bundled Ghidra, so run `scripts/bootstrap.py` first.

## TODO

`reve-model` doesn't carry these yet, so the UI leaves them out:

- Function plate comments and signatures
- Function namespaces
- Imports, by library
- Calls to imported functions
- End-of-line comments and the strings instructions load
- The program's start address
- Decompiled C (the Decompile pane shows Not ready.)
