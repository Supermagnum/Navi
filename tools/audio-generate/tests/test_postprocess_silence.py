import struct
import wave
from pathlib import Path

from navi_voicegen.postprocess import _silence_filter, keep_first_utterance


def test_silence_filter_enabled_by_default():
    filt = _silence_filter({})
    assert filt is not None
    assert filt.startswith("silenceremove=")
    assert "stop_periods=1" in filt


def test_silence_filter_can_disable():
    assert _silence_filter({"silence_trim": False}) is None


def _write_wav(path: Path, samples: list[int], rate: int = 16000) -> None:
    with wave.open(str(path), "wb") as writer:
        writer.setnchannels(1)
        writer.setsampwidth(2)
        writer.setframerate(rate)
        writer.writeframes(struct.pack("<" + ("h" * len(samples)), *samples))


def test_keep_first_utterance_drops_trailing_island(tmp_path: Path):
    rate = 16000
    # loud - silence - loud (simulates "Forty" + gap + "now")
    first = [12000] * int(0.3 * rate)
    gap = [0] * int(0.25 * rate)
    second = [10000] * int(0.25 * rate)
    src = tmp_path / "in.wav"
    dst = tmp_path / "out.wav"
    _write_wav(src, first + gap + second, rate)
    keep_first_utterance(src, dst, {})
    with wave.open(str(dst), "rb") as reader:
        n = reader.getnframes()
    # should be ~first island only (plus small pad), well under full length
    assert n < int(0.55 * rate)
    assert n > int(0.25 * rate)
