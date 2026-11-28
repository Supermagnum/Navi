"""ffmpeg-based loudness / silence / peak metrics for QC."""

from __future__ import annotations

import json
import re
import subprocess
from dataclasses import asdict, dataclass
from pathlib import Path


@dataclass
class FfmpegMetrics:
    duration_s: float | None
    integrated_lufs: float | None
    true_peak_dbtp: float | None
    noise_floor_dbfs: float | None
    clipped_samples: int | None
    leading_silence_ms: float | None
    trailing_silence_ms: float | None
    notes: list[str]

    def to_dict(self) -> dict:
        return asdict(self)


def probe_duration(path: Path) -> float | None:
    try:
        out = subprocess.check_output(
            [
                "ffprobe",
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=nw=1:nk=1",
                str(path),
            ],
            text=True,
        ).strip()
        return float(out)
    except Exception:
        return None


def measure(path: Path) -> FfmpegMetrics:
    notes: list[str] = []
    duration = probe_duration(path)
    integrated = None
    true_peak = None
    clipped = None
    noise_floor = None
    lead_ms = None
    trail_ms = None

    # ebur128 + astats
    try:
        proc = subprocess.run(
            [
                "ffmpeg",
                "-hide_banner",
                "-i",
                str(path),
                "-af",
                "ebur128=peak=true,astats=metadata=1:reset=1",
                "-f",
                "null",
                "-",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        err = proc.stderr
        # Prefer the summary line ("    I:         -19.1 LUFS"), not early -70 placeholders.
        matches = re.findall(r"^\s*I:\s*([-\d.]+)\s*LUFS", err, flags=re.MULTILINE)
        if matches:
            integrated = float(matches[-1])
        else:
            m = re.search(r"I:\s*([-\d.]+)\s*LUFS", err)
            if m:
                integrated = float(m.group(1))
        m = re.search(r"Peak:\s*([-\d.]+)\s*dBFS", err)
        if m:
            true_peak = float(m.group(1))
        m = re.search(r"Peak level dB:\s*([-\d.]+)", err)
        if m and true_peak is None:
            true_peak = float(m.group(1))
        m = re.search(r"Number of clipped samples:\s*(\d+)", err)
        if m:
            clipped = int(m.group(1))
        # RMS level as crude noise proxy when silence exists
        m = re.search(r"RMS level dB:\s*([-\d.]+)", err)
        if m:
            noise_floor = float(m.group(1))
    except Exception as exc:  # noqa: BLE001
        notes.append(f"ebur128/astats failed: {exc}")

    # silencedetect for leading/trailing
    try:
        proc = subprocess.run(
            [
                "ffmpeg",
                "-hide_banner",
                "-i",
                str(path),
                "-af",
                "silencedetect=noise=-40dB:d=0.05",
                "-f",
                "null",
                "-",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        starts = [float(x) for x in re.findall(r"silence_start:\s*([-\d.]+)", proc.stderr)]
        ends = [float(x) for x in re.findall(r"silence_end:\s*([-\d.]+)", proc.stderr)]
        if duration is not None:
            if starts and starts[0] <= 0.02 and ends:
                lead_ms = max(0.0, ends[0] * 1000)
            elif not starts:
                lead_ms = 0.0
            # trailing: last silence that reaches EOF
            if starts:
                last_start = starts[-1]
                if duration - last_start < 0.5 or (ends and ends[-1] >= duration - 0.05):
                    trail_ms = max(0.0, (duration - last_start) * 1000)
    except Exception as exc:  # noqa: BLE001
        notes.append(f"silencedetect failed: {exc}")

    return FfmpegMetrics(
        duration_s=duration,
        integrated_lufs=integrated,
        true_peak_dbtp=true_peak,
        noise_floor_dbfs=noise_floor,
        clipped_samples=clipped,
        leading_silence_ms=lead_ms,
        trailing_silence_ms=trail_ms,
        notes=notes,
    )


def metrics_json(path: Path) -> str:
    return json.dumps(measure(path).to_dict(), indent=2)
