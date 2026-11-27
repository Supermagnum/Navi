"""Chatterbox Multilingual synthesis (optional heavy import)."""

from __future__ import annotations

import tempfile
from pathlib import Path
from typing import Any


class ChatterboxUnavailable(RuntimeError):
    pass


_model_cache: dict[str, Any] = {}

# Upstream Chatterbox 0.1.7 alignment crash on a few one-token fragments.
_SYNTH_TEXT_ALIASES: dict[str, str] = {"One": "Wun", "In": "inn", "Then": "Den"}


def _synth_input(text: str) -> str:
    if text in _SYNTH_TEXT_ALIASES:
        return _SYNTH_TEXT_ALIASES[text]
    if " " not in text and len(text) <= 6:
        return f"{text} now"
    return text


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
    synth_text = _synth_input(text)
    last_err: Exception | None = None
    for attempt, bump in enumerate((0, 1, 7, 13)):
        try:
            if seed is not None:
                torch.manual_seed(seed + bump)
            wav = model.generate(synth_text, **kwargs)
            last_err = None
            break
        except IndexError as exc:
            last_err = exc
            if attempt >= 3:
                raise
    if last_err is not None:
        raise last_err
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