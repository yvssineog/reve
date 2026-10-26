# rêve

Open-source reverse engineering application.

- `reve-engine/model/`: shared engine models.
- `reve-ui/`: GPUI application.

Initial target: Linux ELF x86-64. Windows PE support may follow.

## Development tools

The parser uses **Ghidra 12.2** and **Eclipse Temurin JDK 25 (64-bit)**. 

From the repository root, run:

```powershell
python scripts/setup_tools.py
```

It downloads the Ghidra release and the latest Temurin 25 update for the current operating system and architecture, verifies their SHA-256 checksums, and extracts them under `.tools/`.

The headless launcher is installed at:

- Windows: `.tools/ghidra/ghidra_12.2_PUBLIC/support/analyzeHeadless.bat`
- Linux/macOS: `.tools/ghidra/ghidra_12.2_PUBLIC/support/analyzeHeadless`

The bundled JDK is under `.tools/java/temurin-25/`. The parser can use these paths directly; no system-wide Java installation is required.

## Run the UI

Run `cargo run -p reve-ui` to start the GPUI application.

## TODO

`reve-model` doesn't carry these yet, so the UI leaves them out:

- Function plate comments and signatures
- Function namespaces
- Imports, by library
- Calls to imported functions
- End-of-line comments and the strings instructions load
- The program's start address
- Decompiled C, still mocked in `reve-ui/src/mock.rs`
