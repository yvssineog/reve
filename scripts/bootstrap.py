#!/usr/bin/env python3
"""Install the pinned Ghidra and Temurin JDK into .tools, then build and install the reve extension.

Installed tools are skipped and the extension is always rebuilt, so re-run this
after changing engine.proto or build.gradle. Export.java edits need no re-run:
the Ghidra adapter runs the repo's copy.
"""

import hashlib
import json
import os
import platform
import re
import subprocess
import sys
import tempfile
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TOOLS = ROOT / ".tools"
GHIDRA = TOOLS / "ghidra" / "ghidra_12.2_PUBLIC"
JDK = TOOLS / "java" / "temurin-25"
GHIDRA_ZIP = "ghidra_12.2_PUBLIC_20261016.zip"
GHIDRA_URL = f"https://github.com/NationalSecurityAgency/ghidra/releases/download/Ghidra_12.2_build/{GHIDRA_ZIP}"
GHIDRA_SHA256 = "2c2cd776db3c1f6a097ba418922378ac3649cadd5e844a6f5f088ae628a7be67"

OS = {"Darwin": "mac", "Linux": "linux", "Windows": "windows"}[platform.system()]
ARCH = {"x86_64": "x64", "AMD64": "x64", "arm64": "aarch64", "aarch64": "aarch64", "ARM64": "aarch64"}[platform.machine()]
EXE, BAT = (".exe", ".bat") if OS == "windows" else ("", "")

# api.adoptium.net answers urllib's default User-Agent with 403.
opener = urllib.request.build_opener()
opener.addheaders = [("User-Agent", "reve-bootstrap")]
urllib.request.install_opener(opener)


def fetch(url: str, name: str, sha256: str) -> Path:
    """Download into .tools/downloads. A file only lands there once its checksum matches."""
    path = TOOLS / "downloads" / name
    if not path.exists():
        print(f"Downloading {name}")
        path.parent.mkdir(parents=True, exist_ok=True)
        part = path.with_name(name + ".part")
        urllib.request.urlretrieve(url, part, lambda n, size, _: print(f"\r  {n * size >> 20} MiB", end=""))
        print()
        digest = hashlib.sha256()
        with part.open("rb") as file:
            for chunk in iter(lambda: file.read(1 << 20), b""):
                digest.update(chunk)
        if digest.hexdigest() != sha256:
            part.unlink()
            sys.exit(f"SHA-256 mismatch for {name}")
        part.replace(path)
    return path


def unpack(archive: Path, dest: Path) -> None:
    if archive.suffix != ".zip":
        # System tar refuses paths that escape dest; tarfile only does from Python 3.12.
        subprocess.run(["tar", "-xf", archive, "-C", dest], check=True)
        return
    with zipfile.ZipFile(archive) as zipped:
        zipped.extractall(dest)
        # extractall drops Unix modes, and launchers like analyzeHeadless need their executable bit.
        for info in zipped.infolist():
            if mode := info.external_attr >> 16 & 0o777:
                (dest / info.filename).chmod(mode)


def install(archive: Path, dest: Path, marker: str) -> None:
    """Unpack archive in a scratch directory, then move the folder holding `marker` to dest."""
    print(f"Installing {dest.relative_to(ROOT)}")
    with tempfile.TemporaryDirectory(dir=TOOLS) as scratch:
        unpack(archive, Path(scratch))
        [found] = Path(scratch).glob(f"**/{marker}")
        dest.parent.mkdir(parents=True, exist_ok=True)
        found.parents[marker.count("/")].rename(dest)


def gradle(project: Path, task: str) -> None:
    env = os.environ | {"JAVA_HOME": str(JDK), "GHIDRA_INSTALL_DIR": str(GHIDRA), "GRADLE_USER_HOME": str(TOOLS / "gradle")}
    wrapper = GHIDRA / "support" / "gradle" / f"gradlew{BAT}"
    if subprocess.run([wrapper, "--no-daemon", task], cwd=project, env=env).returncode:
        sys.exit(f"gradle {task} failed in {project}")


TOOLS.mkdir(exist_ok=True)

if not (GHIDRA / "support" / f"analyzeHeadless{BAT}").exists():
    install(fetch(GHIDRA_URL, GHIDRA_ZIP, GHIDRA_SHA256), GHIDRA, f"support/analyzeHeadless{BAT}")

if not (JDK / "bin" / f"java{EXE}").exists():
    query = f"architecture={ARCH}&image_type=jdk&os={OS}&vendor=eclipse"
    with urllib.request.urlopen(f"https://api.adoptium.net/v3/assets/latest/25/hotspot?{query}") as response:
        package = json.load(response)[0]["binary"]["package"]
    install(fetch(package["link"], package["name"], package["checksum"]), JDK, f"bin/java{EXE}")

# Pin Ghidra to the project JDK so a JDK it saved from an earlier run can't win.
properties = GHIDRA / "support" / "launch.properties"
override = f"JAVA_HOME_OVERRIDE={JDK.as_posix()}"
properties.write_text(re.sub(r"(?m)^JAVA_HOME_OVERRIDE=.*$", lambda _: override, properties.read_text()))

# Ghidra ships decompiler binaries for x64 Linux and Windows only.
if (OS, ARCH) not in {("linux", "x64"), ("windows", "x64")} and not any(GHIDRA.glob("Ghidra/Features/Decompiler/build/os/*/decompile")):
    gradle(GHIDRA / "support" / "gradle", "buildNatives")

gradle(ROOT / "reve-engine" / "ghidra-adapter" / "ghidra-extension", "installExtension")
print(f"Ready: {GHIDRA / 'support' / f'analyzeHeadless{BAT}'}")
