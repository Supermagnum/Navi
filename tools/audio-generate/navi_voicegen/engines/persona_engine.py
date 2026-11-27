"""Style-Bert-VITS2 persona path (manual checkpoint only)."""

from __future__ import annotations

from pathlib import Path


PERSONA_LICENSE_WARNING = """
Style-Bert-VITS2 persona packs (voice-guidance.md §10a):
- Engine is AGPL-3.0; checkpoint license and training-data rights must be verified separately.
- Do not use community checkpoints without explicit redistribution permission.
- Navi will not auto-download checkpoints.
"""


def require_persona_args(
    *,
    persona: bool,
    persona_checkpoint: Path | None,
    license_confirmed: bool,
) -> None:
    if not persona:
        return
    print(PERSONA_LICENSE_WARNING.strip())
    if persona_checkpoint is None or not persona_checkpoint.is_file():
        raise SystemExit(
            "Persona generation requires --persona-checkpoint PATH to an existing local checkpoint."
        )
    if not license_confirmed:
        raise SystemExit(
            "Persona generation requires --checkpoint-license-confirmed after you verified rights."
        )


def synthesize_persona_placeholder(
    *,
    checkpoint: Path,
    text: str,
    device: str,
    seed: int | None,
) -> None:
    raise NotImplementedError(
        "Style-Bert-VITS2 persona synthesis is not wired in this scaffold; "
        f"checkpoint={checkpoint} device={device} seed={seed} text_len={len(text)}. "
        "Install Style-Bert-VITS2 manually and extend persona_engine.py."
    )