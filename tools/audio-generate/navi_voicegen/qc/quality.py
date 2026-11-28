"""Optional quality models (DNSMOS / NISQA / Brouhaha / Squim) with soft skips."""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class QualityScores:
    dnsmos_sig: float | None = None
    dnsmos_bak: float | None = None
    dnsmos_ovrl: float | None = None
    nisqa_mos: float | None = None
    nisqa_noi: float | None = None
    brouhaha_snr: float | None = None
    brouhaha_c50: float | None = None
    squim_stoi: float | None = None
    squim_pesq: float | None = None
    squim_si_sdr: float | None = None
    skipped: list[str] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


def score_quality(audio: Path) -> QualityScores:
    scores = QualityScores()

    # TorchAudio Squim (BSD-style via PyTorch) — cheap when torchaudio present
    try:
        import torch
        import torchaudio
        from torchaudio.pipelines import SQUIM_OBJECTIVE

        waveform, sr = torchaudio.load(str(audio))
        if waveform.shape[0] > 1:
            waveform = waveform.mean(dim=0, keepdim=True)
        if sr != 16000:
            waveform = torchaudio.functional.resample(waveform, sr, 16000)
        model = SQUIM_OBJECTIVE.get_model()
        model.eval()
        with torch.no_grad():
            stoi, pesq, si_sdr = model(waveform)
        scores.squim_stoi = float(stoi.mean())
        scores.squim_pesq = float(pesq.mean())
        scores.squim_si_sdr = float(si_sdr.mean())
    except Exception as exc:  # noqa: BLE001
        scores.skipped.append(f"torchaudio-squim skipped: {exc}")

    # DNSMOS: Microsoft DNS-Challenge is CC-BY-4.0, but the P.835 ONNX helper is
    # awkward to vendor. Skip unless a local helper is already installed.
    try:
        import importlib

        if importlib.util.find_spec("speechmos") is None:
            raise ImportError("speechmos not installed")
        scores.skipped.append("dnsmos: speechmos present but not wired in this build")
    except Exception as exc:  # noqa: BLE001
        scores.skipped.append(f"dnsmos skipped: {exc}")

    # NISQA (MIT) — optional heavy install
    try:
        import importlib

        if importlib.util.find_spec("nisqa") is None:
            raise ImportError("nisqa package not installed")
        scores.skipped.append("nisqa present but not wired in this build")
    except Exception as exc:  # noqa: BLE001
        scores.skipped.append(f"nisqa skipped: {exc}")

    # Brouhaha (MIT) — optional
    try:
        import importlib

        if importlib.util.find_spec("brouhaha") is None:
            raise ImportError("brouhaha not installed")
        scores.skipped.append("brouhaha present but not wired in this build")
    except Exception as exc:  # noqa: BLE001
        scores.skipped.append(f"brouhaha skipped: {exc}")

    return scores
