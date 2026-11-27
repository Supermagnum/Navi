"""NST PCM22 utterance files (22.05 kHz mono int16, 16-byte header)."""

from __future__ import annotations

import struct
import subprocess
import tempfile
from pathlib import Path

PCM22_HEADER = b"PCM22   "
PCM22_SAMPLE_RATE = 22050


def pcm22_to_wav(src: Path, dest: Path) -> Path:
    raw = src.read_bytes()
    if not raw.startswith(PCM22_HEADER):
        raise ValueError(f"Not PCM22: {src}")
    if len(raw) <= 16:
        raise ValueError(f"PCM22 payload too short: {src}")
    pcm = raw[16:]
    dest.parent.mkdir(parents=True, exist_ok=True)
    with dest.open("wb") as out:
        out.write(b"RIFF")
        out.write(struct.pack("<I", 36 + len(pcm)))
        out.write(b"WAVEfmt ")
        out.write(struct.pack("<IHHIIHH", 16, 1, 1, PCM22_SAMPLE_RATE, PCM22_SAMPLE_RATE * 2, 2, 16))
        out.write(b"data")
        out.write(struct.pack("<I", len(pcm)))
        out.write(pcm)
    return dest


def concat_pcm22_segments(sources: list[Path], dest_wav: Path) -> Path:
    with tempfile.TemporaryDirectory() as tmp:
        tdir = Path(tmp)
        parts: list[Path] = []
        for i, src in enumerate(sources):
            part = tdir / f"p{i:03d}.wav"
            pcm22_to_wav(src, part)
            parts.append(part)
        list_path = tdir / "list.txt"
        list_path.write_text("".join(f"file '{p}'\n" for p in parts), encoding="utf-8")
        dest_wav.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            [
                "ffmpeg",
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "concat",
                "-safe",
                "0",
                "-i",
                str(list_path),
                "-ac",
                "1",
                "-ar",
                str(PCM22_SAMPLE_RATE),
                str(dest_wav),
            ],
            check=True,
        )
    return dest_wav