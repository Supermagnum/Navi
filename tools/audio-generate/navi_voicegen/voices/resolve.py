"""Resolve default voice-ref wav from cache or CLI overrides."""

from __future__ import annotations

import json
from pathlib import Path

from navi_voicegen.paths import tool_root
from navi_voicegen.voices.config import language_voice_config
from navi_voicegen.voices.fetch import processed_ref_path, provenance_json_path


def resolve_voice_ref_path(
    lang_code: str,
    gender: str,
    *,
    override: Path | None = None,
) -> tuple[Path | None, dict | None]:
    if override is not None and override.is_file():
        return override, {"source": "cli_override", "path": str(override)}

    lcfg = language_voice_config(lang_code)
    gcfg = lcfg.male if gender == "male" else lcfg.female
    if gcfg is None or gcfg.blocked_reason:
        return None, None
    selected = gcfg.selected
    if not selected or selected not in gcfg.candidates:
        return None, None
    ref = processed_ref_path(lang_code, gender, selected)
    prov_path = provenance_json_path(lang_code, gender, selected)
    if not ref.is_file():
        return None, None
    prov = None
    if prov_path.is_file():
        prov = json.loads(prov_path.read_text(encoding="utf-8"))
    return ref, prov