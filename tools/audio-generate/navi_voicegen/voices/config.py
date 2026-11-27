"""Load tools/audio-generate/voices.yaml."""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import yaml

from navi_voicegen.paths import tool_root


@dataclass(frozen=True)
class VoiceCandidate:
    candidate_id: str
    source_url: str
    source_page: str
    dataset: str
    book_or_corpus: str
    speaker_id: str
    gender_source: str
    licence: str
    licence_url: str
    attribution: str | None
    time_range: tuple[float, float]
    download_sha256: str
    fetch_kind: str = "http"
    catalogue_id: str | None = None
    catalogue_url: str | None = None
    sprakbanken_member: str | None = None
    lj_wav_id: str | None = None
    commonvoice_clip_id: str | None = None
    archive_language: str | None = None
    intended_lang_code: str | None = None
    sample_rate_hz: int | None = None
    dialect: str | None = None


@dataclass(frozen=True)
class GenderVoiceConfig:
    selected: str
    selection_reason: str | None
    candidates: dict[str, VoiceCandidate]
    blocked_reason: str | None = None


@dataclass(frozen=True)
class LanguageVoiceConfig:
    male: GenderVoiceConfig | None
    female: GenderVoiceConfig | None


def voices_yaml_path() -> Path:
    return tool_root() / "voices.yaml"


def load_voices_yaml(path: Path | None = None) -> dict[str, Any]:
    p = path or voices_yaml_path()
    if not p.is_file():
        raise FileNotFoundError(f"Missing voice catalog: {p}")
    raw = yaml.safe_load(p.read_text(encoding="utf-8"))
    if not isinstance(raw, dict):
        raise ValueError(f"Invalid voices.yaml root in {p}")
    return raw


def _parse_candidate(cid: str, blob: dict[str, Any]) -> VoiceCandidate:
    tr = blob.get("time_range") or {}
    start = float(tr.get("start_seconds", 0))
    end = float(tr.get("end_seconds", start + 15))
    if end - start < 10 or end - start > 25:
        pass  # authoring may tune; fetch validates after trim
    sr = blob.get("sample_rate_hz")
    return VoiceCandidate(
        candidate_id=cid,
        source_url=str(blob.get("source_url") or blob.get("catalogue_url") or ""),
        source_page=str(blob.get("source_page") or blob.get("catalogue_url") or ""),
        dataset=str(blob["dataset"]),
        book_or_corpus=str(blob.get("book_or_corpus") or blob.get("book") or blob.get("corpus") or ""),
        speaker_id=str(blob["speaker_id"]),
        gender_source=str(blob.get("gender_source") or ""),
        licence=str(blob["licence"]),
        licence_url=str(blob.get("licence_url") or ""),
        attribution=blob.get("attribution"),
        time_range=(start, end),
        download_sha256=str(blob["download_sha256"]).lower(),
        fetch_kind=str(blob.get("fetch_kind") or "http"),
        catalogue_id=blob.get("catalogue_id"),
        catalogue_url=blob.get("catalogue_url"),
        sprakbanken_member=blob.get("sprakbanken_member"),
        lj_wav_id=blob.get("lj_wav_id"),
        commonvoice_clip_id=blob.get("commonvoice_clip_id"),
        archive_language=blob.get("archive_language"),
        intended_lang_code=blob.get("intended_lang_code"),
        sample_rate_hz=int(sr) if sr is not None else None,
        dialect=blob.get("dialect"),
    )


def _parse_gender(blob: dict[str, Any] | None) -> GenderVoiceConfig | None:
    if blob is None:
        return None
    if blob.get("status") == "blocked":
        return GenderVoiceConfig(
            selected="",
            selection_reason=None,
            candidates={},
            blocked_reason=str(blob.get("reason") or "no acceptable source"),
        )
    cands_raw = blob.get("candidates") or {}
    candidates = {cid: _parse_candidate(cid, c) for cid, c in cands_raw.items()}
    return GenderVoiceConfig(
        selected=str(blob.get("selected") or ""),
        selection_reason=blob.get("selection_reason"),
        candidates=candidates,
    )


def language_voice_config(lang_code: str, raw: dict[str, Any] | None = None) -> LanguageVoiceConfig:
    data = raw if raw is not None else load_voices_yaml()
    langs = data.get("languages") or {}
    if lang_code not in langs:
        raise KeyError(f"No voice catalog entry for lang={lang_code!r} in voices.yaml")
    blob = langs[lang_code]
    return LanguageVoiceConfig(
        male=_parse_gender(blob.get("male")),
        female=_parse_gender(blob.get("female")),
    )


def audition_phrases_for(lang_code: str, raw: dict[str, Any] | None = None) -> list[str]:
    data = raw if raw is not None else load_voices_yaml()
    phrases = (data.get("audition_test_phrases") or {}).get(lang_code)
    if not phrases:
        raise KeyError(f"No audition_test_phrases for {lang_code!r} in voices.yaml")
    return [str(p) for p in phrases[:5]]