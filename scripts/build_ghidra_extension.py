#!/usr/bin/env python3
"""Build the Ghidra extension using the project-local Ghidra and JDK tools."""

from __future__ import annotations

import os
import platform
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
GHIDRA_DIR = ROOT / ".tools" / "ghidra" / "ghidra_12.2_PUBLIC"
JAVA_HOME = ROOT / ".tools" / "java" / "temurin-25"
GRADLE_USER_HOME = ROOT / ".tools" / "gradle"
EXTENSION_DIR = ROOT / "reve-engine" / "ghidra-adapter" / "ghidra-extension"
EXTENSION_INSTALL_DIR = GHIDRA_DIR / "Ghidra" / "Extensions"


def install_extension_archive(archive: Path) -> Path:
    """Install a built extension for headless Ghidra without opening the GUI."""
    EXTENSION_INSTALL_DIR.mkdir(parents=True, exist_ok=True)
    install_root = EXTENSION_INSTALL_DIR.resolve()

    with tempfile.TemporaryDirectory(
        prefix=".reve-extension-staging-", dir=EXTENSION_INSTALL_DIR
    ) as temporary_directory:
        staging = Path(temporary_directory)
        staging_root = staging.resolve()

        with zipfile.ZipFile(archive) as package:
            members = package.infolist()
            roots = {
                Path(member.filename).parts[0]
                for member in members
                if Path(member.filename).parts
            }
            if len(roots) != 1:
                raise RuntimeError(
                    f"Expected one extension directory in {archive.name}, got {sorted(roots)}"
                )

            for member in members:
                destination = (staging / member.filename).resolve()
                if destination != staging_root and staging_root not in destination.parents:
                    raise RuntimeError(f"Unsafe path in extension archive: {member.filename}")
            package.extractall(staging)

        extension_name = roots.pop()
        extracted = staging / extension_name
        required_files = (
            extracted / "extension.properties",
            extracted / "lib" / "reve-ghidra-extension.jar",
            extracted / "lib" / "protobuf-java-4.31.0.jar",
            extracted / "ghidra_scripts" / "Export.java",
        )
        if not all(path.is_file() for path in required_files):
            raise RuntimeError(f"Extension archive is missing required files: {archive}")

        target = install_root / extension_name
        if target.is_symlink():
            raise RuntimeError(f"Refusing to replace a symbolic-link extension path: {target}")

        backup = None
        if target.exists():
            if not target.is_dir():
                raise RuntimeError(f"Extension install path is not a directory: {target}")
            backup = install_root / f".{extension_name}.backup-{os.getpid()}"
            if backup.exists():
                raise RuntimeError(f"Stale extension backup already exists: {backup}")
            target.replace(backup)

        try:
            extracted.replace(target)
        except Exception:
            if backup is not None:
                backup.replace(target)
            raise

        if backup is not None:
            shutil.rmtree(backup)

    return target


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
    if result.returncode != 0:
        return result.returncode

    archives = list((EXTENSION_DIR / "dist").glob("*.zip"))
    if not archives:
        print("Gradle succeeded but did not create an extension ZIP in dist/.", file=sys.stderr)
        return 1

    archive = max(archives, key=lambda path: path.stat().st_mtime_ns)
    try:
        installed = install_extension_archive(archive)
    except (OSError, RuntimeError, zipfile.BadZipFile) as error:
        print(f"Could not install the extension for headless Ghidra: {error}", file=sys.stderr)
        return 1

    print(f"Built extension archive: {archive}")
    print(f"Installed for headless Ghidra: {installed}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
