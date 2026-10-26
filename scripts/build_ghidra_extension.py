#!/usr/bin/env python3
"""Build the Ghidra extension using the project-local Ghidra and JDK tools."""

from __future__ import annotations

import os
import platform
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
GHIDRA_DIR = ROOT / ".tools" / "ghidra" / "ghidra_12.2_PUBLIC"
JAVA_HOME = ROOT / ".tools" / "java" / "temurin-25"
GRADLE_USER_HOME = ROOT / ".tools" / "gradle"
EXTENSION_DIR = ROOT / "reve-engine" / "ghidra-adapter" / "ghidra-extension"


def main() -> int:
    is_windows = platform.system() == "Windows"
    java = JAVA_HOME / "bin" / ("java.exe" if is_windows else "java")
    wrapper = GHIDRA_DIR / "support" / "gradle" / (
        "gradlew.bat" if is_windows else "gradlew"
    )

    required_paths = {
        "Ghidra installation": GHIDRA_DIR,
        "Temurin JDK 25": java,
        "Ghidra Gradle wrapper": wrapper,
        "Ghidra extension project": EXTENSION_DIR / "build.gradle",
        "Shared protobuf schema": ROOT / "reve-engine" / "proto" / "engine.proto",
    }
    missing = [f"{label}: {path}" for label, path in required_paths.items() if not path.exists()]
    if missing:
        print("Required build inputs are missing. Run scripts/setup_tools.py first:", file=sys.stderr)
        for item in missing:
            print(f"  - {item}", file=sys.stderr)
        return 1

    env = os.environ.copy()
    env["GHIDRA_INSTALL_DIR"] = str(GHIDRA_DIR)
    env["JAVA_HOME"] = str(JAVA_HOME)
    env["GRADLE_USER_HOME"] = str(GRADLE_USER_HOME)
    env["PATH"] = str(JAVA_HOME / "bin") + os.pathsep + env.get("PATH", "")

    # Invoke the shell wrapper through sh on Unix so this works even when the
    # downloaded Ghidra ZIP did not preserve executable permission bits.
    command = [str(wrapper), "--no-daemon", "buildExtension"]
    if not is_windows:
        command.insert(0, "sh")

    print(f"Building Ghidra extension with Ghidra at: {GHIDRA_DIR}")
    result = subprocess.run(command, cwd=EXTENSION_DIR, env=env, check=False)
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
