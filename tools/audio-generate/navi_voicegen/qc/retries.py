"""QC auto-retry regeneration for FAIL clips (en/nb allowlist only)."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from navi_voicegen.qc.config import assert_generate_allowed, load_qc_config
from navi_voicegen.qc.runner import evaluate_clip, run_qc_pack
from navi_voicegen.languages import SPECS
from navi_voicegen.phrases import load_phrase_file


def qc_with_retries(
    lang_code: str,
    *,
    genders: list[str],
    out_root: Path,
    retries: int,
    seed: int | None,
    device: str,
    fmt: str,
    voice_ref_male: Path | None,
    voice_ref_female: Path | None,
) -> Path:
    assert_generate_allowed(lang_code, action="qc-retries")
    report = run_qc_pack(lang_code, genders=genders, out_root=out_root)
    if retries <= 0:
        return report

    cfg = load_qc_config()
    spec = SPECS[lang_code]
    phrases = {e.concept_key: e.text for e in load_phrase_file(lang_code).entries}

    # Load FAIL clips from gender reports
    qc_dir = out_root.parent / "qc" / spec.folder if False else None
    from navi_voicegen.paths import repo_root
    from navi_voicegen.qc.report import qc_root

    root = qc_root(repo_root(), spec.folder)
    fails: list[dict[str, Any]] = []
    for gender in genders:
        path = root / gender / "report.json"
        if not path.is_file():
            continue
        data = json.loads(path.read_text(encoding="utf-8"))
        for clip in data.get("clips") or []:
            if clip.get("status") == "FAIL":
                fails.append(clip)

    if not fails:
        return report

    from navi_voicegen.engines.chatterbox_engine import synthesize_wav
    from navi_voicegen.languages import SPECS as _S
    from navi_voicegen.postprocess import encode_clip
    from navi_voicegen.voices.resolve import resolve_voice_ref_path

    chatterbox_lang = _S[lang_code].chatterbox_id or lang_code
    for clip in fails:
        key = clip["concept_key"]
        gender = clip["gender"]
        expected = phrases.get(key) or clip.get("expected_text") or key
        dest = Path(repo_root()) / clip["path"]
        attempts = list(clip.get("attempts") or [])
        best = clip
        for attempt in range(1, retries + 1):
            attempt_seed = (seed or 0) + 1000 * attempt + (17 if gender == "female" else 0)
            ref, _ = resolve_voice_ref_path(
                lang_code,
                gender,
                override=voice_ref_male if gender == "male" else voice_ref_female,
            )
            wav = synthesize_wav(
                text=expected,
                language_id=chatterbox_lang,
                device=device,
                voice_ref=ref,
                seed=attempt_seed,
            )
            encode_clip(wav, dest, fmt)
            wav.unlink(missing_ok=True)
            row = evaluate_clip(
                lang_code=lang_code,
                concept_key=key,
                expected=expected,
                audio=dest,
                gender=gender,
                cfg=cfg,
            )
            row["attempt"] = attempt
            attempts.append({"attempt": attempt, "status": row["status"], "wer": row["wer"], "reasons": row["reasons"]})
            if row["status"] == "PASS":
                best = row
                break
            if row["status"] == "WARN" and best.get("status") == "FAIL":
                best = row
            elif best.get("status") == "FAIL" and (row.get("wer") or 1) < (best.get("wer") or 1):
                best = row
        best["attempts"] = attempts
        # rewrite single clip entry in gender report
        path = root / gender / "report.json"
        data = json.loads(path.read_text(encoding="utf-8"))
        for i, c in enumerate(data.get("clips") or []):
            if c.get("concept_key") == key and c.get("gender") == gender:
                data["clips"][i] = best
                break
        path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    # Refresh summary markdown
    return run_qc_pack(lang_code, genders=genders, out_root=out_root)
