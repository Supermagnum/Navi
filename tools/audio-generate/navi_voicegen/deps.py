"""System dependency detection (no sudo)."""

from __future__ import annotations

import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path


class SystemDepsError(RuntimeError):
    def __init__(self, messages: list[str]):
        self.messages = messages
        super().__init__("\n".join(messages))


REQUIRED_BINS = ("ffmpeg", "git", "git-lfs", "python3")


def _distro_hint() -> str:
    if Path("/etc/debian_version").is_file():
        return "debian"
    if Path("/etc/fedora-release").is_file():
        return "fedora"
    if Path("/etc/arch-release").is_file():
        return "arch"
    return "unknown"


def install_hints(missing: list[str]) -> list[str]:
    distro = _distro_hint()
    hints: list[str] = []
    if distro == "debian":
        pkgs = []
        for m in missing:
            if m == "ffmpeg":
                pkgs.append("ffmpeg")
            elif m == "git":
                pkgs.append("git")
            elif m == "git-lfs":
                pkgs.append("git-lfs")
            elif m == "python3-venv":
                pkgs.append("python3-venv")
            elif m == "build-essential":
                pkgs.append("build-essential")
        if pkgs:
            hints.append(f"sudo apt-get update && sudo apt-get install -y {' '.join(sorted(set(pkgs)))}")
    elif distro == "fedora":
        hints.append(
            "sudo dnf install -y ffmpeg git git-lfs python3 python3-pip "
            "gcc gcc-c++ make"
        )
    elif distro == "arch":
        hints.append("sudo pacman -S --needed ffmpeg git git-lfs python python-pip base-devel")
    else:
        hints.append(f"Install missing tools ({', '.join(missing)}) using your distro package manager.")
    return hints


def python_venv_available() -> bool:
    try:
        import ensurepip  # noqa: F401
        import venv  # noqa: F401

        return True
    except ImportError:
        return False


def ensure_system_deps(*, for_generate: bool) -> None:
    missing_bins = [b for b in REQUIRED_BINS if shutil.which(b) is None]
    missing: list[str] = list(missing_bins)
    if not python_venv_available():
        missing.append("python3-venv")
    if for_generate and shutil.which("ffmpeg") is None:
        missing.append("ffmpeg")
    if missing:
        msgs = ["Missing required system dependencies:"]
        msgs.extend(f"  - {m}" for m in sorted(set(missing)))
        msgs.append("")
        msgs.extend(install_hints(missing))
        raise SystemDepsError(msgs)


def tool_venv_python(tool_root: Path) -> Path:
    if os.name == "nt":
        return tool_root / ".venv" / "Scripts" / "python.exe"
    return tool_root / ".venv" / "bin" / "python"


def run_venv_python(tool_root: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    py = tool_venv_python(tool_root)
    if not py.is_file():
        raise FileNotFoundError(f"venv Python not found at {py}; run --setup-only first")
    return subprocess.run(
        [str(py), *args],
        cwd=tool_root,
        text=True,
        capture_output=True,
        check=check,
    )


def detect_cuda() -> bool:
    if shutil.which("nvidia-smi") is None:
        return False
    try:
        subprocess.run(
            ["nvidia-smi", "-L"],
            check=True,
            capture_output=True,
            text=True,
        )
        return True
    except (subprocess.CalledProcessError, FileNotFoundError):
        return False


def resolve_device(requested: str) -> str:
    if requested == "auto":
        return "cuda" if detect_cuda() else "cpu"
    if requested not in ("cpu", "cuda"):
        raise ValueError("--device must be auto, cpu, or cuda")
    if requested == "cuda" and not detect_cuda():
        print("warning: CUDA requested but nvidia-smi unavailable; falling back to cpu", file=sys.stderr)
        return "cpu"
    return requested