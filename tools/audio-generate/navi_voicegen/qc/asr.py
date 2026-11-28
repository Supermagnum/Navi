"""ASR backends for QC transcript checks."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass
class AsrResult:
    text: str
    language: str | None
    backend: str
    model: str
    notes: list[str]


_model_cache: dict[str, Any] = {}


def _device_str(prefer_cuda: bool) -> str:
    if not prefer_cuda:
        return "cpu"
    try:
        import torch

        return "cuda" if torch.cuda.is_available() else "cpu"
    except Exception:
        return "cpu"


def _faster_whisper(
    audio: Path,
    *,
    model_name: str,
    language: str | None,
    prefer_cuda: bool,
    compute_cuda: str,
    compute_cpu: str,
) -> AsrResult:
    from faster_whisper import WhisperModel

    device = _device_str(prefer_cuda)
    compute = compute_cuda if device == "cuda" else compute_cpu
    key = f"fw:{model_name}:{device}:{compute}"
    if key not in _model_cache:
        _model_cache[key] = WhisperModel(model_name, device=device, compute_type=compute)
    model = _model_cache[key]
    segments, info = model.transcribe(
        str(audio),
        language=language,
        beam_size=1,
        vad_filter=True,
    )
    text = " ".join(seg.text.strip() for seg in segments).strip()
    return AsrResult(
        text=text,
        language=getattr(info, "language", language),
        backend="faster-whisper",
        model=model_name,
        notes=[],
    )


def _transformers_whisper(
    audio: Path,
    *,
    model_id: str,
    language: str | None,
    prefer_cuda: bool,
) -> AsrResult:
    import torch
    from transformers import pipeline

    device = 0 if prefer_cuda and torch.cuda.is_available() else -1
    key = f"hf:{model_id}:{device}"
    if key not in _model_cache:
        _model_cache[key] = pipeline(
            "automatic-speech-recognition",
            model=model_id,
            device=device,
        )
    pipe = _model_cache[key]
    kwargs: dict[str, Any] = {"generate_kwargs": {}}
    if language:
        # transformers whisper uses language names sometimes; pass code when supported
        kwargs["generate_kwargs"]["language"] = language
    out = pipe(str(audio), **kwargs)
    text = out["text"] if isinstance(out, dict) else str(out)
    return AsrResult(
        text=str(text).strip(),
        language=language,
        backend="transformers",
        model=model_id,
        notes=[],
    )


def transcribe_clip(
    audio: Path,
    lang_code: str,
    cfg: dict[str, Any],
    *,
    prefer_cuda: bool = True,
) -> AsrResult:
    asr_cfg = cfg.get("asr") or {}
    lang_asr = asr_cfg.get(lang_code) or asr_cfg.get("default") or {}
    backend = str(lang_asr.get("backend") or "faster-whisper")
    model = str(lang_asr.get("model") or "large-v3")
    notes: list[str] = []

    # Map Navi codes to Whisper language codes
    whisper_lang = {"nb": "no", "zh": "zh", "ja": "ja", "ar": "ar", "en": "en"}.get(
        lang_code, lang_code
    )

    try:
        if backend == "nb-whisper":
            try:
                result = _transformers_whisper(
                    audio,
                    model_id=model,
                    language="no",
                    prefer_cuda=prefer_cuda,
                )
                result.notes.append("nb-whisper (NbAiLab, Apache-2.0)")
                return result
            except Exception as exc:  # noqa: BLE001 — fall back per spec
                notes.append(f"nb-whisper unavailable ({exc}); falling back")
                fb = lang_asr.get("fallback_backend") or "faster-whisper"
                fm = lang_asr.get("fallback_model") or "large-v3"
                backend, model = str(fb), str(fm)

        if backend in ("faster-whisper", "whisper"):
            try:
                result = _faster_whisper(
                    audio,
                    model_name=model,
                    language=whisper_lang if lang_asr.get("force_language", True) else None,
                    prefer_cuda=prefer_cuda,
                    compute_cuda=str(
                        lang_asr.get("compute_type_cuda")
                        or asr_cfg.get("default", {}).get("compute_type_cuda")
                        or "int8_float16"
                    ),
                    compute_cpu=str(lang_asr.get("compute_type_cpu") or "int8"),
                )
            except Exception as cuda_exc:  # noqa: BLE001
                notes.append(f"CUDA/ASR retry on CPU after: {cuda_exc}")
                # Drop any cached CUDA model that may be pinning VRAM
                _model_cache.clear()
                try:
                    import torch

                    if torch.cuda.is_available():
                        torch.cuda.empty_cache()
                except Exception:
                    pass
                result = _faster_whisper(
                    audio,
                    model_name=model,
                    language=whisper_lang if lang_asr.get("force_language", True) else None,
                    prefer_cuda=False,
                    compute_cuda="int8",
                    compute_cpu=str(lang_asr.get("compute_type_cpu") or "int8"),
                )
            result.notes.extend(notes)
            return result

        raise RuntimeError(f"Unknown ASR backend {backend!r}")
    except Exception as exc:  # noqa: BLE001
        return AsrResult(
            text="",
            language=None,
            backend=backend,
            model=model,
            notes=notes + [f"ASR failed: {exc}"],
        )
