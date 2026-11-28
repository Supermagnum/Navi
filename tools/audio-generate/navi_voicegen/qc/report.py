"""Write QC JSON/Markdown reports under build/qc/."""

from __future__ import annotations

import json
from collections import Counter
from pathlib import Path
from typing import Any


def qc_root(repo_root: Path, folder: str) -> Path:
    return repo_root / "build" / "qc" / folder


def write_gender_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def write_summary_md(path: Path, *, folder: str, clips: list[dict[str, Any]]) -> None:
    counts = Counter(c.get("status", "PASS") for c in clips)
    lines = [
        f"# QC report — {folder}",
        "",
        f"PASS: **{counts.get('PASS', 0)}** · WARN: **{counts.get('WARN', 0)}** · FAIL: **{counts.get('FAIL', 0)}**",
        "",
    ]

    def rank(c: dict[str, Any]) -> tuple[int, float]:
        order = {"FAIL": 0, "WARN": 1, "PASS": 2}.get(str(c.get("status")), 3)
        wer = float(c.get("wer") or 0)
        return order, -wer

    worst = sorted(clips, key=rank)
    for label in ("FAIL", "WARN"):
        subset = [c for c in worst if c.get("status") == label]
        lines.append(f"## {label}")
        lines.append("")
        if not subset:
            lines.append("(none)")
            lines.append("")
            continue
        lines.append("| key | gender | status | reason | clip |")
        lines.append("|---|---|---|---|---|")
        for c in subset:
            reason = "; ".join(c.get("reasons") or []) or "—"
            rel = c.get("path") or ""
            lines.append(
                f"| `{c.get('concept_key')}` | {c.get('gender')} | {c.get('status')} | {reason} | [{rel}]({rel}) |"
            )
        lines.append("")

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
