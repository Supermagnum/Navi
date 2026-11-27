"""Mozilla Common Voice — manual placement only (account terms)."""

from __future__ import annotations

from pathlib import Path

from navi_voicegen.paths import tool_root


def commonvoice_cache_dir(lang_code: str) -> Path:
    return tool_root() / "cache" / "commonvoice" / lang_code


def commonvoice_manual_instructions(lang_code: str) -> str:
    base = commonvoice_cache_dir(lang_code)
    return (
        "Mozilla Common Voice requires accepting dataset terms on mozilla.org — "
        "the CLI will not log in on your behalf.\n\n"
        f"After you download clips manually, place files under:\n"
        f"  {base}/male/<clip-id>.mp3\n"
        f"  {base}/female/<clip-id>.mp3\n"
        "Then add candidates to voices.yaml with source_url file://… and pin SHA-256.\n"
        f"Corpus hub: https://commonvoice.mozilla.org/{lang_code}"
    )


def resolve_commonvoice_local(lang_code: str, gender: str, clip_id: str, dest: Path) -> None:
    for ext in ("mp3", "wav", "opus", "ogg"):
        src = commonvoice_cache_dir(lang_code) / gender / f"{clip_id}.{ext}"
        if src.is_file():
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(src.read_bytes())
            return
    raise SystemExit(commonvoice_manual_instructions(lang_code))