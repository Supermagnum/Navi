"""Native review sheet export / apply for phrase YAML."""

from __future__ import annotations

import csv
from dataclasses import replace
from pathlib import Path

from navi_voicegen.languages import SPECS
from navi_voicegen.packs import find_clip_for_key
from navi_voicegen.paths import default_out_dir, repo_root
from navi_voicegen.phrases import PhraseEntry, load_phrase_file, phrases_dir, write_phrase_file
from navi_voicegen.review_gate import assert_sheet_matches_record, write_review_record


def review_dir_for(lang_code: str) -> Path:
    return repo_root() / "build" / "review" / SPECS[lang_code].folder


def _clip_link(lang_code: str, gender: str, concept_key: str) -> str:
    spec = SPECS[lang_code]
    ext = "opus"
    root = default_out_dir()
    for pack_root in (root / "unreviewed" / spec.folder, root / spec.folder):
        p = find_clip_for_key(pack_root, gender, concept_key, ext)
        if p is not None:
            try:
                return p.relative_to(repo_root()).as_posix()
            except ValueError:
                return p.as_posix()
    return ""


def write_review_sheet(lang_code: str) -> Path:
    pf = load_phrase_file(lang_code)
    out_dir = review_dir_for(lang_code)
    out_dir.mkdir(parents=True, exist_ok=True)
    tsv = out_dir / "review-sheet.tsv"
    with tsv.open("w", encoding="utf-8", newline="") as f:
        w = csv.writer(f, delimiter="\t")
        w.writerow(["concept_key", "text", "reviewed", "notes"])
        for e in pf.entries:
            w.writerow([e.concept_key, e.text, "yes" if e.reviewed else "no", ""])

    csv_path = out_dir / "review.csv"
    with csv_path.open("w", encoding="utf-8", newline="") as f:
        w = csv.writer(f)
        w.writerow(
            [
                "key",
                "valhalla_original",
                "proposed",
                "alternatives",
                "audio_male",
                "audio_female",
                "ok_change",
                "my_text",
            ]
        )
        for e in pf.entries:
            orig = e.valhalla_original or e.text
            alts = "; ".join(e.alternatives or [])
            w.writerow(
                [
                    e.concept_key,
                    orig,
                    e.text,
                    alts,
                    _clip_link(lang_code, "male", e.concept_key),
                    _clip_link(lang_code, "female", e.concept_key),
                    "",
                    "",
                ]
            )

    md_lines = [
        f"# {SPECS[lang_code].folder} phrase review",
        "",
        "Edit `review.csv` or `review-sheet.tsv`, then apply with `--apply-review`.",
        "Only `--apply-review` may set `reviewed: true` in phrases YAML.",
        "",
        "| key | Valhalla original | proposed | alternatives |",
        "|---|---|---|---|",
    ]
    for e in pf.entries:
        orig = e.valhalla_original or e.text
        alts = ", ".join(e.alternatives or []) or "—"
        reason = f" ({e.change_reason})" if e.change_reason else ""
        md_lines.append(f"| `{e.concept_key}` | {orig} | {e.text}{reason} | {alts} |")

    md_lines.extend(
        [
            "",
            "## Assembled sentence audition",
            "",
            "See `build/audition/norwegian/INDEX.md` for fragment-concat vs whole-phrase pairs.",
            "",
        ]
    )
    md_path = out_dir / "review.md"
    md_path.write_text("\n".join(md_lines) + "\n", encoding="utf-8")

    readme = out_dir / "README.txt"
    readme.write_text(
        "Edit review-sheet.tsv (set reviewed=yes and fix text), then:\n"
        f"  navi-voicegen --apply-review {tsv}\n\n"
        "Rich columns: review.csv / review.md\n",
        encoding="utf-8",
    )
    return tsv


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

    assert_sheet_matches_record(lang_guess, sheet_path)

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
    new_pf = replace(
        pf,
        entries=updated,
        reviewed_file=reviewed_all,
        review_status="reviewed" if reviewed_all else pf.review_status,
    )
    # machine_checked is never promoted by apply-review unless every row is marked reviewed
    if not reviewed_all and new_pf.review_status == "reviewed":
        new_pf = replace(new_pf, review_status="unreviewed")
    out = phrases_dir() / f"{lang_guess}.yaml"
    write_phrase_file(out, new_pf)
    write_review_record(lang_guess, sheet_path=sheet_path)
    return out