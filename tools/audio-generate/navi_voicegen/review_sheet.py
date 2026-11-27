"""Native review sheet export / apply for phrase YAML."""

from __future__ import annotations

import csv
from dataclasses import replace
from pathlib import Path

from navi_voicegen.languages import SPECS
from navi_voicegen.paths import repo_root
from navi_voicegen.phrases import PhraseEntry, load_phrase_file, write_phrase_file


def review_dir_for(lang_code: str) -> Path:
    return repo_root() / "build" / "review" / SPECS[lang_code].folder


def write_review_sheet(lang_code: str) -> Path:
    pf = load_phrase_file(lang_code)
    out_dir = review_dir_for(lang_code)
    out_dir.mkdir(parents=True, exist_ok=True)
    out = out_dir / "review-sheet.tsv"
    with out.open("w", encoding="utf-8", newline="") as f:
        w = csv.writer(f, delimiter="\t")
        w.writerow(["concept_key", "text", "reviewed", "notes"])
        for e in pf.entries:
            w.writerow([e.concept_key, e.text, "yes" if e.reviewed else "no", ""])
    readme = out_dir / "README.txt"
    readme.write_text(
        "Edit review-sheet.tsv (set reviewed=yes and fix text), then:\n"
        f"  navi-voicegen --apply-review {out}\n",
        encoding="utf-8",
    )
    return out


def apply_review_sheet(sheet_path: Path) -> Path:
    if not sheet_path.is_file():
        raise SystemExit(f"Review sheet not found: {sheet_path}")
    lang_guess = None
    parent = sheet_path.parent.name
    for code, spec in SPECS.items():
        if spec.folder == parent:
            lang_guess = code
            break
    if lang_guess is None:
        raise SystemExit(
            f"Cannot infer language from path {sheet_path}; expected …/build/review/<language-folder>/review-sheet.tsv"
        )

    pf = load_phrase_file(lang_guess)
    by_key = {e.concept_key: e for e in pf.entries}
    updated: list[PhraseEntry] = []

    with sheet_path.open(encoding="utf-8", newline="") as f:
        reader = csv.DictReader(f, delimiter="\t")
        for row in reader:
            key = (row.get("concept_key") or "").strip()
            if not key:
                continue
            prev = by_key.get(key)
            if prev is None:
                raise SystemExit(f"Unknown concept_key {key!r} in review sheet")
            text = (row.get("text") or prev.text).strip()
            reviewed_raw = (row.get("reviewed") or "").strip().lower()
            reviewed = reviewed_raw in ("yes", "true", "1", "y")
            updated.append(replace(prev, text=text, reviewed=reviewed))
            del by_key[key]

    if by_key:
        for e in by_key.values():
            updated.append(e)

    reviewed_all = all(e.reviewed for e in updated)
    new_pf = replace(pf, entries=updated, reviewed_file=reviewed_all)
    out = Path(__file__).resolve().parents[1] / "phrases" / f"{lang_guess}.yaml"
    write_phrase_file(out, new_pf)
    return out