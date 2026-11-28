"""Chatterbox Multilingual synthesis (optional heavy import)."""

from __future__ import annotations

import tempfile
from pathlib import Path
from typing import Any


class ChatterboxUnavailable(RuntimeError):
    pass


_model_cache: dict[str, Any] = {}

# Upstream Chatterbox 0.1.7 alignment crash on a few one-token fragments.
# Prefer phonetic/spelling aliases over padding with an extra spoken English word.
_SYNTH_TEXT_ALIASES: dict[str, str] = {"One": "Wun", "In": "inn", "Then": "Den"}

# Soft, non-lexical fallbacks if the bare token still trips alignment.
_SHORT_TOKEN_FALLBACKS: tuple[str, ...] = (".", "…", ",")


def _synth_candidates(text: str) -> list[str]:
    """Ordered synthesis strings.

    Never prefer English \" now\" — it is audible on number fragments. Keep it only as a
    last-resort pad after punctuation fails; postprocess keeps the first speech island so
    a separated trailing pad word is discarded.
    """
    primary = _SYNTH_TEXT_ALIASES.get(text, text)
    candidates = [primary]
    if primary != text:
        candidates.append(text)

    short = " " not in primary and len(primary) <= 6
    if short:
        for suffix in _SHORT_TOKEN_FALLBACKS:
            candidates.append(f"{primary}{suffix}")
        # Last resorts: multi-token pads that usually avoid IndexError.
        candidates.append(f"{primary} now")
        candidates.append(f"{primary} {primary}")

    out: list[str] = []
    for item in candidates:
        if item not in out:
            out.append(item)
    return out


def _synth_input(text: str) -> str:
    """Primary synth string (first candidate). Kept for tests and call sites."""
    return _synth_candidates(text)[0]


def supported_language_ids() -> set[str]:
    try:
        from chatterbox.mtl_tts import ChatterboxMultilingualTTS  # noqa: F401
    except ImportError as exc:
        raise ChatterboxUnavailable(
            "chatterbox-tts not installed; run navi-voicegen --setup-only"
        ) from exc
    # Documented language_id values in upstream examples + table.
    return {
        "en",
        "fr",
        "de",
        "ru",
        "ar",
        "zh",
        "ja",
        "uk",
        "hi",
        "no",
        "sv",
        "da",
        "fi",
        "pl",
        "nl",
        "tr",
        "hu",
        "ro",
        "vi",
        "es",
        "pt",
        "it",
        "ko",
    }


def _ensure_perth_watermarker() -> None:
    """resemble-perth may ship without PerthImplicitWatermarker on Python 3.13."""
    import perth
    from perth.dummy_watermarker import DummyWatermarker

    if perth.PerthImplicitWatermarker is None:
        perth.PerthImplicitWatermarker = DummyWatermarker


def _get_model(device: str):
    import torch
    from chatterbox.mtl_tts import ChatterboxMultilingualTTS

    _ensure_perth_watermarker()
    dev = torch.device(device)
    key = str(dev)
    if key not in _model_cache:
        _model_cache[key] = ChatterboxMultilingualTTS.from_pretrained(dev)
    return _model_cache[key]


def synthesize_wav(
    *,
    text: str,
    language_id: str,
    device: str,
    voice_ref: Path | None,
    seed: int | None,
) -> Path:
    import torch
    import torchaudio as ta

    if seed is not None:
        torch.manual_seed(seed)

    model = _get_model(device)
    kwargs: dict[str, Any] = {"language_id": language_id}
    if voice_ref is not None:
        kwargs["audio_prompt_path"] = str(voice_ref)

    candidates = _synth_candidates(text)
    seed_bumps = (0, 1, 7, 13)
    last_err: Exception | None = None
    wav = None
    for synth_text in candidates:
        for bump in seed_bumps:
            try:
                if seed is not None:
                    torch.manual_seed(seed + bump)
                wav = model.generate(synth_text, **kwargs)
                last_err = None
                break
            except IndexError as exc:
                last_err = exc
        if last_err is None:
            break
    if last_err is not None or wav is None:
        raise last_err if last_err is not None else RuntimeError("synthesis produced no audio")

    out = Path(tempfile.mkstemp(suffix=".wav")[1])
    ta.save(str(out), wav, model.sr)
    return out


def clear_model_cache() -> None:
    import gc

    import torch

    _model_cache.clear()
    if torch.cuda.is_available():
        torch.cuda.empty_cache()
    gc.collect()
