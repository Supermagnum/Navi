"""Word timing / bleed checks (WhisperX when available, else VAD duration)."""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any

from navi_voicegen.qc.normalize import syllable_estimate, tokens_for_wer
from navi_voicegen.qc.vad import VadRegion, speech_regions


@dataclass
class AlignResult:
    backend: str
    words: list[dict[str, Any]] = field(default_factory=list)
    leading_bleed_ms: float | None = None
    trailing_bleed_ms: float | None = None
    gaps_ms: list[dict[str, Any]] = field(default_factory=list)
    duration_s: float | None = None
    expected_duration_s: float | None = None
    duration_ratio: float | None = None
    vad_regions: list[dict[str, float]] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


def _expected_duration(text: str, lang_code: str, median_rate_syl_per_s: float = 4.0) -> float:
    syl = syllable_estimate(text, lang_code)
    return max(0.25, syl / median_rate_syl_per_s)


def align_clip(
    audio: Path,
    expected_text: str,
    lang_code: str,
    thr: dict[str, Any],
    cfg: dict[str, Any],
    *,
    assembled_keys: list[str] | None = None,
) -> AlignResult:
    notes: list[str] = []
    align_cfg = cfg.get("alignment") or {}
    prefer = str(align_cfg.get(lang_code) or align_cfg.get("default") or "vad_duration")

    regions = speech_regions(audio)
    vad = [{"start_s": r.start_s, "end_s": r.end_s} for r in regions]

    duration = None
    try:
        from navi_voicegen.qc.ffmpeg_metrics import probe_duration

        duration = probe_duration(audio)
    except Exception:
        if regions:
            duration = regions[-1].end_s

    expected_dur = _expected_duration(expected_text, lang_code)
    ratio = (duration / expected_dur) if duration and expected_dur else None

    words: list[dict[str, Any]] = []
    backend = "vad_duration"
    if prefer == "whisperx":
        try:
            words = _whisperx_words(audio, expected_text, lang_code)
            backend = "whisperx"
        except Exception as exc:  # noqa: BLE001
            notes.append(f"whisperx unavailable ({exc}); VAD+duration fallback")
            backend = "vad_duration"

    leading = None
    trailing = None
    if words:
        leading = max(0.0, float(words[0]["start"]) * 1000)
        if duration is not None:
            trailing = max(0.0, (duration - float(words[-1]["end"])) * 1000)
    elif regions:
        leading = max(0.0, regions[0].start_s * 1000)
        if duration is not None:
            trailing = max(0.0, (duration - regions[-1].end_s) * 1000)

    gaps: list[dict[str, Any]] = []
    if assembled_keys and words and len(words) >= 2:
        # Approximate joints between consecutive words
        for i in range(len(words) - 1):
            gap = (float(words[i + 1]["start"]) - float(words[i]["end"])) * 1000
            left = assembled_keys[min(i, len(assembled_keys) - 1)]
            right = assembled_keys[min(i + 1, len(assembled_keys) - 1)]
            gaps.append({"between": [left, right], "gap_ms": gap})
    elif assembled_keys and len(regions) >= 2:
        for i in range(len(regions) - 1):
            gap = (regions[i + 1].start_s - regions[i].end_s) * 1000
            left = assembled_keys[min(i, len(assembled_keys) - 1)]
            right = assembled_keys[min(i + 1, len(assembled_keys) - 1)]
            gaps.append({"between": [left, right], "gap_ms": gap})

    return AlignResult(
        backend=backend,
        words=words,
        leading_bleed_ms=leading,
        trailing_bleed_ms=trailing,
        gaps_ms=gaps,
        duration_s=duration,
        expected_duration_s=expected_dur,
        duration_ratio=ratio,
        vad_regions=vad,
        notes=notes,
    )


def _whisperx_words(audio: Path, expected_text: str, lang_code: str) -> list[dict[str, Any]]:
    """Optional WhisperX alignment; raises if not installed."""
    import whisperx  # type: ignore

    device = "cpu"
    try:
        import torch

        if torch.cuda.is_available():
            device = "cuda"
    except Exception:
        pass
    audio_arr = whisperx.load_audio(str(audio))
    model = whisperx.load_model("large-v3", device, compute_type="int8")
    lang = {"nb": "no"}.get(lang_code, lang_code)
    result = model.transcribe(audio_arr, language=lang, text=expected_text)
    model_a, metadata = whisperx.load_align_model(language_code=lang, device=device)
    aligned = whisperx.align(result["segments"], model_a, metadata, audio_arr, device)
    words = []
    for w in aligned.get("word_segments") or []:
        if w.get("start") is None or w.get("end") is None:
            continue
        words.append({"word": w.get("word"), "start": w["start"], "end": w["end"]})
    return words


def multi_region_flag(regions: list[VadRegion] | list[dict], single_word: bool) -> bool:
    if not single_word:
        return False
    return len(regions) > 1
