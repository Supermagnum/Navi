"""Chatterbox Multilingual synthesis (optional heavy import)."""

from __future__ import annotations

import tempfile
from pathlib import Path
from typing import Any


class ChatterboxUnavailable(RuntimeError):
    pass


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
    from chatterbox.mtl_tts import ChatterboxMultilingualTTS

    if seed is not None:
        torch.manual_seed(seed)

    model = ChatterboxMultilingualTTS.from_pretrained(device=device, t3_model="v3")
    kwargs: dict[str, Any] = {"language_id": language_id}
    if voice_ref is not None:
        kwargs["audio_prompt_path"] = str(voice_ref)
    wav = model.generate(text, **kwargs)
    out = Path(tempfile.mkstemp(suffix=".wav")[1])
    ta.save(str(out), wav, model.sr)
    return out