"""ffprobe-based quality flags for voice reference clips."""

from __future__ import annotations

import json
import subprocess
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class QualityReport:
    sample_rate_hz: int | None
    duration_s: float | None
    peak_db: float | None
    flags: list[str]

    def to_dict(self) -> dict:
        return {
            "sample_rate_hz": self.sample_rate_hz,
            "duration_s": self.duration_s,
            "peak_db": self.peak_db,
            "flags": self.flags,
        }


def analyze_audio(path: Path) -> QualityReport:
    flags: list[str] = []
    sample_rate: int | None = None
    duration: float | None = None
    peak_db: float | None = None

    probe = subprocess.run(
        [
            "ffprobe",
            "-v",
            "error",
            "-show_entries",
            "stream=sample_rate,duration",
            "-of",
            "json",
            str(path),
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    if probe.returncode == 0 and probe.stdout.strip():
        try:
            meta = json.loads(probe.stdout)
            streams = meta.get("streams") or [{}]
            st0 = streams[0]
            if st0.get("sample_rate"):
                sample_rate = int(st0["sample_rate"])
            if st0.get("duration"):
                duration = float(st0["duration"])
        except (json.JSONDecodeError, ValueError, TypeError):
            flags.append("ffprobe_parse_error")

    vol = subprocess.run(
        [
            "ffmpeg",
            "-hide_banner",
            "-nostats",
            "-i",
            str(path),
            "-af",
            "volumedetect",
            "-f",
            "null",
            "-",
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    for line in (vol.stderr or "").splitlines():
        if "max_volume:" in line:
            try:
                peak_db = float(line.split("max_volume:")[1].strip().split()[0])
            except (IndexError, ValueError):
                pass

    if sample_rate is not None and sample_rate < 22050:
        flags.append("sample_rate_below_22khz")
    if duration is not None:
        if duration < 8.0:
            flags.append("duration_short")
        if duration > 25.0:
            flags.append("duration_long")
    if peak_db is not None:
        if peak_db > -1.0:
            flags.append("possible_clipping")
        if peak_db < -30.0:
            flags.append("quiet_signal")

    return QualityReport(sample_rate_hz=sample_rate, duration_s=duration, peak_db=peak_db, flags=flags)