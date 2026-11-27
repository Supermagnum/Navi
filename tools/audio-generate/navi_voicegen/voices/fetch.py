"""Download curated voice references into gitignored cache."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path

from navi_voicegen.manifest_out import sha256_file
from navi_voicegen.paths import repo_root, tool_root
from navi_voicegen.voices.config import (
    GenderVoiceConfig,
    VoiceCandidate,
    language_voice_config,
    load_voices_yaml,
)
from navi_voicegen.voices.commonvoice import resolve_commonvoice_local
from navi_voicegen.voices.licence import validate_librivox_language, validate_voice_blob
from navi_voicegen.voices.lj_speech import ensure_lj_wav
from navi_voicegen.voices.process import download_url, make_reference_wav
from navi_voicegen.voices.quality import analyze_audio
from navi_voicegen.voices.sprakbanken import resolve_sprakbanken_download


def voice_cache_root() -> Path:
    return tool_root() / "cache" / "voice-refs"


def candidate_cache_dir(lang: str, gender: str, candidate_id: str) -> Path:
    return voice_cache_root() / lang / gender / candidate_id


def processed_ref_path(lang: str, gender: str, candidate_id: str) -> Path:
    return candidate_cache_dir(lang, gender, candidate_id) / "ref.wav"


def provenance_json_path(lang: str, gender: str, candidate_id: str) -> Path:
    return candidate_cache_dir(lang, gender, candidate_id) / "provenance.json"


@dataclass
class FetchReport:
    lang: str
    gender: str
    candidate_id: str
    ref_wav: Path
    provenance: dict


def _ext_from_url(url: str) -> str:
    path = url.split("?", 1)[0].rstrip("/")
    if path.endswith((".mp3", ".wav", ".opus", ".ogg", ".m4a", ".flac")):
        return path.rsplit(".", 1)[-1].lower()
    return "bin"


def _raw_candidate_blob(lang: str, gender: str, candidate_id: str) -> dict:
    raw = load_voices_yaml()
    return ((raw.get("languages") or {}).get(lang, {}).get(gender, {}).get("candidates") or {})[
        candidate_id
    ]


def _materialize_download(
    lang: str,
    gender: str,
    cand: VoiceCandidate,
    download_dest: Path,
    *,
    force: bool,
) -> None:
    if download_dest.is_file() and not force:
        return
    kind = cand.fetch_kind
    if kind == "http":
        download_url(cand.source_url, download_dest)
        return
    if kind == "sprakbanken":
        member = cand.sprakbanken_member or "channel_1/2187.wav"
        cat = cand.catalogue_id or "sbr-14"
        resolve_sprakbanken_download(cat, member, download_dest)
        return
    if kind == "lj_speech":
        wav_id = cand.lj_wav_id or "LJ001-0001"
        src = ensure_lj_wav(wav_id)
        download_dest.parent.mkdir(parents=True, exist_ok=True)
        download_dest.write_bytes(src.read_bytes())
        return
    if kind == "commonvoice":
        clip = cand.commonvoice_clip_id or cand.candidate_id
        resolve_commonvoice_local(lang, gender, clip, download_dest)
        return
    raise SystemExit(f"Unknown fetch_kind={kind!r} for {lang}/{gender}/{cand.candidate_id}")


def fetch_candidate(lang: str, gender: str, cand: VoiceCandidate, *, force: bool = False) -> FetchReport:
    raw_blob = _raw_candidate_blob(lang, gender, cand.candidate_id)
    validate_voice_blob(raw_blob, candidate_id=cand.candidate_id)
    if cand.archive_language:
        validate_librivox_language(cand.archive_language, lang)

    base = candidate_cache_dir(lang, gender, cand.candidate_id)
    ref = processed_ref_path(lang, gender, cand.candidate_id)
    prov_path = provenance_json_path(lang, gender, cand.candidate_id)

    if ref.is_file() and prov_path.is_file() and not force:
        prov = json.loads(prov_path.read_text(encoding="utf-8"))
        return FetchReport(lang, gender, cand.candidate_id, ref, prov)

    if cand.fetch_kind == "http":
        ext = _ext_from_url(cand.source_url)
    elif cand.fetch_kind == "sprakbanken":
        ext = "wav"
    elif cand.fetch_kind == "lj_speech":
        ext = "wav"
    else:
        ext = "bin"
    download_dest = base / f"download.{ext}"

    _materialize_download(lang, gender, cand, download_dest, force=force)
    got = sha256_file(download_dest)
    placeholder = "0" * 64
    if cand.download_sha256.lower() != placeholder and got != cand.download_sha256.lower():
        raise SystemExit(
            f"SHA-256 mismatch for {lang}/{gender}/{cand.candidate_id}: "
            f"expected {cand.download_sha256}, got {got}"
        )
    if cand.download_sha256.lower() == placeholder:
        print(
            f"note: pin download_sha256 for {lang}/{gender}/{cand.candidate_id} after verifying local "
            f"Språkbanken clip (got {got})",
            flush=True,
        )

    start, end = cand.time_range
    make_reference_wav(download_dest, ref, start_s=start, end_s=end)

    ref_sha = sha256_file(ref)
    quality = analyze_audio(ref)
    prov = {
        "lang_code": lang,
        "gender": gender,
        "candidate_id": cand.candidate_id,
        "source_url": cand.source_url,
        "source_page": cand.source_page,
        "dataset": cand.dataset,
        "book_or_corpus": cand.book_or_corpus,
        "speaker_id": cand.speaker_id,
        "gender_source": cand.gender_source,
        "licence": cand.licence,
        "licence_url": cand.licence_url,
        "attribution": cand.attribution,
        "catalogue_id": cand.catalogue_id,
        "catalogue_url": cand.catalogue_url,
        "time_range_seconds": {"start": start, "end": end},
        "download_sha256": got,
        "ref_wav_sha256": ref_sha,
        "ref_wav_path": str(ref.relative_to(tool_root())),
        "quality": quality.to_dict(),
    }
    if quality.flags:
        print(
            f"quality flags {lang}/{gender}/{cand.candidate_id}: {', '.join(quality.flags)}",
            flush=True,
        )
    prov_path.write_text(json.dumps(prov, indent=2) + "\n", encoding="utf-8")
    return FetchReport(lang, gender, cand.candidate_id, ref, prov)


def fetch_voices(
    lang_codes: list[str],
    *,
    genders: tuple[str, ...] = ("male", "female"),
    force: bool = False,
    max_candidates: int = 3,
) -> list[FetchReport]:
    raw = load_voices_yaml()
    reports: list[FetchReport] = []
    for lang in lang_codes:
        lcfg = language_voice_config(lang, raw)
        for gender in genders:
            gblob = (raw.get("languages") or {}).get(lang, {}).get(gender)
            if gblob and gblob.get("status") == "blocked":
                print(f"skip {lang}/{gender}: {gblob.get('reason')}", flush=True)
                continue
            gcfg = lcfg.male if gender == "male" else lcfg.female
            if gcfg is None:
                print(f"skip {lang}/{gender}: no catalog entry", flush=True)
                continue
            if gcfg.blocked_reason:
                print(f"skip {lang}/{gender}: {gcfg.blocked_reason}", flush=True)
                continue
            for cid, cand in list(gcfg.candidates.items())[:max_candidates]:
                print(f"fetch {lang}/{gender}/{cid} …", flush=True)
                try:
                    reports.append(fetch_candidate(lang, gender, cand, force=force))
                except SystemExit as exc:
                    print(str(exc), flush=True)
    return reports


def write_pack_attribution(language_folder: str, provenances: list[dict], out_dir: Path | None = None) -> Path | None:
    """Write build/sounds/<language>/ATTRIBUTION.txt when any CC-BY voice was used."""
    lines: list[str] = []
    for p in provenances:
        if p.get("licence", "").lower() not in ("cc-by-4.0", "cc-by", "cc-by-sa-4.0", "cc-by-sa"):
            continue
        attr = p.get("attribution") or ""
        if not attr:
            attr = (
                f"Voice reference: {p.get('speaker_id')} from {p.get('dataset')} "
                f"({p.get('book_or_corpus')}). Source: {p.get('source_page')}"
            )
        lines.append(attr.strip())

    if not lines:
        return None
    root = out_dir or (repo_root() / "build" / "sounds")
    lang_dir = root / language_folder
    lang_dir.mkdir(parents=True, exist_ok=True)
    out = lang_dir / "ATTRIBUTION.txt"
    body = "\n\n".join(sorted(set(lines))) + "\n"
    out.write_text(body, encoding="utf-8")
    return out