"""Write per-language build/sounds/<language>/manifest.json."""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


@dataclass
class ClipRecord:
    concept_key: str
    gender: str
    relative_path: str
    bytes: int
    sha256: str
    reviewed: bool
    phrase_text: str


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def write_language_manifest(
    language_dir: Path,
    *,
    language_folder: str,
    lang_code: str,
    pack_style: str,
    clips: list[ClipRecord],
    authoring: dict[str, Any],
    template_source: str | None,
    template_commit: str | None,
    voice_provenance: dict[str, Any] | None = None,
) -> Path:
    total_bytes = sum(c.bytes for c in clips)
    man = {
        "schema_version": 1,
        "language_folder": language_folder,
        "lang_code": lang_code,
        "pack_style": pack_style,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "aggregate_bytes": total_bytes,
        "clip_count": len(clips),
        "authoring": authoring,
        "template_source": template_source,
        "template_commit": template_commit,
        "voice_provenance": voice_provenance,
        "clips": [
            {
                "concept_key": c.concept_key,
                "gender": c.gender,
                "path": c.relative_path,
                "bytes": c.bytes,
                "sha256": c.sha256,
                "reviewed": c.reviewed,
                "phrase_text": c.phrase_text,
            }
            for c in clips
        ],
    }
    language_dir.mkdir(parents=True, exist_ok=True)
    out = language_dir / "manifest.json"
    out.write_text(json.dumps(man, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return out