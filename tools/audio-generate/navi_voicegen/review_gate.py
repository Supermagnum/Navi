"""Review gate: reviewed YAML must have a matching review-record file."""

from __future__ import annotations

import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from navi_voicegen.paths import tool_root

RECORD_DIR = tool_root() / "phrases" / "review_records"
HAND_AUTHORED = "hand_authored"


def review_record_path(lang_code: str) -> Path:
    return RECORD_DIR / f"{lang_code}.json"


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def load_review_record(lang_code: str) -> dict[str, Any] | None:
    path = review_record_path(lang_code)
    if not path.is_file():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def write_review_record(
    lang_code: str,
    *,
    sheet_path: Path,
    applied_at: datetime | None = None,
) -> Path:
    RECORD_DIR.mkdir(parents=True, exist_ok=True)
    applied = applied_at or datetime.now(timezone.utc)
    record = {
        "lang_code": lang_code,
        "sheet_path": sheet_path.as_posix(),
        "sheet_sha256": sha256_file(sheet_path),
        "applied_at": applied.isoformat(),
        "source": "apply_review",
    }
    out = review_record_path(lang_code)
    out.write_text(json.dumps(record, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return out


def assert_review_gate(lang_code: str, reviewed_file: bool) -> None:
    """Raise SystemExit when reviewed_file is true without a valid review record."""
    if not reviewed_file:
        return
    rec = load_review_record(lang_code)
    if rec is None:
        raise SystemExit(
            f"{lang_code}: phrases YAML has reviewed:true but no review record at "
            f"{review_record_path(lang_code)} — only --apply-review may set reviewed:true "
            "(voice-guidance.md §7)."
        )
    if rec.get("source") == HAND_AUTHORED:
        return
    sheet_sha = rec.get("sheet_sha256")
    if not sheet_sha:
        raise SystemExit(f"{lang_code}: review record missing sheet_sha256")


def assert_sheet_matches_record(lang_code: str, sheet_path: Path) -> None:
    rec = load_review_record(lang_code)
    if rec is None or rec.get("source") == HAND_AUTHORED:
        return
    expected = rec.get("sheet_sha256")
    actual = sha256_file(sheet_path)
    if expected != actual:
        raise SystemExit(
            f"{lang_code}: review sheet SHA-256 mismatch (record {expected!r}, file {actual!r})"
        )