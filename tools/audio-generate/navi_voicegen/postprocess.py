"""ffmpeg trim, loudnorm, encode to Opus or MP3."""

from __future__ import annotations

import json
import struct
import subprocess
import tempfile
import wave
from pathlib import Path

from navi_voicegen.paths import tool_root

TARGET_I = -16.0
TARGET_LRA = 7.0
TARGET_TP = -1.5

DEFAULT_SILENCE_STOP_PERIODS = 1
DEFAULT_SILENCE_STOP_DURATION = 0.18
DEFAULT_SILENCE_THRESHOLD_DB = -40.0
DEFAULT_SILENCE_START_DURATION = 0.05
DEFAULT_FIRST_UTTERANCE_PAD_S = 0.05
DEFAULT_FIRST_UTTERANCE_GAP_S = 0.12
DEFAULT_FIRST_UTTERANCE_RMS = 0.02


def _load_post_cfg() -> dict:
    lock = tool_root() / "lock" / "manifest.json"
    return json.loads(lock.read_text(encoding="utf-8")).get("postprocess", {})


def _silence_filter(cfg: dict) -> str | None:
    if cfg.get("silence_trim", True) is False:
        return None
    start_dur = float(cfg.get("silence_start_duration_s", DEFAULT_SILENCE_START_DURATION))
    stop_dur = float(cfg.get("silence_stop_duration_s", DEFAULT_SILENCE_STOP_DURATION))
    thr = float(cfg.get("silence_threshold_db", DEFAULT_SILENCE_THRESHOLD_DB))
    stop_periods = int(cfg.get("silence_stop_periods", DEFAULT_SILENCE_STOP_PERIODS))
    return (
        "silenceremove="
        f"start_periods=1:start_duration={start_dur}:start_threshold={thr}dB:"
        f"stop_periods={stop_periods}:stop_duration={stop_dur}:stop_threshold={thr}dB"
    )


def _speech_regions(
    samples: list[float],
    rate: int,
    *,
    rms_threshold: float,
    min_gap_s: float,
) -> list[tuple[int, int]]:
    """Return inclusive sample index ranges for speech islands."""
    win = max(1, int(0.02 * rate))
    gap_frames = max(1, int(min_gap_s / 0.02))
    frame_speech: list[bool] = []
    for i in range(0, len(samples) - win + 1, win):
        chunk = samples[i : i + win]
        rms = (sum(x * x for x in chunk) / len(chunk)) ** 0.5
        frame_speech.append(rms >= rms_threshold)

    regions: list[tuple[int, int]] = []
    start_f: int | None = None
    quiet = 0
    for idx, spoken in enumerate(frame_speech):
        if spoken:
            if start_f is None:
                start_f = idx
            quiet = 0
        elif start_f is not None:
            quiet += 1
            if quiet >= gap_frames:
                end_f = idx - quiet
                regions.append((start_f * win, min(len(samples), (end_f + 1) * win)))
                start_f = None
                quiet = 0
    if start_f is not None:
        regions.append((start_f * win, len(samples)))
    return regions


def keep_first_utterance(wav_in: Path, wav_out: Path, cfg: dict) -> None:
    """Drop trailing speech islands (e.g. last-resort \"now\" pad after a gap)."""
    if cfg.get("first_utterance_trim", True) is False:
        wav_out.write_bytes(wav_in.read_bytes())
        return

    with wave.open(str(wav_in), "rb") as reader:
        channels = reader.getnchannels()
        width = reader.getsampwidth()
        rate = reader.getframerate()
        nframes = reader.getnframes()
        raw = reader.readframes(nframes)

    if channels != 1 or width != 2:
        # Unexpected layout from Chatterbox — skip island trim rather than corrupt.
        wav_out.write_bytes(wav_in.read_bytes())
        return

    samples = list(struct.unpack("<" + ("h" * (len(raw) // 2)), raw))
    floats = [s / 32768.0 for s in samples]
    rms_thr = float(cfg.get("first_utterance_rms", DEFAULT_FIRST_UTTERANCE_RMS))
    gap_s = float(cfg.get("first_utterance_gap_s", DEFAULT_FIRST_UTTERANCE_GAP_S))
    pad_s = float(cfg.get("first_utterance_pad_s", DEFAULT_FIRST_UTTERANCE_PAD_S))
    regions = _speech_regions(floats, rate, rms_threshold=rms_thr, min_gap_s=gap_s)
    if not regions:
        wav_out.write_bytes(wav_in.read_bytes())
        return

    start, end = regions[0]
    pad = int(pad_s * rate)
    start = max(0, start - pad)
    end = min(len(samples), end + pad)
    clipped = samples[start:end]
    out_raw = struct.pack("<" + ("h" * len(clipped)), *clipped)
    wav_out.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(wav_out), "wb") as writer:
        writer.setnchannels(1)
        writer.setsampwidth(2)
        writer.setframerate(rate)
        writer.writeframes(out_raw)


def encode_clip(wav_in: Path, out_path: Path, fmt: str) -> None:
    cfg = _load_post_cfg()
    i = float(cfg.get("loudnorm_i_lufs", TARGET_I))
    lra = float(cfg.get("loudnorm_lra", TARGET_LRA))
    tp = float(cfg.get("loudnorm_tp_db", TARGET_TP))
    out_path.parent.mkdir(parents=True, exist_ok=True)

    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tmp_utt:
        utt_wav = Path(tmp_utt.name)
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tmp_norm:
        norm_wav = Path(tmp_norm.name)

    try:
        keep_first_utterance(wav_in, utt_wav, cfg)

        loudnorm = f"loudnorm=I={i}:LRA={lra}:TP={tp}:linear=true"
        filters = [part for part in (_silence_filter(cfg), loudnorm) if part]

        subprocess.run(
            [
                "ffmpeg",
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-i",
                str(utt_wav),
                "-af",
                ",".join(filters),
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
    finally:
        utt_wav.unlink(missing_ok=True)
        norm_wav.unlink(missing_ok=True)
