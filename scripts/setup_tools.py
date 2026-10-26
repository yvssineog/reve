#!/usr/bin/env python3
"""Download the pinned Ghidra release and a compatible Temurin JDK into .tools."""

from __future__ import annotations

import hashlib
import json
import os
import platform
import shutil
import sys
import tarfile
import urllib.error
import urllib.request
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TOOLS_DIR = ROOT / ".tools"

# Pin Ghidra to a public release so everyone on the project uses the same API
# and headless analyzer behavior. SHA-256 is published by the Ghidra release.
GHIDRA_VERSION = "12.2"
GHIDRA_RELEASE_TAG = "Ghidra_12.2_build"
GHIDRA_ARCHIVE = "ghidra_12.2_PUBLIC_20261016.zip"
GHIDRA_SHA256 = "2c2cd776db3c1f6a097ba418922378ac3649cadd5e844a6f5f088ae628a7be67"
GHIDRA_URL = (
    "https://github.com/NationalSecurityAgency/ghidra/releases/download/"
    f"{GHIDRA_RELEASE_TAG}/{GHIDRA_ARCHIVE}"
)
GHIDRA_DIR = TOOLS_DIR / "ghidra" / "ghidra_12.2_PUBLIC"
JDK_DIR = TOOLS_DIR / "java" / "temurin-25"


def platform_key() -> tuple[str, str]:
    """Return the Adoptium API OS and architecture for this machine."""
    system = platform.system().lower()
    machine = platform.machine().lower()
    os_name = {"windows": "windows", "linux": "linux", "darwin": "mac"}.get(system)
    arch = {
        "amd64": "x64",
        "x86_64": "x64",
        "arm64": "aarch64",
        "aarch64": "aarch64",
    }.get(machine)
    if os_name is None or arch is None:
        raise RuntimeError(f"Unsupported platform: {system} {machine}")
    if os_name == "windows" and arch == "aarch64":
        raise RuntimeError("Temurin JDK 25 ARM64 for Windows is not supported by this script")
    return os_name, arch


def request(url: str, *, accept: str | None = None) -> urllib.request.urlopen:
    headers = {"User-Agent": "reve-tool-bootstrap/1.0"}
    if accept:
        headers["Accept"] = accept
    return urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=60)


def download(url: str, destination: Path, expected_sha256: str | None = None) -> str:
    """Download to a temporary file, then verify and publish it atomically."""
    temporary = destination.with_suffix(destination.suffix + ".part")
    digest = hashlib.sha256()
    destination.parent.mkdir(parents=True, exist_ok=True)
    try:
        with request(url) as response, temporary.open("wb") as output:
            total = int(response.headers.get("Content-Length", "0"))
            received = 0
            while chunk := response.read(1024 * 1024):
                output.write(chunk)
                digest.update(chunk)
                received += len(chunk)
                if total:
                    print(f"\r  {received / total:6.1%}", end="", flush=True)
        if total:
            print()
        actual_sha256 = digest.hexdigest()
        if expected_sha256 and actual_sha256.lower() != expected_sha256.lower():
            raise RuntimeError(
                f"SHA-256 mismatch for {destination.name}: expected {expected_sha256}, "
                f"got {actual_sha256}"
            )
        temporary.replace(destination)
        return actual_sha256
    except Exception:
        temporary.unlink(missing_ok=True)
        raise


def get_jdk_package(os_name: str, arch: str) -> tuple[str, str, str]:
    """Resolve the newest Temurin JDK 25 GA archive and its published checksum."""
    url = (
        "https://api.adoptium.net/v3/assets/latest/25/hotspot"
        f"?architecture={arch}&image_type=jdk&os={os_name}&vendor=eclipse"
    )
    with request(url, accept="application/json") as response:
        assets = json.load(response)
    if not assets:
        raise RuntimeError(f"Adoptium returned no JDK 25 package for {os_name}/{arch}")
    package = assets[0]["binary"]["package"]
    return package["link"], package["name"], package["checksum"]


def safe_extract_zip(archive: Path, destination: Path) -> None:
    """Extract a ZIP while rejecting paths that escape the staging directory."""
    root = destination.resolve()
    with zipfile.ZipFile(archive) as zipped:
        for entry in zipped.infolist():
            target = (destination / entry.filename).resolve()
            if target != root and root not in target.parents:
                raise RuntimeError(f"Unsafe path in archive: {entry.filename}")
        zipped.extractall(destination)


def safe_extract_tar(archive: Path, destination: Path) -> None:
    """Extract a tarball using Python's data filter to block unsafe members."""
    with tarfile.open(archive, "r:*") as tar:
        tar.extractall(destination, filter="data")


def install_archive(
    archive: Path,
    staging: Path,
    final_dir: Path,
    expected_top_dir: str | None = None,
) -> Path:
    """Extract into staging and move the archive's JDK/Ghidra root into place."""
    if final_dir.exists():
        raise RuntimeError(f"Destination already exists: {final_dir} (remove it to reinstall)")
    staging.mkdir(parents=True, exist_ok=True)
    if zipfile.is_zipfile(archive):
        safe_extract_zip(archive, staging)
    else:
        safe_extract_tar(archive, staging)

    if expected_top_dir:
        extracted = staging / expected_top_dir
    else:
        java_executable = "java.exe" if os.name == "nt" else "java"
        candidates = list(staging.glob(f"*/bin/{java_executable}"))
        if len(candidates) != 1:
            raise RuntimeError(f"Could not identify JDK root in {archive.name}")
        extracted = candidates[0].parent.parent
    if not extracted.is_dir():
        raise RuntimeError(f"Expected extracted directory is missing: {extracted}")

    final_dir.parent.mkdir(parents=True, exist_ok=True)
    shutil.move(str(extracted), str(final_dir))
    return final_dir


def main() -> int:
    try:
        if sys.version_info < (3, 12):
            raise RuntimeError("Python 3.12 or newer is required (safe tar extraction uses the data filter)")
        os_name, arch = platform_key()
        print(f"Project tools directory: {TOOLS_DIR}")

        if GHIDRA_DIR.joinpath("support", "analyzeHeadless.bat").exists() or GHIDRA_DIR.joinpath(
            "support", "analyzeHeadless"
        ).exists():
            print(f"Ghidra {GHIDRA_VERSION} is already installed: {GHIDRA_DIR}")
        else:
            ghidra_archive = TOOLS_DIR / "downloads" / GHIDRA_ARCHIVE
            if not ghidra_archive.exists():
                print(f"Downloading Ghidra {GHIDRA_VERSION}...")
                download(GHIDRA_URL, ghidra_archive, GHIDRA_SHA256)
            else:
                actual = hashlib.sha256(ghidra_archive.read_bytes()).hexdigest()
                if actual != GHIDRA_SHA256:
                    raise RuntimeError(f"Existing archive has an invalid SHA-256: {ghidra_archive}")
            print("Extracting Ghidra...")
            staging = TOOLS_DIR / ".staging-ghidra"
            install_archive(
                ghidra_archive,
                staging,
                GHIDRA_DIR,
                expected_top_dir="ghidra_12.2_PUBLIC",
            )
            shutil.rmtree(staging, ignore_errors=True)
            print(f"Installed Ghidra: {GHIDRA_DIR}")

        if JDK_DIR.joinpath("bin", "java.exe" if os.name == "nt" else "java").exists():
            print(f"Temurin JDK 25 is already installed: {JDK_DIR}")
        else:
            jdk_url, jdk_archive_name, jdk_sha256 = get_jdk_package(os_name, arch)
            jdk_archive = TOOLS_DIR / "downloads" / jdk_archive_name
            if not jdk_archive.exists():
                print(f"Downloading Temurin JDK 25 for {os_name}/{arch}...")
                download(jdk_url, jdk_archive, jdk_sha256)
            else:
                actual = hashlib.sha256(jdk_archive.read_bytes()).hexdigest()
                if actual.lower() != jdk_sha256.lower():
                    raise RuntimeError(f"Existing archive has an invalid SHA-256: {jdk_archive}")
            print("Extracting Temurin JDK...")
            staging = TOOLS_DIR / ".staging-jdk"
            install_archive(jdk_archive, staging, JDK_DIR)
            shutil.rmtree(staging, ignore_errors=True)
            print(f"Installed Temurin JDK 25: {JDK_DIR}")

        print("\nTools are ready. Ghidra's headless launcher:")
        launcher = "analyzeHeadless.bat" if os.name == "nt" else "analyzeHeadless"
        print(f"  {GHIDRA_DIR / 'support' / launcher}")
        print("The script does not modify JAVA_HOME or the system PATH.")
        return 0
    except (OSError, urllib.error.URLError, zipfile.BadZipFile, tarfile.TarError, RuntimeError) as error:
        print(f"setup_tools: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
