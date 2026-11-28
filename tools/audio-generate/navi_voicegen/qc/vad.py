"""Silero VAD speech-region detection for QC."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass
class VadRegion:
    start_s: float
    end_s: float


_vad_model: Any | None = None


def _load_silero():
    global _vad_model
    if _vad_model is not None:
        return _vad_model
    import torch

    model, utils = torch.hub.load(
        repo_or_dir="snakers4/silero-vad",
        model="silero_vad",
        trust_repo=True,
    )
    get_speech_timestamps = utils[0]
    _vad_model = (model, get_speech_timestamps)
    return _vad_model


def speech_regions(audio_path: Path, *, sampling_rate: int = 16000) -> list[VadRegion]:
    """Return speech regions; empty list on failure (caller treats as unavailable)."""
    try:
        import torch
        import torchaudio

        model, get_speech_timestamps = _load_silero()
        wav, sr = torchaudio.load(str(audio_path))
        if wav.shape[0] > 1:
            wav = wav.mean(dim=0, keepdim=True)
        if sr != sampling_rate:
            wav = torchaudio.functional.resample(wav, sr, sampling_rate)
        wav = wav.squeeze(0)
        ts = get_speech_timestamps(wav, model, sampling_rate=sampling_rate)
        return [VadRegion(t["start"] / sampling_rate, t["end"] / sampling_rate) for t in ts]
    except Exception:
        return _ffmpeg_energy_regions(audio_path)


def _ffmpeg_energy_regions(audio_path: Path) -> list[VadRegion]:
    """Fallback energy VAD via decoded PCM when Silero is unavailable."""
    import struct
    import subprocess
    import tempfile
    import wave

    with tempfile.NamedTemporaryFile(suffix=".wav") as tmp:
        subprocess.run(
            [
                "ffmpeg",
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-i",
                str(audio_path),
                "-ac",
                "1",
                "-ar",
                "16000",
                tmp.name,
            ],
            check=True,
        )
        with wave.open(tmp.name, "rb") as reader:
            raw = reader.readframes(reader.getnframes())
            rate = reader.getframerate()
        samples = [x / 32768.0 for x in struct.unpack("<" + ("h" * (len(raw) // 2)), raw)]
    win = int(0.02 * rate)
    gap = int(0.12 / 0.02)
    regions: list[VadRegion] = []
    start: int | None = None
    quiet = 0
    for i in range(0, len(samples) - win, win):
        chunk = samples[i : i + win]
        rms = (sum(x * x for x in chunk) / len(chunk)) ** 0.5
        spoken = rms >= 0.02
        frame = i // win
        if spoken:
            if start is None:
                start = frame
            quiet = 0
        elif start is not None:
            quiet += 1
            if quiet >= gap:
                end = frame - quiet
                regions.append(VadRegion(start * 0.02, (end + 1) * 0.02))
                start = None
                quiet = 0
    if start is not None:
        regions.append(VadRegion(start * 0.02, len(samples) / rate))
    return regions
