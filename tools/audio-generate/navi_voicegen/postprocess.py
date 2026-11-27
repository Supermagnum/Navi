"""ffmpeg trim, loudnorm, encode to Opus or MP3."""

from __future__ import annotations

import json
import subprocess
import tempfile
from pathlib import Path

from navi_voicegen.paths import tool_root

TARGET_I = -16.0
TARGET_LRA = 7.0
TARGET_TP = -1.5


def _load_post_cfg() -> dict:
    lock = tool_root() / "lock" / "manifest.json"
    return json.loads(lock.read_text(encoding="utf-8")).get("postprocess", {})


def encode_clip(wav_in: Path, out_path: Path, fmt: str) -> None:
    cfg = _load_post_cfg()
    i = float(cfg.get("loudnorm_i_lufs", TARGET_I))
    lra = float(cfg.get("loudnorm_lra", TARGET_LRA))
    tp = float(cfg.get("loudnorm_tp_db", TARGET_TP))
    out_path.parent.mkdir(parents=True, exist_ok=True)

    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tmp:
        norm_wav = Path(tmp.name)

    loudnorm = f"loudnorm=I={i}:LRA={lra}:TP={tp}:linear=true"
    subprocess.run(
        [
            "ffmpeg",
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-i",
            str(wav_in),
            "-af",
            loudnorm,
            "-ac",
            "1",
            str(norm_wav),
        ],
        check=True,
    )

    if fmt == "opus":
        br = f"{int(cfg.get('opus_bitrate_kbps', 32))}k"
        subprocess.run(
            [
                "ffmpeg",
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-i",
                str(norm_wav),
                "-ac",
                "1",
                "-c:a",
                "libopus",
                "-b:a",
                br,
                str(out_path),
            ],
            check=True,
        )
    elif fmt == "mp3":
        br = f"{int(cfg.get('mp3_bitrate_kbps', 64))}k"
        subprocess.run(
            [
                "ffmpeg",
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-i",
                str(norm_wav),
                "-ac",
                "1",
                "-c:a",
                "libmp3lame",
                "-b:a",
                br,
                str(out_path),
            ],
            check=True,
        )
    else:
        raise ValueError(f"Unsupported --format: {fmt}")

    norm_wav.unlink(missing_ok=True)