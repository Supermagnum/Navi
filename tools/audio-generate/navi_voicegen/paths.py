"""Clip path resolution under sounds/<language>/..."""

from __future__ import annotations

from pathlib import Path


VALID_GENDERS = frozenset({"male", "female", "persona"})


def tool_root() -> Path:
    return Path(__file__).resolve().parents[1]


def repo_root() -> Path:
    return tool_root().parents[1]


def default_out_dir() -> Path:
    return repo_root() / "build" / "sounds"


def clip_path(
    out_root: Path,
    language_folder: str,
  gender: str,
    concept_key: str,
    ext: str,
    *,
    unreviewed: bool = False,
    persona: bool = False,
) -> Path:
    if gender not in VALID_GENDERS:
        raise ValueError(f"Invalid gender folder: {gender!r}")
    if persona:
        gender = "persona"
    base = out_root / language_folder / gender
    if unreviewed:
        base = base / "unreviewed"
    return base / f"{concept_key}.{ext}"


def state_hash_path(out_root: Path, language_folder: str) -> Path:
    return out_root / language_folder / ".navi-voicegen-state.json"