"""Concatenate fragment clips into approach-style sentences; compare to whole-phrase synth."""

from __future__ import annotations

import json
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path

from navi_voicegen.deps import resolve_device
from navi_voicegen.engines.chatterbox_engine import synthesize_wav
from navi_voicegen.languages import SPECS
from navi_voicegen.packs import find_clip_for_key
from navi_voicegen.paths import default_out_dir, repo_root
from navi_voicegen.phrases import load_phrase_file
from navi_voicegen.postprocess import encode_clip
from navi_voicegen.voices.resolve import resolve_voice_ref_path

# Gap between fragment clips (ms) — no crossfade; matches sequential rodio playback spacing.
ASSEMBLED_INTER_CLIP_GAP_MS = 120

# approach-instructions.md + review sentence sets for native concat audition.
NB_ASSEMBLED_SENTENCES: list[tuple[str, list[str], str]] = [
    ("om_to_hundre_meter_ta_til_venstre", ["in", "two", "hundred", "meters", "left"], "Om to hundre meter, ta til venstre"),
    ("femti_meter_ta_til_hoyre", ["in", "fifty", "meters", "right"], "Om femti meter, ta til høyre"),
    ("fem_kilometer_fortsett_rett_frem", ["in", "five", "kilometers", "straight"], "Om fem kilometer, fortsett rett frem"),
    ("hundre_meter_rundkjoring_forste", ["in", "hundred", "meters", "roundabout", "first"], "Om hundre meter, rundkjøring, første"),
    ("tohundre_meter_flett", ["in", "two", "hundred", "meters", "merge"], "Om to hundre meter, flett"),
]


@dataclass
class AssembledAuditionResult:
    index_path: Path
    assembled_dir: Path
    whole_dir: Path
    gap_ms: int


def audition_root(lang_code: str) -> Path:
    folder = SPECS[lang_code].folder
    return repo_root() / "build" / "audition" / folder


def _concat_opus_with_gap(clips: list[Path], dest: Path, gap_ms: int) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        td = Path(tmp)
        parts: list[Path] = []
        silence_dur = gap_ms / 1000.0
        for i, clip in enumerate(clips):
            parts.append(clip)
            if i < len(clips) - 1:
                sil = td / f"gap_{i}.opus"
                subprocess.run(
                    [
                        "ffmpeg",
                        "-hide_banner",
                        "-loglevel",
                        "error",
                        "-y",
                        "-f",
                        "lavfi",
                        "-i",
                        f"anullsrc=r=48000:cl=mono",
                        "-t",
                        str(silence_dur),
                        "-c:a",
                        "libopus",
                        str(sil),
                    ],
                    check=True,
                )
                parts.append(sil)
        list_file = td / "list.txt"
        list_file.write_text(
            "".join(f"file '{p.resolve()}'\n" for p in parts),
            encoding="utf-8",
        )
        subprocess.run(
            [
                "ffmpeg",
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "concat",
                "-safe",
                "0",
                "-i",
                str(list_file),
                "-c",
                "copy",
                str(dest),
            ],
            check=True,
        )


def run_assembled_audition(
    lang_code: str,
    *,
    genders: tuple[str, ...] = ("male", "female"),
    pack_root: Path | None = None,
    device: str = "auto",
    fmt: str = "opus",
    seed: int | None = None,
    dry_run: bool = False,
) -> AssembledAuditionResult:
    spec = SPECS[lang_code]
    load_phrase_file(lang_code)
    device = resolve_device(device)
    root = pack_root or default_out_dir()
    unreviewed_root = root / "unreviewed" / spec.folder
    if unreviewed_root.is_dir():
        clip_root = unreviewed_root
    else:
        clip_root = root / spec.folder
    ext = "opus" if fmt == "opus" else "mp3"
    base = audition_root(lang_code)
    assembled_base = base / "assembled"
    whole_base = base / "whole"
    rows: list[dict] = []

    for gender in genders:
        adir = assembled_base / gender
        wdir = whole_base / gender
        ref, _ = resolve_voice_ref_path(lang_code, gender)
        if ref is None and not dry_run:
            raise SystemExit(f"No voice ref for {lang_code}/{gender}")
        for slug, keys, whole_text in NB_ASSEMBLED_SENTENCES:
            clip_paths: list[Path] = []
            missing = False
            for key in keys:
                p = find_clip_for_key(clip_root, gender, key, ext)
                if p is None:
                    missing = True
                    break
                clip_paths.append(p)
            asm_out = adir / f"{slug}.{ext}"
            whole_out = wdir / f"{slug}.{ext}"
            if dry_run or missing:
                print(f"DRY-RUN assembled {asm_out} keys={keys} missing={missing}")
                print(f"DRY-RUN whole {whole_out} <- {whole_text!r}")
                rows.append(
                    {
                        "slug": slug,
                        "gender": gender,
                        "assembled": asm_out.relative_to(base).as_posix(),
                        "whole": whole_out.relative_to(base).as_posix(),
                        "gap_ms": ASSEMBLED_INTER_CLIP_GAP_MS,
                        "missing_clips": missing,
                    }
                )
                continue
            _concat_opus_with_gap(clip_paths, asm_out, ASSEMBLED_INTER_CLIP_GAP_MS)
            wav = synthesize_wav(
                text=whole_text,
                language_id=spec.chatterbox_id,
                device=device,  # resolved
                voice_ref=ref,
                seed=seed,
            )
            try:
                encode_clip(wav, whole_out, fmt)
            finally:
                wav.unlink(missing_ok=True)
            rows.append(
                {
                    "slug": slug,
                    "gender": gender,
                    "assembled": asm_out.relative_to(base).as_posix(),
                    "whole": whole_out.relative_to(base).as_posix(),
                    "gap_ms": ASSEMBLED_INTER_CLIP_GAP_MS,
                    "whole_text": whole_text,
                }
            )

    index = base / "INDEX.md"
    lines = [
        f"# {spec.folder} assembled vs whole-phrase audition",
        "",
        f"Inter-clip gap (assembled): **{ASSEMBLED_INTER_CLIP_GAP_MS} ms** (silent insert; no crossfade).",
        "",
        "| sentence | gender | assembled | whole |",
        "|---|---|---|---|",
    ]
    for row in rows:
        lines.append(
            f"| {row['slug']} | {row['gender']} | `{row['assembled']}` | `{row['whole']}` |"
        )
    lines.extend(
        [
            "",
            "## Sentence sets (fragment keys)",
            "",
        ]
    )
    for slug, keys, whole_text in NB_ASSEMBLED_SENTENCES:
        lines.append(f"- **{slug}**: `{' + '.join(keys)}` → whole: “{whole_text}”")
    meta = base / "assembled-meta.json"
    meta.write_text(
        json.dumps(
            {"gap_ms": ASSEMBLED_INTER_CLIP_GAP_MS, "pairs": rows},
            indent=2,
            ensure_ascii=False,
        )
        + "\n",
        encoding="utf-8",
    )
    index.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return AssembledAuditionResult(
        index_path=index,
        assembled_dir=assembled_base,
        whole_dir=whole_base,
        gap_ms=ASSEMBLED_INTER_CLIP_GAP_MS,
    )