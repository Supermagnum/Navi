"""Combine QC signals into PASS / WARN / FAIL."""

from __future__ import annotations

from typing import Any

from navi_voicegen.qc.normalize import normalize_text, syllable_estimate


def decide_status(
    *,
    lang_code: str,
    expected: str,
    transcript: str,
    wer: float,
    cer: float,
    word_diff: dict,
    align: dict[str, Any],
    ffmpeg: dict[str, Any],
    quality: dict[str, Any],
    thr: dict[str, Any],
    single_token: bool,
) -> tuple[str, list[str]]:
    """Return (status, reasons)."""
    reasons: list[str] = []
    fail = False
    warn = False

    cer_primary = lang_code in (thr.get("cer_primary_langs") or [])
    primary = cer if cer_primary else wer
    primary_fail = float(thr.get("cer_fail" if cer_primary else "wer_fail", 0.35))
    primary_warn = float(thr.get("cer_warn" if cer_primary else "wer_warn", 0.15))

    norm_eq = normalize_text(expected, lang_code) == normalize_text(transcript, lang_code)
    syl = syllable_estimate(expected, lang_code)
    short = syl <= int(thr.get("short_clip_max_syllables", 2))

    timing_fail = False
    trail = align.get("trailing_bleed_ms")
    lead = align.get("leading_bleed_ms")
    if trail is not None and trail > float(thr.get("trailing_bleed_ms", 120)):
        reasons.append(f"trailing bleed {trail:.0f}ms")
        timing_fail = True
    if lead is not None and lead > float(thr.get("leading_bleed_ms", 120)):
        reasons.append(f"leading bleed {lead:.0f}ms")
        timing_fail = True

    regions = align.get("vad_regions") or []
    if single_token and len(regions) > 1 and thr.get("multi_speech_region_fail", True):
        reasons.append(f"multiple VAD regions ({len(regions)}) on single-word clip")
        timing_fail = True

    ratio = align.get("duration_ratio")
    tol = float(thr.get("duration_tolerance", 0.40))
    if ratio is not None and (ratio < 1 - tol or ratio > 1 + tol):
        reasons.append(f"duration ratio {ratio:.2f} outside ±{tol:.0%}")
        warn = True

    for gap in align.get("gaps_ms") or []:
        g = float(gap.get("gap_ms") or 0)
        pair = gap.get("between")
        if g < float(thr.get("assembled_merge_gap_ms", 40)):
            reasons.append(f"merged gap {g:.0f}ms between {pair}")
            fail = True
        elif g > float(thr.get("assembled_long_gap_ms", 600)):
            reasons.append(f"long gap {g:.0f}ms between {pair}")
            warn = True

    if not norm_eq:
        detail = (
            f"transcript mismatch WER={wer:.2f} CER={cer:.2f} "
            f"ins={word_diff.get('insertions')} del={word_diff.get('deletions')} "
            f"sub={word_diff.get('substitutions')}"
        )
        if short and not timing_fail:
            reasons.append(f"WARN short-clip ASR: {detail}")
            warn = True
        elif primary >= primary_fail or timing_fail:
            reasons.append(detail)
            fail = True
        elif primary >= primary_warn:
            reasons.append(detail)
            warn = True
        else:
            reasons.append(detail)
            warn = True

    # ffmpeg loudness / clipping / silence
    lufs = ffmpeg.get("integrated_lufs")
    if lufs is not None:
        target = float(thr.get("target_lufs", -16))
        tol_l = float(thr.get("lufs_tolerance", 3.5))
        if abs(lufs - target) > tol_l:
            reasons.append(f"LUFS {lufs:.1f} vs target {target}")
            warn = True
    peak = ffmpeg.get("true_peak_dbtp")
    if peak is not None and peak > float(thr.get("max_true_peak_dbtp", -1.0)):
        reasons.append(f"true peak {peak:.1f} dBTP")
        fail = True
    if (ffmpeg.get("clipped_samples") or 0) > 0:
        reasons.append(f"clipped samples {ffmpeg.get('clipped_samples')}")
        fail = True
    nf = ffmpeg.get("noise_floor_dbfs")
    if nf is not None:
        if nf > float(thr.get("noise_floor_dbfs_fail", -35)):
            reasons.append(f"noise floor {nf:.1f} dBFS")
            fail = True
        elif nf > float(thr.get("noise_floor_dbfs_warn", -45)):
            reasons.append(f"noise floor {nf:.1f} dBFS")
            warn = True

    bak = quality.get("dnsmos_bak")
    if bak is not None:
        if bak < float(thr.get("dnsmos_bak_fail", 2.8)):
            reasons.append(f"DNSMOS BAK {bak:.2f}")
            fail = True
        elif bak < float(thr.get("dnsmos_bak_warn", 3.5)):
            reasons.append(f"DNSMOS BAK {bak:.2f}")
            warn = True

    if timing_fail:
        fail = True

    if fail:
        return "FAIL", reasons or ["failed checks"]
    if warn:
        return "WARN", reasons or ["warnings"]
    return "PASS", reasons or ["ok"]
