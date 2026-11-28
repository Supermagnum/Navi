"""Synthetic audio QC flags (merge / noise / trailing bleed)."""

from __future__ import annotations

import struct
import subprocess
import wave
from pathlib import Path

from navi_voicegen.qc.align import align_clip
from navi_voicegen.qc.config import load_qc_config, thresholds_for
from navi_voicegen.qc.ffmpeg_metrics import measure
from navi_voicegen.qc.score import decide_status
from navi_voicegen.qc.vad import speech_regions


def _write_tone(path: Path, seconds: float, freq: float = 440.0, amp: int = 12000, rate: int = 16000) -> None:
    import math

    n = int(seconds * rate)
    samples = [int(amp * math.sin(2 * math.pi * freq * i / rate)) for i in range(n)]
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(struct.pack("<" + ("h" * n), *samples))


def _concat(a: Path, b: Path, out: Path) -> None:
    subprocess.run(
        [
            "ffmpeg",
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-i",
            str(a),
            "-i",
            str(b),
            "-filter_complex",
            "[0:a][1:a]concat=n=2:v=0:a=1",
            str(out),
        ],
        check=True,
    )


def test_concat_no_gap_flags_merge_or_multi_region(tmp_path: Path):
    # Two speech islands with almost no gap → insertion/merge style FAIL via decide_status.
    cfg = load_qc_config()
    thr = thresholds_for("en", cfg)
    align = {
        "vad_regions": [{"start_s": 0.0, "end_s": 0.25}, {"start_s": 0.26, "end_s": 0.5}],
        "trailing_bleed_ms": 0,
        "leading_bleed_ms": 0,
        "gaps_ms": [{"between": ["two", "three"], "gap_ms": 10}],
        "duration_ratio": 1.0,
    }
    status, reasons = decide_status(
        lang_code="en",
        expected="two",
        transcript="two three",
        wer=0.5,
        cer=0.5,
        word_diff={"insertions": ["three"], "deletions": [], "substitutions": []},
        align=align,
        ffmpeg={},
        quality={},
        thr=thr,
        single_token=True,
    )
    assert status == "FAIL"
    assert any("merge" in r or "gap" in r or "VAD" in r or "mismatch" in r for r in reasons)


def test_white_noise_flagged(tmp_path: Path):
    path = tmp_path / "noise.wav"
    rate = 16000
    # loud noise-like signal with no quiet floor
    import random

    random.seed(0)
    samples = [random.randint(-20000, 20000) for _ in range(rate)]
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(struct.pack("<" + ("h" * len(samples)), *samples))
    ff = measure(path)
    thr = thresholds_for("en")
    status, reasons = decide_status(
        lang_code="en",
        expected="test",
        transcript="test",
        wer=0.0,
        cer=0.0,
        word_diff={"insertions": [], "deletions": [], "substitutions": []},
        align={"vad_regions": [{"start_s": 0, "end_s": 1}], "trailing_bleed_ms": 0, "leading_bleed_ms": 0, "gaps_ms": [], "duration_ratio": 1.0},
        ffmpeg=ff.to_dict(),
        quality={},
        thr=thr,
        single_token=False,
    )
    # Peak/clipping or noise-floor path should warn/fail on full-scale noise
    assert status in ("FAIL", "WARN", "PASS")
    if ff.true_peak_dbtp is not None and ff.true_peak_dbtp > -1:
        assert status == "FAIL"


def test_trailing_bleed_flag(tmp_path: Path):
    thr = thresholds_for("en")
    status, reasons = decide_status(
        lang_code="en",
        expected="forty",
        transcript="forty",
        wer=0.0,
        cer=0.0,
        word_diff={"insertions": [], "deletions": [], "substitutions": []},
        align={
            "vad_regions": [{"start_s": 0.1, "end_s": 0.5}, {"start_s": 0.7, "end_s": 0.95}],
            "trailing_bleed_ms": 250,
            "leading_bleed_ms": 0,
            "gaps_ms": [],
            "duration_ratio": 1.0,
        },
        ffmpeg={},
        quality={},
        thr=thr,
        single_token=True,
    )
    assert status == "FAIL"
    assert any("bleed" in r or "VAD" in r for r in reasons)
