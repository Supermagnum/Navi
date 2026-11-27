"""Draft phrase YAML from OSRM / Valhalla template caches."""

from __future__ import annotations

import json
import re
from pathlib import Path

from navi_voicegen.languages import SPECS
from navi_voicegen.paths import tool_root
from navi_voicegen.phrases import PhraseEntry, PhraseFile, phrases_dir, write_phrase_file

# voice-guidance.md §5 concept keys → search hints in flattened OSRM/Valhalla strings.
CONCEPT_OSRM_HINTS: dict[str, list[str]] = {
    "roundabout": ["roundabout", "rundkjøring"],
    "first": ["first", "første", "1."],
    "second": ["second", "andre", "2."],
    "third": ["third", "tredje", "3."],
    "cross": ["cross", "kryss"],
    "left": ["turn left", "til venstre", "left"],
    "right": ["turn right", "til høyre", "right"],
    "u_turn": ["u-turn", "uturn", "u-sving"],
    "straight": ["straight", "rett frem", "continue straight"],
    "continue": ["continue", "fortsett"],
    "exit": ["take the exit", "ta avkjørsel"],
    "merge": ["merge", "flett"],
    "keep_left": ["keep left", "hold til venstre"],
    "keep_right": ["keep right", "hold til høyre"],
    "arrive": ["you have arrived", "du har ankommet", "arrive"],
    "destination": ["destination", "destinasjon"],
    "zero": ["zero", "null"],
    "one": [" one", "én", "en "],
    "two": [" two", "to "],
    "three": [" three", "tre "],
    "four": ["four", "fire"],
    "five": ["five", "fem"],
    "six": ["six", "seks"],
    "seven": ["seven", "syv", "sju"],
    "eight": ["eight", "åtte"],
    "nine": ["nine", "ni"],
    "ten": ["ten", "ti"],
    "eleven": ["eleven", "elleve"],
    "twelve": ["twelve", "tolv"],
    "thirteen": ["thirteen", "tretten"],
    "fourteen": ["fourteen", "fjorten"],
    "fifteen": ["fifteen", "femten"],
    "sixteen": ["sixteen", "seksten"],
    "seventeen": ["seventeen", "sytten"],
    "eighteen": ["eighteen", "atten"],
    "nineteen": ["nineteen", "nitten"],
    "twenty": ["twenty", "tjue"],
    "thirty": ["thirty", "tretti"],
    "forty": ["forty", "førti"],
    "fifty": ["fifty", "femti"],
    "sixty": ["sixty", "seksti"],
    "seventy": ["seventy", "sytti"],
    "eighty": ["eighty", "åtti"],
    "ninety": ["ninety", "nitti"],
    "hundred": ["hundred", "hundre"],
    "thousand": ["thousand", "tusen"],
    "meters": ["meters", "metres", " meter"],
    "kilometers": ["kilometers", "kilometres", "kilometer"],
    "in": ["in ", "om "],
    "then": ["then", "deretter", "så"],
}


def _cache_root() -> Path:
    return tool_root() / "cache" / "phrase-templates"


def _load_osrm_lang(tag: str) -> dict | None:
    path = _cache_root() / "osrm-text-instructions" / "languages" / "translations" / f"{tag}.json"
    if not path.is_file():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def _load_valhalla_lang(tag: str) -> dict | None:
    path = _cache_root() / "valhalla" / "locales" / f"{tag}.json"
    if not path.is_file():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def _flatten_strings(obj, out: list[str]) -> None:
    if isinstance(obj, str):
        out.append(obj)
    elif isinstance(obj, dict):
        for v in obj.values():
            _flatten_strings(v, out)
    elif isinstance(obj, list):
        for v in obj:
            _flatten_strings(v, out)


def _pick_phrase(strings: list[str], hints: list[str]) -> str | None:
    lowered = [
        (s, s.lower())
        for s in strings
        if s.strip() and "<" not in s and "{" not in s
    ]
    for hint in hints:
        for original, low in lowered:
            if hint in low:
                return re.sub(r"\{[^}]+\}", "", original).strip()
    return None


def draft_language(lang_code: str) -> Path:
    spec = SPECS[lang_code]
    if spec.template_source is None:
        raise SystemExit(
            f"Cannot draft phrases for {lang_code}: no OSRM/Valhalla template "
            "(voice-guidance.md §7 — MT + native review only)."
        )
    if not _cache_root().is_dir():
        raise SystemExit("Template cache missing; run navi-voicegen --setup-only first")

    osrm = _load_osrm_lang(spec.template_tag) if spec.template_tag else None
    valh = None
    if spec.template_source in ("valhalla", "either") and spec.template_tag:
        valh = _load_valhalla_lang(spec.template_tag)
    if spec.template_source == "osrm" and osrm is None:
        raise SystemExit(f"OSRM template {spec.template_tag!r} not found in cache")
    if spec.template_source == "valhalla" and valh is None:
        raise SystemExit(f"Valhalla template {spec.template_tag!r} not found in cache")

    strings: list[str] = []
    if osrm is not None:
        _flatten_strings(osrm, strings)
    if valh is not None:
        _flatten_strings(valh, strings)

    commits: dict[str, str | None] = {}
    for repo, sub in (
        ("osrm-text-instructions", "osrm-text-instructions"),
        ("valhalla", "valhalla"),
    ):
        git_dir = _cache_root() / sub / ".git"
        if git_dir.is_dir():
            import subprocess

            r = subprocess.run(
                ["git", "-C", str(_cache_root() / sub), "rev-parse", "HEAD"],
                capture_output=True,
                text=True,
                check=True,
            )
            commits[repo] = r.stdout.strip()

    entries: list[PhraseEntry] = []
    for concept, hints in CONCEPT_OSRM_HINTS.items():
        text = _pick_phrase(strings, hints) or f"[DRAFT {concept}]"
        entries.append(
            PhraseEntry(concept_key=concept, text=text, reviewed=False, pack_style=None)
        )

    pf = PhraseFile(
        lang_code=lang_code,
        language_folder=spec.folder,
        reviewed_file=False,
        template_source=spec.template_source,
        template_commit=commits.get("valhalla") or commits.get("osrm-text-instructions"),
        entries=entries,
    )
    out = phrases_dir() / f"{lang_code}.yaml"
    write_phrase_file(out, pf)
    return out