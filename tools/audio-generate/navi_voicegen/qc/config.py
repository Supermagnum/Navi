"""Load qc.yaml thresholds and generation allowlist."""

from __future__ import annotations

from pathlib import Path
from typing import Any

import yaml

from navi_voicegen.paths import tool_root


def qc_yaml_path() -> Path:
    return tool_root() / "qc.yaml"


def load_qc_config(path: Path | None = None) -> dict[str, Any]:
    p = path or qc_yaml_path()
    if not p.is_file():
        raise FileNotFoundError(f"Missing QC config: {p}")
    raw = yaml.safe_load(p.read_text(encoding="utf-8"))
    if not isinstance(raw, dict):
        raise ValueError(f"Invalid qc.yaml root in {p}")
    return raw


def generate_languages(cfg: dict[str, Any] | None = None) -> list[str]:
    data = cfg if cfg is not None else load_qc_config()
    langs = data.get("generate_languages") or ["en", "nb"]
    return [str(x) for x in langs]


def assert_generate_allowed(lang_code: str, *, action: str = "generation") -> None:
    allowed = generate_languages()
    if lang_code not in allowed:
        raise SystemExit(
            f"{action} refused for lang={lang_code!r}: only {allowed} may be synthesised "
            f"in this round (qc.yaml generate_languages)."
        )


def thresholds_for(lang_code: str, cfg: dict[str, Any] | None = None) -> dict[str, Any]:
    data = cfg if cfg is not None else load_qc_config()
    base = dict(data.get("thresholds") or {})
    overrides = (data.get("language_overrides") or {}).get(lang_code) or {}
    base.update(overrides)
    return base
