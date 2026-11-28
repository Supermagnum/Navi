"""Run QC over packs, references, and audition trees."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any, Iterable

from navi_voicegen.languages import SPECS
from navi_voicegen.paths import default_out_dir, repo_root
from navi_voicegen.phrases import load_phrase_file
from navi_voicegen.qc.align import align_clip
from navi_voicegen.qc.asr import transcribe_clip
from navi_voicegen.qc.config import assert_generate_allowed, load_qc_config, thresholds_for
from navi_voicegen.qc.ffmpeg_metrics import measure
from navi_voicegen.qc.normalize import normalize_text, syllable_estimate, wer_cer
from navi_voicegen.qc.quality import score_quality
from navi_voicegen.qc.report import qc_root, write_gender_json, write_summary_md
from navi_voicegen.qc.score import decide_status


def _phrase_map(lang_code: str) -> dict[str, str]:
    try:
        pf = load_phrase_file(lang_code)
    except FileNotFoundError:
        return {}
    return {e.concept_key: e.text for e in pf.entries}


def _iter_pack_clips(pack_dir: Path, genders: list[str]) -> Iterable[tuple[str, str, Path]]:
    for gender in genders:
        gdir = pack_dir / gender
        if not gdir.is_dir():
            continue
        for path in sorted(gdir.glob("*.opus")) + sorted(gdir.glob("*.mp3")):
            yield gender, path.stem, path


def evaluate_clip(
    *,
    lang_code: str,
    concept_key: str,
    expected: str,
    audio: Path,
    gender: str,
    cfg: dict[str, Any],
    assembled_keys: list[str] | None = None,
) -> dict[str, Any]:
    thr = thresholds_for(lang_code, cfg)
    asr = transcribe_clip(audio, lang_code, cfg)
    wer, cer, diff = wer_cer(expected, asr.text, lang_code)
    align = align_clip(
        audio,
        expected,
        lang_code,
        thr,
        cfg,
        assembled_keys=assembled_keys,
    )
    ff = measure(audio)
    qual = score_quality(audio)
    single_token = " " not in expected.strip() and syllable_estimate(expected, lang_code) <= 2
    status, reasons = decide_status(
        lang_code=lang_code,
        expected=expected,
        transcript=asr.text,
        wer=wer,
        cer=cer,
        word_diff=diff,
        align=align.to_dict(),
        ffmpeg=ff.to_dict(),
        quality=qual.to_dict(),
        thr=thr,
        single_token=single_token,
    )
    try:
        rel = audio.relative_to(repo_root()).as_posix()
    except ValueError:
        rel = audio.as_posix()
    return {
        "concept_key": concept_key,
        "gender": gender,
        "path": rel,
        "expected_text": expected,
        "transcript": asr.text,
        "asr_backend": asr.backend,
        "asr_model": asr.model,
        "asr_notes": asr.notes,
        "detected_language": asr.language,
        "normalized_expected": normalize_text(expected, lang_code),
        "normalized_transcript": normalize_text(asr.text, lang_code),
        "wer": wer,
        "cer": cer,
        "word_diff": diff,
        "align": align.to_dict(),
        "ffmpeg": ff.to_dict(),
        "quality": qual.to_dict(),
        "status": status,
        "reasons": reasons,
        "attempts": [],
    }


def run_qc_pack(
    lang_code: str,
    *,
    genders: list[str] | None = None,
    out_root: Path | None = None,
    prefer_unreviewed: bool = True,
) -> Path:
    cfg = load_qc_config()
    spec = SPECS[lang_code]
    genders = genders or ["male", "female"]
    root = out_root or default_out_dir()
    candidates = []
    if prefer_unreviewed:
        candidates.append(root / "unreviewed" / spec.folder)
    candidates.append(root / spec.folder)
    pack_dir = next((p for p in candidates if p.is_dir()), None)
    if pack_dir is None:
        raise SystemExit(f"No pack directory for {lang_code} under {root}")

    phrases = _phrase_map(lang_code)
    clips: list[dict[str, Any]] = []
    by_gender: dict[str, list[dict[str, Any]]] = {g: [] for g in genders}
    for gender, key, path in _iter_pack_clips(pack_dir, genders):
        expected = phrases.get(key) or key
        row = evaluate_clip(
            lang_code=lang_code,
            concept_key=key,
            expected=expected,
            audio=path,
            gender=gender,
            cfg=cfg,
        )
        clips.append(row)
        by_gender.setdefault(gender, []).append(row)

    out = qc_root(repo_root(), spec.folder)
    for gender, rows in by_gender.items():
        write_gender_json(
            out / gender / "report.json",
            {"lang_code": lang_code, "folder": spec.folder, "gender": gender, "clips": rows},
        )
    write_summary_md(out / "report.md", folder=spec.folder, clips=clips)
    _patch_manifest_qc(pack_dir, clips)
    return out / "report.md"


def _patch_manifest_qc(pack_dir: Path, clips: list[dict[str, Any]]) -> None:
    man = pack_dir / "manifest.json"
    if not man.is_file():
        return
    data = json.loads(man.read_text(encoding="utf-8"))
    by_key = {(c["gender"], c["concept_key"]): c for c in clips}
    for entry in data.get("clips") or []:
        hit = by_key.get((entry.get("gender"), entry.get("concept_key")))
        if not hit:
            continue
        entry["qc"] = {
            "status": hit["status"],
            "wer": hit["wer"],
            "cer": hit["cer"],
            "reasons": hit["reasons"][:5],
        }
    # review_status from phrases if present
    lang = data.get("lang_code")
    if lang:
        try:
            pf = load_phrase_file(lang)
            status = "reviewed" if pf.reviewed_file else "unreviewed"
            data["review_status"] = status
        except Exception:
            pass
    man.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def run_qc_references(lang_code: str | None = None) -> list[Path]:
    from navi_voicegen.voices.config import language_voice_config
    from navi_voicegen.voices.resolve import resolve_voice_ref_path

    cfg = load_qc_config()
    codes = [lang_code] if lang_code else list(SPECS.keys())
    reports: list[Path] = []
    for code in codes:
        if code not in SPECS:
            continue
        rows: list[dict[str, Any]] = []
        for gender in ("male", "female"):
            try:
                ref, _prov = resolve_voice_ref_path(code, gender)
            except Exception:
                ref = None
            if ref is None or not ref.is_file():
                continue
            # Refs have no expected phrase — score ffmpeg/quality/VAD only.
            from navi_voicegen.qc.ffmpeg_metrics import measure
            from navi_voicegen.qc.quality import score_quality
            from navi_voicegen.qc.vad import speech_regions
            from navi_voicegen.qc.config import thresholds_for

            thr = thresholds_for(code if code in ("en", "nb") else "en", cfg)
            ff = measure(ref)
            qual = score_quality(ref)
            regions = speech_regions(ref)
            reasons: list[str] = []
            status = "PASS"
            nf = ff.noise_floor_dbfs
            if nf is not None and nf > float(thr.get("noise_floor_dbfs_fail", -35)):
                status = "FAIL"
                reasons.append(f"noise floor {nf:.1f} dBFS")
            elif nf is not None and nf > float(thr.get("noise_floor_dbfs_warn", -45)):
                status = "WARN"
                reasons.append(f"noise floor {nf:.1f} dBFS")
            if (ff.clipped_samples or 0) > 0:
                status = "FAIL"
                reasons.append(f"clipped samples {ff.clipped_samples}")
            if not regions:
                status = "FAIL" if status != "PASS" else "WARN"
                reasons.append("no VAD speech regions detected")
            if status == "FAIL":
                reasons.append("reference FAIL — do not use for generation")
            try:
                rel = ref.relative_to(repo_root()).as_posix()
            except ValueError:
                rel = ref.as_posix()
            row = {
                "concept_key": f"ref_{gender}",
                "gender": gender,
                "path": rel,
                "ffmpeg": ff.to_dict(),
                "quality": qual.to_dict(),
                "vad_regions": [{"start_s": r.start_s, "end_s": r.end_s} for r in regions],
                "status": status,
                "reasons": reasons or ["ok"],
            }
            rows.append(row)
        if not rows:
            continue
        folder = SPECS[code].folder
        out = qc_root(repo_root(), folder) / "references"
        write_gender_json(out / "report.json", {"lang_code": code, "clips": rows})
        md = out / "report.md"
        write_summary_md(md, folder=f"{folder}/references", clips=rows)
        reports.append(md)
    return reports


def run_qc_audition(lang_code: str) -> Path:
    """QC assembled/ and whole/ trees under build/audition/<folder>/."""
    assert_generate_allowed(lang_code, action="qc-audition synthesis path") if False else None
    # Audition QC does not synthesise; allow any lang that has audition audio.
    spec = SPECS[lang_code]
    root = repo_root() / "build" / "audition" / spec.folder
    if not root.is_dir():
        raise SystemExit(f"No audition directory: {root}")
    cfg = load_qc_config()
    phrases = _phrase_map(lang_code)
    clips: list[dict[str, Any]] = []
    for kind in ("assembled", "whole"):
        for gender_dir in sorted((root / kind).glob("*")) if (root / kind).exists() else []:
            if not gender_dir.is_dir():
                # files may be directly under kind/
                continue
        # files can be kind/*.opus or kind/<gender>/*.opus
        paths = list((root / kind).rglob("*.opus")) + list((root / kind).rglob("*.mp3"))
        for path in paths:
            key = path.stem
            expected = phrases.get(key) or key.replace("_", " ")
            assembled_keys = key.split("__") if "__" in key else None
            row = evaluate_clip(
                lang_code=lang_code,
                concept_key=f"{kind}/{key}",
                expected=expected,
                audio=path,
                gender=path.parent.name if path.parent.name in ("male", "female") else "mixed",
                cfg=cfg,
                assembled_keys=assembled_keys,
            )
            row["audition_kind"] = kind
            clips.append(row)
    out = qc_root(repo_root(), spec.folder) / "audition"
    write_gender_json(out / "report.json", {"lang_code": lang_code, "clips": clips})
    md = out / "report.md"
    write_summary_md(md, folder=f"{spec.folder}/audition", clips=clips)
    return md
