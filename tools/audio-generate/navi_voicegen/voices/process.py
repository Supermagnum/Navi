"""ffmpeg download trim / normalize for voice references."""

from __future__ import annotations

import hashlib
import subprocess
import tempfile
from pathlib import Path


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def download_url(url: str, dest: Path, *, timeout_s: int = 600) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    partial = dest.with_suffix(dest.suffix + ".partial")
    subprocess.run(
        ["curl", "-fsSL", "--retry", "3", "--retry-delay", "2", "--max-time", str(timeout_s), "-o", str(partial), url],
        check=True,
    )
    partial.rename(dest)


def make_reference_wav(
    src: Path,
    dest: Path,
    *,
    start_s: float,
    end_s: float,
    apply_loudnorm: bool = True,
) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    duration = max(0.1, end_s - start_s)
    af_parts = []
    if apply_loudnorm:
        af_parts.append("loudnorm=I=-23:TP=-2:LRA=7:linear=true")
    af = ",".join(af_parts) if af_parts else None
    cmd = [
        "ffmpeg",
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-ss",
        str(start_s),
        "-i",
        str(src),
        "-t",
        str(duration),
    ]
    if af:
        cmd.extend(["-af", af])
    cmd.extend(["-ac", "1", "-ar", "24000", str(dest)])
    subprocess.run(cmd, check=True)