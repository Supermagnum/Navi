"""Load phrase YAML inventories."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml


@dataclass
class PhraseEntry:
    concept_key: str
    text: str
    reviewed: bool
    pack_style: str | None = None
    valhalla_original: str | None = None
    change_reason: str | None = None
    alternatives: list[str] | None = None


VALID_REVIEW_STATUS = frozenset({"unreviewed", "machine_checked", "reviewed"})


@dataclass
class PhraseFile:
    lang_code: str
    language_folder: str
    reviewed_file: bool
    template_source: str | None
    template_commit: str | None
    entries: list[PhraseEntry]
    pack_style: str | None = None  # TBD until native review picks fragment / hybrid / whole
    review_status: str = "unreviewed"

    def unreviewed_entries(self) -> list[PhraseEntry]:
        return [e for e in self.entries if not e.reviewed]

    @property
    def is_natively_reviewed(self) -> bool:
        """Only native --apply-review counts; machine_checked never does."""
        return self.review_status == "reviewed" and self.reviewed_file


def phrases_dir() -> Path:
    return Path(__file__).resolve().parents[1] / "phrases"


def load_phrase_file(lang_code: str) -> PhraseFile:
    path = phrases_dir() / f"{lang_code}.yaml"
    if not path.is_file():
        raise FileNotFoundError(f"No phrase file: {path}")
    raw: dict[str, Any] = yaml.safe_load(path.read_text(encoding="utf-8"))
    entries: list[PhraseEntry] = []
    for concept_key, body in raw.get("phrases", {}).items():
        if not isinstance(body, dict):
            raise ValueError(f"{path}: phrase {concept_key!r} must be a mapping")
        raw_text = body.get("text")
        if raw_text is None:
            raise ValueError(
                f"{path}: phrase {concept_key!r} has null text — "
                f'quote YAML strings like text: "null" when the spoken word is null'
            )
        text = str(raw_text).strip()
        if not text:
            raise ValueError(f"{path}: phrase {concept_key!r} has empty text")
        reviewed = bool(body.get("reviewed", raw.get("reviewed", False)))
        pack_style = body.get("pack_style")
        alts = body.get("alternatives")
        if alts is not None and not isinstance(alts, list):
            alts = [str(alts)]
        entries.append(
            PhraseEntry(
                concept_key=concept_key,
                text=text,
                reviewed=reviewed,
                pack_style=pack_style,
                valhalla_original=body.get("valhalla_original"),
                change_reason=body.get("change_reason"),
                alternatives=alts,
            )
        )
    reviewed_file = bool(raw.get("reviewed", False))
    review_status = str(raw.get("review_status") or ("reviewed" if reviewed_file else "unreviewed"))
    if review_status not in VALID_REVIEW_STATUS:
        raise ValueError(f"{path}: invalid review_status {review_status!r}")
    # machine_checked must never imply reviewed:true
    if review_status == "machine_checked":
        reviewed_file = False
    if reviewed_file and review_status != "reviewed":
        review_status = "reviewed"

    pf = PhraseFile(
        lang_code=str(raw.get("lang_code", lang_code)),
        language_folder=str(raw["language_folder"]),
        reviewed_file=reviewed_file,
        template_source=raw.get("template_source"),
        template_commit=raw.get("template_commit"),
        entries=entries,
        pack_style=raw.get("pack_style"),
        review_status=review_status,
    )
    from navi_voicegen.review_gate import assert_review_gate

    assert_review_gate(pf.lang_code, pf.reviewed_file)
    return pf


def write_phrase_file(path: Path, phrase_file: PhraseFile) -> None:
    status = phrase_file.review_status
    if phrase_file.reviewed_file:
        status = "reviewed"
    elif status == "reviewed":
        status = "unreviewed"
    data: dict[str, Any] = {
        "lang_code": phrase_file.lang_code,
        "language_folder": phrase_file.language_folder,
        "reviewed": phrase_file.reviewed_file and status == "reviewed",
        "review_status": status,
        "template_source": phrase_file.template_source,
        "template_commit": phrase_file.template_commit,
        "phrases": {},
    }
    if phrase_file.pack_style is not None:
        data["pack_style"] = phrase_file.pack_style
    for e in phrase_file.entries:
        row: dict[str, Any] = {"text": e.text, "reviewed": e.reviewed}
        if e.pack_style:
            row["pack_style"] = e.pack_style
        if e.valhalla_original:
            row["valhalla_original"] = e.valhalla_original
        if e.change_reason:
            row["change_reason"] = e.change_reason
        if e.alternatives:
            row["alternatives"] = e.alternatives
        data["phrases"][e.concept_key] = row
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(yaml.safe_dump(data, sort_keys=False, allow_unicode=True), encoding="utf-8")