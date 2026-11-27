"""Generation state for skip/--force hashing."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any


def phrase_inputs_hash(
    *,
    lang_code: str,
    gender: str,
    engine_id: str,
    seed: int | None,
    entries: list[tuple[str, str, bool]],
    suffix: str | None,
) -> str:
    blob = json.dumps(
        {
            "lang": lang_code,
            "gender": gender,
            "engine": engine_id,
            "seed": seed,
            "suffix": suffix or "",
            "entries": entries,
        },
        sort_keys=True,
    )
    return hashlib.sha256(blob.encode("utf-8")).hexdigest()


def load_state(path: Path) -> dict[str, Any]:
    if not path.is_file():
        return {}
    return json.loads(path.read_text(encoding="utf-8"))


def save_state(path: Path, state: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(state, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def should_skip(
    state: dict[str, Any],
    key: str,
    content_hash: str,
    clip_path: Path,
    *,
    force: bool,
) -> bool:
    if force:
        return False
    if not clip_path.is_file():
        return False
    prev = state.get("clips", {}).get(key)
    if not isinstance(prev, dict):
        return False
    return prev.get("hash") == content_hash