"""Hard licence and source gates for voices.yaml candidates."""

from __future__ import annotations

import re
from typing import Any

ALLOWED_LICENCE_PREFIXES: tuple[str, ...] = (
    "public-domain",
    "pd",
    "cc0",
    "cc-by",
    "cc-by-sa",
)

FORBIDDEN_URL_PATTERNS: tuple[re.Pattern[str], ...] = (
    re.compile(r"youtube\.com|youtu\.be", re.I),
    re.compile(r"facebook\.com|fb\.watch", re.I),
    re.compile(r"spotify\.com", re.I),
)

FORBIDDEN_CORPUS_PATTERNS: tuple[re.Pattern[str], ...] = (
    re.compile(r"\bnpsc\b", re.I),
    re.compile(r"stortinget", re.I),
    re.compile(r"norwegian.?parliament", re.I),
    re.compile(r"norwegischevolksmahrchen", re.I),
)


def _norm_licence(blob: dict[str, Any]) -> str:
    return str(blob.get("licence") or "").strip().lower()


def licence_allowed(blob: dict[str, Any]) -> bool:
    lic = _norm_licence(blob)
    if not lic:
        return False
    return any(lic.startswith(prefix) for prefix in ALLOWED_LICENCE_PREFIXES)


def validate_voice_blob(blob: dict[str, Any], *, candidate_id: str) -> None:
    if not licence_allowed(blob):
        raise SystemExit(
            f"Voice candidate {candidate_id!r} rejected: missing or disallowed licence "
            f"({blob.get('licence')!r}); allowed PD/CC0/CC-BY/CC-BY-SA only."
        )
    url = str(blob.get("source_url") or "")
    for pat in FORBIDDEN_URL_PATTERNS:
        if pat.search(url):
            raise SystemExit(f"Voice candidate {candidate_id!r} rejected: forbidden URL host ({url!r}).")
    hay = " ".join(
        str(blob.get(k) or "")
        for k in (
            "source_url",
            "source_page",
            "book_or_corpus",
            "dataset",
            "speaker_id",
            "catalogue_id",
        )
    )
    for pat in FORBIDDEN_CORPUS_PATTERNS:
        if pat.search(hay):
            raise SystemExit(
                f"Voice candidate {candidate_id!r} rejected: forbidden corpus/source ({pat.pattern})."
            )
    if blob.get("fetch_kind") == "librivox_archive":
        lang = str(blob.get("archive_language") or "").lower()
        if lang and lang not in ("nor", "nob", "no", "eng", "en", "deu", "de", "fra", "fr"):
            pass
        if lang in ("deu", "de") and str(blob.get("intended_lang_code") or "") == "nb":
            raise SystemExit(
                f"Voice candidate {candidate_id!r} rejected: LibriVox German recording cannot voice nb packs."
            )


def validate_librivox_language(archive_language: str | None, lang_code: str) -> None:
    if not archive_language:
        return
    al = archive_language.strip().lower()
    wanted = {"nb": {"nor", "nob", "no"}, "en": {"eng", "en"}, "de": {"deu", "de"}, "fr": {"fra", "fr"}}
    ok = wanted.get(lang_code)
    if ok and al not in ok:
        raise SystemExit(
            f"LibriVox archive language {archive_language!r} does not match pack language {lang_code!r}."
        )