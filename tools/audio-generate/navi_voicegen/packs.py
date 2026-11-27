"""Locate generated clips on disk."""

from __future__ import annotations

from pathlib import Path


def find_clip_for_key(pack_root: Path, gender: str, concept_key: str, ext: str) -> Path | None:
    """pack_root is …/norwegian or …/unreviewed/norwegian."""
    candidates = [
        pack_root / gender / f"{concept_key}.{ext}",
        pack_root / gender / "unreviewed" / f"{concept_key}.{ext}",
    ]
    for p in candidates:
        if p.is_file():
            return p
    return None