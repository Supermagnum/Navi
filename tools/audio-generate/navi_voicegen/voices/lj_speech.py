"""LJ Speech (PD) download + single-utterance extract."""

from __future__ import annotations

import shutil
import tarfile
from pathlib import Path

from navi_voicegen.paths import tool_root
from navi_voicegen.voices.process import download_url, sha256_file

LJ_ARCHIVE_URL = "https://data.keithito.com/data/speech/LJSpeech-1.1.tar.bz2"
LJ_CACHE_ARCHIVE = tool_root() / "cache" / "lj-speech" / "LJSpeech-1.1.tar.bz2"
LJ_EXTRACT_ROOT = tool_root() / "cache" / "lj-speech" / "extracted"


def lj_wav_path(wav_id: str) -> Path:
    return LJ_EXTRACT_ROOT / "LJSpeech-1.1" / "wavs" / f"{wav_id}.wav"


def ensure_lj_wav(wav_id: str) -> Path:
    out = lj_wav_path(wav_id)
    if out.is_file():
        return out
    LJ_CACHE_ARCHIVE.parent.mkdir(parents=True, exist_ok=True)
    if not LJ_CACHE_ARCHIVE.is_file():
        print(f"Downloading LJ Speech archive (~2.6 GiB) to {LJ_CACHE_ARCHIVE} …", flush=True)
        download_url(LJ_ARCHIVE_URL, LJ_CACHE_ARCHIVE, timeout_s=7200)
    member = f"LJSpeech-1.1/wavs/{wav_id}.wav"
    LJ_EXTRACT_ROOT.mkdir(parents=True, exist_ok=True)
    with tarfile.open(LJ_CACHE_ARCHIVE, "r:bz2") as tf:
        hit = next((m for m in tf.getmembers() if m.name == member), None)
        if hit is None:
            raise SystemExit(f"LJ Speech member missing: {member!r}")
        src = tf.extractfile(hit)
        if src is None:
            raise SystemExit(f"Failed to extract {member!r}")
        out.parent.mkdir(parents=True, exist_ok=True)
        with src, out.open("wb") as dest:
            shutil.copyfileobj(src, dest)
    return out


def lj_wav_sha256(wav_id: str) -> str:
    return sha256_file(ensure_lj_wav(wav_id))