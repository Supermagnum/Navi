"""Synthesize audition clips for up to 3 voice candidates per gender."""

from __future__ import annotations

import json
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

from navi_voicegen.deps import resolve_device
from navi_voicegen.engines.chatterbox_engine import (
    ChatterboxUnavailable,
    supported_language_ids,
    synthesize_wav,
)
from navi_voicegen.languages import SPECS
from navi_voicegen.paths import repo_root
from navi_voicegen.postprocess import encode_clip
from navi_voicegen.voices.config import audition_phrases_for, language_voice_config, load_voices_yaml
from navi_voicegen.voices.fetch import fetch_candidate, processed_ref_path
from navi_voicegen.voices.process import sha256_file


def audition_root() -> Path:
    return repo_root() / "build" / "audition"


@dataclass
class AuditionSummary:
    lang: str
    summary_path: Path
    device: str


def run_audition(
    lang_code: str,
    *,
    genders: tuple[str, ...] = ("male", "female"),
    device: str = "auto",
    fmt: str = "opus",
    seed: int | None = 42,
    max_candidates: int = 3,
) -> AuditionSummary:
    spec = SPECS[lang_code]
    if spec.chatterbox_id is None:
        raise SystemExit(f"Chatterbox unsupported for {lang_code}; cannot audition")

    resolved = resolve_device(device)
    try:
        cb_langs = supported_language_ids()
    except ChatterboxUnavailable as exc:
        raise SystemExit(str(exc)) from exc
    if spec.chatterbox_id not in cb_langs:
        raise SystemExit(f"Chatterbox build missing language_id={spec.chatterbox_id!r}")

    phrases = audition_phrases_for(lang_code)
    lcfg = language_voice_config(lang_code)
    raw = load_voices_yaml()
    lines: list[str] = [
        f"# Audition summary — {lang_code}",
        "",
        f"- generated_at: {datetime.now(timezone.utc).isoformat()}",
        f"- device: {resolved}",
        f"- seed: {seed}",
        "",
        "Listen under `build/audition/` and set `selected` + `selection_reason` in voices.yaml.",
        "",
    ]

    for gender in genders:
        gcfg = lcfg.male if gender == "male" else lcfg.female
        gblob = (raw.get("languages") or {}).get(lang_code, {}).get(gender)
        if gblob and gblob.get("status") == "blocked":
            lines.append(f"## {gender}")
            lines.append(f"- **blocked**: {gblob.get('reason')}")
            lines.append("")
            continue
        if gcfg is None or gcfg.blocked_reason:
            lines.append(f"## {gender}")
            lines.append(f"- **blocked**: {gcfg.blocked_reason if gcfg else 'missing config'}")
            lines.append("")
            continue

        lines.append(f"## {gender}")
        selected = gcfg.selected
        if selected:
            lines.append(f"- current default in voices.yaml: `{selected}`")
            if gcfg.selection_reason:
                lines.append(f"- reason: {gcfg.selection_reason}")
        lines.append("")

        for cid, cand in list(gcfg.candidates.items())[:max_candidates]:
            fetch_candidate(lang_code, gender, cand)
            ref = processed_ref_path(lang_code, gender, cid)
            out_dir = audition_root() / lang_code / gender / cid
            out_dir.mkdir(parents=True, exist_ok=True)
            meta_entries: list[dict] = []
            for idx, text in enumerate(phrases, start=1):
                key = f"phrase_{idx}"
                ext = "opus" if fmt == "opus" else "mp3"
                dest = out_dir / f"{key}.{ext}"
                wav = synthesize_wav(
                    text=text,
                    language_id=spec.chatterbox_id,
                    device=resolved,
                    voice_ref=ref,
                    seed=seed,
                )
                try:
                    encode_clip(wav, dest, fmt)
                finally:
                    wav.unlink(missing_ok=True)
                meta_entries.append(
                    {
                        "phrase": text,
                        "path": dest.relative_to(audition_root()).as_posix(),
                        "bytes": dest.stat().st_size,
                        "sha256": sha256_file(dest),
                    }
                )
            (out_dir / "manifest.json").write_text(
                json.dumps(
                    {
                        "lang_code": lang_code,
                        "gender": gender,
                        "candidate_id": cid,
                        "ref_wav": str(ref),
                        "phrases": meta_entries,
                    },
                    indent=2,
                )
                + "\n",
                encoding="utf-8",
            )
            lines.append(f"### `{cid}` ({cand.speaker_id}, {cand.dataset})")
            lines.append(f"- ref: `{ref}`")
            lines.append(f"- out: `{out_dir.relative_to(repo_root())}`")
            lines.append("")

    audition_root().mkdir(parents=True, exist_ok=True)
    summary = audition_root() / lang_code / "SUMMARY.md"
    summary.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return AuditionSummary(lang_code, summary, resolved)