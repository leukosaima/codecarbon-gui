#!/usr/bin/env python3
"""Freeze the agent with PyInstaller and copy it where Tauri expects its sidecar.

    python build.py [--target <rust-target-triple>] [--no-copy]

The binary lands in ``dist/codecarbon-agent[.exe]`` and is copied to
``../src-tauri/binaries/codecarbon-agent-<target-triple>[.exe]``. The triple
is ``--target`` if given, else the ``host`` of ``rustc -vV``, else a guess
from the current platform.
"""

from __future__ import annotations

import argparse
import platform
import shutil
import stat
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SPEC = HERE / "codecarbon-agent.spec"
DIST = HERE / "dist"
WORK = HERE / "build"
BINARIES = HERE.parent / "src-tauri" / "binaries"
NAME = "codecarbon-agent"


def rustc_host_triple() -> str | None:
    try:
        output = subprocess.run(
            ["rustc", "-vV"], capture_output=True, text=True, check=True, timeout=30
        ).stdout
    except (OSError, subprocess.SubprocessError):
        return None
    for line in output.splitlines():
        if line.startswith("host:"):
            return line.split(":", 1)[1].strip() or None
    return None


def guess_triple() -> str:
    system = platform.system()
    machine = platform.machine().lower()
    arch = "aarch64" if machine in ("arm64", "aarch64") else "x86_64"
    if system == "Darwin":
        return f"{arch}-apple-darwin"
    if system == "Windows":
        return f"{arch}-pc-windows-msvc"
    return f"{arch}-unknown-linux-gnu"


def target_triple(explicit: str | None) -> str:
    return explicit or rustc_host_triple() or guess_triple()


def run_pyinstaller() -> Path:
    command = [
        sys.executable,
        "-m",
        "PyInstaller",
        "--noconfirm",
        "--clean",
        "--distpath",
        str(DIST),
        "--workpath",
        str(WORK),
        str(SPEC),
    ]
    print("+", " ".join(command), flush=True)
    subprocess.run(command, check=True, cwd=HERE)
    for candidate in (DIST / f"{NAME}.exe", DIST / NAME):
        if candidate.is_file():
            return candidate
    raise SystemExit(f"PyInstaller did not produce {DIST / NAME}")


def install(binary: Path, triple: str) -> Path:
    suffix = ".exe" if "windows" in triple else ""
    BINARIES.mkdir(parents=True, exist_ok=True)
    destination = BINARIES / f"{NAME}-{triple}{suffix}"
    shutil.copy2(binary, destination)
    destination.chmod(destination.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    return destination


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--target", help="Rust target triple used to name the sidecar")
    parser.add_argument("--no-copy", action="store_true", help="do not copy to src-tauri")
    args = parser.parse_args(argv)

    binary = run_pyinstaller()
    size_mb = binary.stat().st_size / 1e6
    print(f"built {binary} ({size_mb:.1f} MB)")
    if not args.no_copy:
        destination = install(binary, target_triple(args.target))
        print(f"copied to {destination}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
