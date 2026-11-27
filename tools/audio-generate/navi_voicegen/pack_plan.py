"""Dry-run pack size / clip-count estimates per pack_style mode."""

from __future__ import annotations

from dataclasses import dataclass

from navi_voicegen.languages import PackStyle, SPECS, resolve_pack_style
from navi_voicegen.phrases import PhraseFile, load_phrase_file

# voice-guidance.md §4 planning bytes (mono Opus ~32 kbps order of magnitude).
FRAGMENT_CLIP_BYTES = 8_000
WHOLE_CLIP_BYTES = 15_000


@dataclass(frozen=True)
class ModePlan:
    mode: str
    pack_style: PackStyle
    clip_keys: int
    clips_per_gender: int
    estimated_bytes_per_gender: int
    genders: int

    @property
    def estimated_total_bytes(self) -> int:
        return self.estimated_bytes_per_gender * self.genders


def _hybrid_clip_count(fragment_count: int) -> int:
    """Fragment inventory plus composed distance+maneuver phrases (not generated yet)."""
    distance_announcements = 10  # 50–800 m + 1/2/5 km style slots
    maneuver_whole = 12  # §5 maneuvers that may need whole clips in hybrid
    return fragment_count + distance_announcements + maneuver_whole


def _whole_phrase_clip_count(fragment_count: int) -> int:
    """Whole-phrase inventory replaces most distance templates."""
    return max(150, fragment_count * 3)


def plan_for_language(lang_code: str, mode: str, *, genders: int = 2) -> ModePlan:
    pf = load_phrase_file(lang_code)
    spec = SPECS[lang_code]
    style = resolve_pack_style(spec, mode)
    fragment_n = len(pf.entries)
    if style == PackStyle.FRAGMENT:
        per_gender = fragment_n
        est = per_gender * FRAGMENT_CLIP_BYTES
    elif style == PackStyle.HYBRID:
        per_gender = _hybrid_clip_count(fragment_n)
        est = fragment_n * FRAGMENT_CLIP_BYTES + (per_gender - fragment_n) * WHOLE_CLIP_BYTES
    else:
        per_gender = _whole_phrase_clip_count(fragment_n)
        est = per_gender * WHOLE_CLIP_BYTES
    return ModePlan(
        mode=mode,
        pack_style=style,
        clip_keys=fragment_n,
        clips_per_gender=per_gender,
        estimated_bytes_per_gender=est,
        genders=genders,
    )


def inventory_pack_style(pf: PhraseFile) -> str:
    """Manifest pack_style must reflect what clips exist (fragment keys in YAML)."""
    styles = {e.pack_style for e in pf.entries if e.pack_style}
    if not styles:
        return "fragment"
    if len(styles) == 1:
        return next(iter(styles))
    return "hybrid"


def format_mode_report(lang_code: str) -> str:
    lines = [f"Pack mode dry-run for {lang_code} (deterministic estimates):", ""]
    lines.append("| mode | pack_style | clips/gender | est. bytes/gender | est. total (2 genders) |")
    lines.append("|---|---|---:|---:|---:|")
    for mode in ("fragment", "hybrid", "whole_phrase"):
        plan = plan_for_language(lang_code, mode)
        lines.append(
            f"| {mode} | {plan.pack_style.value} | {plan.clips_per_gender} | "
            f"{plan.estimated_bytes_per_gender} | {plan.estimated_total_bytes} |"
        )
    return "\n".join(lines) + "\n"