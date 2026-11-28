"""Machine-check mode: text (+ optional audio) checks for languages without native review."""

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from navi_voicegen.languages import SPECS, TIER1
from navi_voicegen.paths import repo_root
from navi_voicegen.phrases import load_phrase_file


MANEUVER_TOKENS = {
    "left": {"left", "venstre", "gauche", "links", "izquierda", "يسار", "左"},
    "right": {"right", "høyre", "hoyre", "droite", "rechts", "derecha", "يمين", "右"},
    "u_turn": {"u-turn", "u turn", "u-sving", "usving", "demi-tour", "kehren", "دوران"},
    "roundabout": {"roundabout", "rundkjøring", "rond-point", "kreisverkehr", "دوار"},
    "exit": {"exit", "avkjørsel", "sortie", "ausfahrt", "خروج"},
    "merge": {"merge", "flett", "insertion", "einfädeln"},
    "arrive": {"arrive", "ankommet", "arrivé", "angekommen", "وصلت"},
    "straight": {"straight", "rett", "tout droit", "geradeaus"},
}


@dataclass
class MachineRow:
    key: str
    english_meaning: str
    phrase: str
    back_translation: str
    transcript: str
    transcript_bt: str
    status: str
    reasons: list[str]


def _en_meaning(key: str) -> str:
    try:
        en = load_phrase_file("en")
        for e in en.entries:
            if e.concept_key == key:
                return e.text
    except FileNotFoundError:
        pass
    return key.replace("_", " ")


def _script_ok(text: str, lang_code: str) -> tuple[bool, str]:
    if re.search(r"<[^>]+>|\{[^}]+\}|\[DRAFT", text):
        return False, "placeholder left in phrase"
    if lang_code == "ar":
        if not re.search(r"[\u0600-\u06FF]", text):
            return False, "expected Arabic script"
        if re.search(r"[A-Za-z]{3,}", text):
            return False, "mixed Latin script in Arabic phrase"
    if lang_code == "zh" and not re.search(r"[\u4e00-\u9fff]", text):
        return False, "expected Han characters"
    if lang_code == "ja" and not re.search(r"[\u3040-\u30ff\u4e00-\u9fff]", text):
        return False, "expected Japanese script"
    if lang_code == "ru" and not re.search(r"[\u0400-\u04FF]", text):
        return False, "expected Cyrillic"
    if lang_code == "hi" and not re.search(r"[\u0900-\u097F]", text):
        return False, "expected Devanagari"
    return True, ""


def _gender_rule_check(text: str, lang_code: str) -> tuple[str, str]:
    """Return (PASS|WARN, reason). Spec wants masculine/neutral imperatives for Arabic."""
    if lang_code != "ar":
        return "PASS", ""
    # Heuristic: feminine plural/dual imperative markers often end with ي for some verbs — WARN only.
    if re.search(r"ي\s*$", text.strip()):
        return "WARN", "possible feminine imperative ending — verify masculine/neutral (§6)"
    return "PASS", ""


def back_translate(text: str, src_lang: str, cfg: dict[str, Any]) -> tuple[str, list[str]]:
    """Offline MT to English (OPUS-MT). Never writes back into phrase files."""
    notes: list[str] = []
    mc = cfg.get("machine_check") or {}
    model_id = str(mc.get("mt_model") or "Helsinki-NLP/opus-mt-mul-en")
    # NLLB is CC-BY-NC — refuse explicitly if configured
    if "nllb" in model_id.lower():
        return "", ["NLLB is CC-BY-NC-4.0 — not used"]
    try:
        from transformers import AutoModelForSeq2SeqLM, AutoTokenizer

        tok = AutoTokenizer.from_pretrained(model_id)
        model = AutoModelForSeq2SeqLM.from_pretrained(model_id)
        batch = tok(text, return_tensors="pt", truncation=True)
        gen = model.generate(**batch, max_new_tokens=128)
        out = tok.decode(gen[0], skip_special_tokens=True)
        return out.strip(), notes
    except Exception as exc:  # noqa: BLE001
        # Deterministic offline stub for tests / missing models: identity-ish echo
        notes.append(f"MT unavailable ({exc}); using heuristic gloss")
        return _heuristic_gloss(text, src_lang), notes


def _heuristic_gloss(text: str, src_lang: str) -> str:
    # Enough for maneuver token checks in unit tests without downloading models.
    return text


def _maneuver_check(key: str, back_en: str, english_src: str) -> tuple[str, str]:
    back_l = back_en.casefold()
    src_l = english_src.casefold()
    for mane, tokens in MANEUVER_TOKENS.items():
        if mane in key or any(t in src_l for t in tokens if t.isascii()):
            # require at least one token of that maneuver family in back-translation
            if mane in ("left", "right") and mane in key:
                if mane == "left" and not any(t in back_l for t in ("left", "venstre", "gauche")):
                    # also accept if English src word survives
                    if "left" not in back_l:
                        return "FAIL", f"maneuver left missing in back-translation {back_en!r}"
                if mane == "right" and "right" not in back_l and "høyre" not in back_l:
                    return "FAIL", f"maneuver right missing in back-translation {back_en!r}"
            # swapped left/right detection
            if key == "left" and "right" in back_l and "left" not in back_l:
                return "FAIL", "left/right swapped in back-translation"
            if key == "right" and "left" in back_l and "right" not in back_l:
                return "FAIL", "left/right swapped in back-translation"
    # distance+unit for meter keys
    if key in ("meters", "kilometers"):
        if key == "meters" and "meter" not in back_l and "metre" not in back_l:
            return "WARN", "unit meter not obvious in back-translation"
        if key == "kilometers" and "kilo" not in back_l and "km" not in back_l:
            return "WARN", "unit kilometer not obvious in back-translation"
    return "PASS", ""


def run_machine_check(
    lang_code: str,
    *,
    cfg: dict[str, Any] | None = None,
    with_audio: bool = True,
) -> Path:
    from navi_voicegen.qc.config import load_qc_config
    from navi_voicegen.qc.runner import evaluate_clip
    from navi_voicegen.packs import find_clip_for_key
    from navi_voicegen.paths import default_out_dir

    cfg = cfg or load_qc_config()
    pf = load_phrase_file(lang_code)
    spec = SPECS[lang_code]
    rows: list[MachineRow] = []
    out_root = default_out_dir()

    for entry in pf.entries:
        meaning = _en_meaning(entry.concept_key)
        bt, mt_notes = back_translate(entry.text, lang_code, cfg)
        reasons = list(mt_notes)
        status = "PASS"

        ok_script, script_reason = _script_ok(entry.text, lang_code)
        if not ok_script:
            status = "FAIL"
            reasons.append(script_reason)

        # template drift: if valhalla_original present, proposed may differ — note only
        if entry.valhalla_original and entry.valhalla_original.strip() != entry.text.strip():
            reasons.append(
                f"differs from template original {entry.valhalla_original!r} (intentional?)"
            )
            if status == "PASS":
                status = "WARN"

        g_status, g_reason = _gender_rule_check(entry.text, lang_code)
        if g_status == "WARN":
            reasons.append(g_reason)
            if status == "PASS":
                status = "WARN"

        m_status, m_reason = _maneuver_check(entry.concept_key, bt, meaning)
        if m_status == "FAIL":
            status = "FAIL"
            reasons.append(m_reason)
        elif m_status == "WARN" and status == "PASS":
            status = "WARN"
            reasons.append(m_reason)

        transcript = ""
        transcript_bt = ""
        if with_audio:
            for gender in ("male", "female"):
                for pack in (out_root / "unreviewed" / spec.folder, out_root / spec.folder):
                    clip = find_clip_for_key(pack, gender, entry.concept_key, "opus")
                    if clip is None:
                        clip = find_clip_for_key(pack, gender, entry.concept_key, "mp3")
                    if clip is None:
                        continue
                    qc = evaluate_clip(
                        lang_code=lang_code,
                        concept_key=entry.concept_key,
                        expected=entry.text,
                        audio=clip,
                        gender=gender,
                        cfg=cfg,
                    )
                    transcript = qc.get("transcript") or ""
                    transcript_bt, _ = back_translate(transcript, lang_code, cfg)
                    if qc.get("status") == "FAIL":
                        status = "FAIL"
                        reasons.extend(qc.get("reasons") or [])
                    elif qc.get("status") == "WARN" and status == "PASS":
                        status = "WARN"
                        reasons.extend(qc.get("reasons") or [])
                    break
                if transcript:
                    break

        rows.append(
            MachineRow(
                key=entry.concept_key,
                english_meaning=meaning,
                phrase=entry.text,
                back_translation=bt,
                transcript=transcript,
                transcript_bt=transcript_bt,
                status=status,
                reasons=reasons,
            )
        )

    # never set reviewed from machine-check; optional review_status field write is separate
    out_dir = repo_root() / "build" / "review" / spec.folder
    out_dir.mkdir(parents=True, exist_ok=True)
    md = out_dir / "machine_check.md"
    order = {"FAIL": 0, "WARN": 1, "PASS": 2}
    rows.sort(key=lambda r: (order.get(r.status, 9), r.key))
    lines = [
        f"# Machine-check — {spec.folder} (`{lang_code}`)",
        "",
        "> This is **not** a native review. `machine_checked` is never treated as `reviewed`.",
        "> Back-translations are for the operator’s understanding only and must not edit phrase wording.",
        "",
        "| key | English meaning | phrase | back-translation | transcript | transcript BT | status | reason |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for r in rows:
        reason = "; ".join(r.reasons) or "—"
        lines.append(
            f"| `{r.key}` | {r.english_meaning} | {r.phrase} | {r.back_translation} | "
            f"{r.transcript or '—'} | {r.transcript_bt or '—'} | {r.status} | {reason} |"
        )
    md.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return md


def tier1_codes_with_phrases_or_templates() -> list[str]:
    out = []
    for code in TIER1:
        phrase = repo_root() / "tools" / "audio-generate" / "phrases" / f"{code}.yaml"
        if phrase.is_file() or SPECS[code].template_source:
            out.append(code)
    return out
