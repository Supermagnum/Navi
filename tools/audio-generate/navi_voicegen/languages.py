"""Language tiers, folder names, template sources, and Chatterbox IDs."""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import Iterable


class PackStyle(str, Enum):
    FRAGMENT = "fragment"
    WHOLE_PHRASE = "whole_phrase"
    HYBRID = "hybrid"


@dataclass(frozen=True)
class LanguageSpec:
    code: str
    folder: str
    tier: int
    pack_style_default: PackStyle
    chatterbox_id: str | None
    template_source: str | None  # osrm tag, valhalla tag, or None (MT / hand)
    template_tag: str | None


# Chatterbox Multilingual language_id values (subset; unsupported => None).
# See resemble-ai/chatterbox docs; Navi refuses generation when None.
TIER1: tuple[str, ...] = (
    "en",
    "fr",
    "de",
    "ru",
    "ar",
    "zh",
    "ja",
    "uk",
    "hi",
)
TIER2: tuple[str, ...] = ("nb", "sv", "da", "fi", "pl", "nl", "tr")
TIER3: tuple[str, ...] = (
    "is",
    "el",
    "sw",
    "ms",
    "cs",
    "hu",
    "ro",
    "vi",
    "lv",
    "sr",
    "ka",
    "kk",
    "ne",
)

LANG_ALIASES: dict[str, str] = {
    "english": "en",
    "french": "fr",
    "german": "de",
    "russian": "ru",
    "arabic": "ar",
    "chinese": "zh",
    "mandarin": "zh",
    "japanese": "ja",
    "ukrainian": "uk",
    "hindi": "hi",
    "norwegian": "nb",
    "no": "nb",
    "swedish": "sv",
    "danish": "da",
    "finnish": "fi",
    "polish": "pl",
    "dutch": "nl",
    "turkish": "tr",
    "icelandic": "is",
    "greek": "el",
    "swahili": "sw",
    "malay": "ms",
    "czech": "cs",
    "hungarian": "hu",
    "romanian": "ro",
    "vietnamese": "vi",
    "latvian": "lv",
    "serbian": "sr",
    "georgian": "ka",
    "kazakh": "kk",
    "nepali": "ne",
    "tier1": "tier1",
    "tier2": "tier2",
    "tier3": "tier3",
    "all": "all",
}

SPECS: dict[str, LanguageSpec] = {
    "en": LanguageSpec("en", "english", 1, PackStyle.FRAGMENT, "en", "either", "en"),
    "fr": LanguageSpec("fr", "french", 1, PackStyle.FRAGMENT, "fr", "either", "fr"),
    "de": LanguageSpec("de", "german", 1, PackStyle.FRAGMENT, "de", "either", "de"),
    "ru": LanguageSpec("ru", "russian", 1, PackStyle.WHOLE_PHRASE, "ru", "either", "ru"),
    "ar": LanguageSpec("ar", "arabic", 1, PackStyle.WHOLE_PHRASE, "ar", "either", "ar"),
    "zh": LanguageSpec("zh", "chinese", 1, PackStyle.HYBRID, "zh", "osrm", "zh-Hans"),
    "ja": LanguageSpec("ja", "japanese", 1, PackStyle.HYBRID, "ja", "either", "ja"),
    "uk": LanguageSpec("uk", "ukrainian", 1, PackStyle.WHOLE_PHRASE, "uk", "either", "uk"),
    "hi": LanguageSpec("hi", "hindi", 1, PackStyle.HYBRID, "hi", "valhalla", "hi-IN"),
    "nb": LanguageSpec("nb", "norwegian", 2, PackStyle.WHOLE_PHRASE, "no", "valhalla", "nb-NO"),
    "sv": LanguageSpec("sv", "swedish", 2, PackStyle.WHOLE_PHRASE, "sv", "either", "sv"),
    "da": LanguageSpec("da", "danish", 2, PackStyle.FRAGMENT, "da", "either", "da"),
    "fi": LanguageSpec("fi", "finnish", 2, PackStyle.WHOLE_PHRASE, "fi", "either", "fi"),
    "pl": LanguageSpec("pl", "polish", 2, PackStyle.WHOLE_PHRASE, "pl", "either", "pl"),
    "nl": LanguageSpec("nl", "dutch", 2, PackStyle.FRAGMENT, "nl", "either", "nl"),
    "tr": LanguageSpec("tr", "turkish", 2, PackStyle.FRAGMENT, "tr", "either", "tr"),
    "is": LanguageSpec("is", "icelandic", 3, PackStyle.HYBRID, None, None, None),
    "el": LanguageSpec("el", "greek", 3, PackStyle.WHOLE_PHRASE, None, "valhalla", "el-GR"),
    "sw": LanguageSpec("sw", "swahili", 3, PackStyle.HYBRID, None, None, None),
    "ms": LanguageSpec("ms", "malay", 3, PackStyle.HYBRID, None, None, None),
    "cs": LanguageSpec("cs", "czech", 3, PackStyle.WHOLE_PHRASE, None, "valhalla", "cs-CZ"),
    "hu": LanguageSpec("hu", "hungarian", 3, PackStyle.FRAGMENT, "hu", "either", "hu"),
    "ro": LanguageSpec("ro", "romanian", 3, PackStyle.FRAGMENT, "ro", "either", "ro"),
    "vi": LanguageSpec("vi", "vietnamese", 3, PackStyle.HYBRID, "vi", "either", "vi"),
    "lv": LanguageSpec("lv", "latvian", 3, PackStyle.WHOLE_PHRASE, None, None, None),
    "sr": LanguageSpec("sr", "serbian", 3, PackStyle.WHOLE_PHRASE, None, None, None),
    "ka": LanguageSpec("ka", "georgian", 3, PackStyle.HYBRID, None, None, None),
    "kk": LanguageSpec("kk", "kazakh", 3, PackStyle.HYBRID, None, None, None),
    "ne": LanguageSpec("ne", "nepali", 3, PackStyle.HYBRID, None, None, None),
}


def normalize_lang_token(token: str) -> str:
    t = token.strip().lower().replace("_", "-")
    return LANG_ALIASES.get(t, t)


def resolve_language_codes(lang_arg: str) -> list[str]:
    key = normalize_lang_token(lang_arg)
    if key == "all":
        return sorted(SPECS.keys())
    if key == "tier1":
        return list(TIER1)
    if key == "tier2":
        return list(TIER2)
    if key == "tier3":
        return list(TIER3)
    if key not in SPECS:
        raise ValueError(f"Unknown language selector: {lang_arg!r}")
    return [key]


def iter_tier_matrix() -> Iterable[tuple[str, LanguageSpec, str]]:
    """Yield (code, spec, chatterbox_support) for reporting."""
    for code, spec in sorted(SPECS.items()):
        if spec.chatterbox_id:
            status = f"chatterbox:{spec.chatterbox_id}"
        else:
            status = "chatterbox:unsupported"
        yield code, spec, status


def language_folder_table() -> list[tuple[str, str, int, str | None]]:
    """Return (iso_code, sounds_folder, tier, chatterbox_id) for docs/CLI."""
    rows: list[tuple[str, str, int, str | None]] = []
    for code, spec in sorted(SPECS.items()):
        rows.append((code, spec.folder, spec.tier, spec.chatterbox_id))
    return rows


def folder_name_for(lang_code: str) -> str:
    return SPECS[lang_code].folder


def resolve_pack_style(spec: LanguageSpec, mode: str) -> PackStyle:
    if mode == "fragment":
        return PackStyle.FRAGMENT
    if mode == "whole_phrase":
        return PackStyle.WHOLE_PHRASE
    if mode == "hybrid":
        return PackStyle.HYBRID
    if mode != "auto":
        raise ValueError(f"Unknown pack mode: {mode!r}")
    return spec.pack_style_default