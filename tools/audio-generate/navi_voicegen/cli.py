"""navi-voicegen CLI entry."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from navi_voicegen.deps import SystemDepsError, ensure_system_deps
from navi_voicegen.draft_phrases import draft_language
from navi_voicegen.assembled_audition import run_assembled_audition
from navi_voicegen.pack_plan import format_mode_report
from navi_voicegen.review_sheet import apply_review_sheet, write_review_sheet
from navi_voicegen.generate import GenerateOptions, generate_packs
from navi_voicegen.languages import iter_tier_matrix, resolve_language_codes
from navi_voicegen.paths import default_out_dir
from navi_voicegen.setup_env import setup_only
from navi_voicegen.voices.audition import run_audition
from navi_voicegen.voices.config import load_voices_yaml
from navi_voicegen.voices.fetch import fetch_voices
from navi_voicegen.voices.sprakbanken_corpus import fetch_sprakbanken_sbr14


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="navi-voicegen",
        description="Author offline Navi turn-by-turn voice clip packs (no runtime TTS).",
    )
    p.add_argument("--setup-only", action="store_true", help="Install venv, deps, template caches")
    p.add_argument(
        "--fetch-voices",
        action="store_true",
        help="Download/verify voice refs from voices.yaml into gitignored cache",
    )
    p.add_argument(
        "--fetch-corpus",
        metavar="ID",
        help="Download a pinned corpus into gitignored cache (e.g. sprakbanken-sbr14)",
    )
    p.add_argument(
        "--audition",
        action="store_true",
        help="Synthesize audition clips for up to 3 candidates per gender (see voices.yaml)",
    )
    p.add_argument("--lang", default="en", help="Language code, folder alias, tier1|tier2|tier3|all")
    p.add_argument("--list-languages", action="store_true", help="Print tier matrix and Chatterbox coverage")
    p.add_argument("--gender", default="both", choices=("male", "female", "both"))
    p.add_argument(
        "--mode",
        default="auto",
        choices=("auto", "fragment", "whole_phrase", "hybrid"),
        help="Pack style; auto follows voice-guidance.md §6 bias",
    )
    p.add_argument("--format", default="opus", choices=("opus", "mp3"))
    p.add_argument("--out", type=Path, default=None, help="Output root (default: build/sounds/)")
    p.add_argument("--device", default="auto", choices=("auto", "cpu", "cuda"))
    p.add_argument("--seed", type=int, default=None)
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--force", action="store_true", help="Regenerate even when input hash matches")
    p.add_argument("--persona", action="store_true", help="Persona pack (Style-Bert-VITS2; opt-in)")
    p.add_argument("--persona-checkpoint", type=Path, default=None)
    p.add_argument("--checkpoint-license-confirmed", action="store_true")
    p.add_argument("--suffix", default=None, help="Optional persona sentence-ending suffix clip text")
    p.add_argument("--draft-phrases", action="store_true", help="Draft phrases/*.yaml from template cache")
    p.add_argument(
        "--review-sheet",
        action="store_true",
        help="Write build/review/<language>/review-sheet.tsv from phrases/<lang>.yaml",
    )
    p.add_argument(
        "--assembled-audition",
        action="store_true",
        help="Concat fragment clips + synthesize whole phrases (build/audition/<folder>/assembled|whole)",
    )
    p.add_argument(
        "--pack-mode-report",
        action="store_true",
        help="Print deterministic dry-run clip counts/sizes for fragment|hybrid|whole_phrase",
    )
    p.add_argument(
        "--apply-review",
        type=Path,
        default=None,
        metavar="FILE",
        help="Apply native edits from a review-sheet TSV back into phrases/<lang>.yaml",
    )
    p.add_argument("--allow-unreviewed", action="store_true")
    p.add_argument("--voice-ref-male", type=Path, default=None)
    p.add_argument("--voice-ref-female", type=Path, default=None)
    return p


def _genders(arg: str) -> list[str]:
    if arg == "both":
        return ["male", "female"]
    return [arg]


def _langs_for_voice_ops(args: argparse.Namespace) -> list[str]:
    if args.lang != "en":
        return resolve_language_codes(args.lang)
    catalog = list((load_voices_yaml().get("languages") or {}).keys())
    return catalog if catalog else ["en"]


def _run_fetch(args: argparse.Namespace) -> int:
    try:
        lang_codes = _langs_for_voice_ops(args) if args.setup_only and not args.audition else resolve_language_codes(args.lang)
    except ValueError as exc:
        print(exc, file=sys.stderr)
        return 2
    try:
        ensure_system_deps(for_generate=False)
    except SystemDepsError as exc:
        print("\n".join(exc.messages), file=sys.stderr)
        return 2
    reports = fetch_voices(lang_codes, genders=tuple(_genders(args.gender)))
    for r in reports:
        print(f"Voice ref ready: {r.ref_wav}")
    return 0


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)

    if args.list_languages:
        print("code\ttier\tfolder\tchatterbox\tdefault_pack_style\ttemplate")
        for code, spec, cb in iter_tier_matrix():
            print(
                f"{code}\t{spec.tier}\t{spec.folder}\t{cb}\t"
                f"{spec.pack_style_default.value}\t{spec.template_source}:{spec.template_tag}"
            )
        return 0

    if args.setup_only:
        try:
            setup_only()
        except SystemDepsError as exc:
            print("\n".join(exc.messages), file=sys.stderr)
            return 2
        rc = _run_fetch(args)
        if rc != 0:
            return rc
        if args.audition or args.fetch_voices:
            pass
        elif not args.draft_phrases and not args.dry_run:
            return 0

    if args.fetch_corpus:
        corpus = args.fetch_corpus.strip().lower()
        if corpus != "sprakbanken-sbr14":
            print(f"Unknown --fetch-corpus {args.fetch_corpus!r}", file=sys.stderr)
            return 2
        fetch_sprakbanken_sbr14()
        return 0

    if args.fetch_voices and not args.setup_only:
        return _run_fetch(args)

    if args.audition:
        try:
            lang_codes = resolve_language_codes(args.lang)
        except ValueError as exc:
            print(exc, file=sys.stderr)
            return 2
        try:
            ensure_system_deps(for_generate=True)
        except SystemDepsError as exc:
            print("\n".join(exc.messages), file=sys.stderr)
            return 2
        for code in lang_codes:
            fetch_voices([code], genders=tuple(_genders(args.gender)))
            summary = run_audition(
                code,
                genders=tuple(_genders(args.gender)),
                device=args.device,
                fmt=args.format,
                seed=args.seed,
            )
            print(f"Audition summary: {summary.summary_path} (device={summary.device})")
        if not args.dry_run:
            return 0

    if args.setup_only and not args.audition and not args.draft_phrases and not args.dry_run:
        return 0

    try:
        lang_codes = resolve_language_codes(args.lang)
    except ValueError as exc:
        print(exc, file=sys.stderr)
        return 2

    if args.apply_review is not None:
        path = apply_review_sheet(args.apply_review)
        print(f"Applied review: {path}")
        return 0

    if args.pack_mode_report:
        for code in lang_codes:
            print(format_mode_report(code))
        return 0

    if args.assembled_audition:
        try:
            ensure_system_deps(for_generate=True)
        except SystemDepsError as exc:
            print("\n".join(exc.messages), file=sys.stderr)
            return 2
        for code in lang_codes:
            result = run_assembled_audition(
                code,
                genders=tuple(_genders(args.gender)),
                device=args.device,
                fmt=args.format,
                seed=args.seed,
                dry_run=args.dry_run,
            )
            print(f"Assembled audition index: {result.index_path} (gap_ms={result.gap_ms})")
        if args.dry_run:
            return 0
        return 0

    if args.review_sheet:
        try:
            ensure_system_deps(for_generate=False)
        except SystemDepsError as exc:
            print("\n".join(exc.messages), file=sys.stderr)
            return 2
        for code in lang_codes:
            path = write_review_sheet(code)
            print(f"Wrote review sheet: {path}")
        return 0

    if args.draft_phrases:
        try:
            ensure_system_deps(for_generate=False)
        except SystemDepsError as exc:
            print("\n".join(exc.messages), file=sys.stderr)
            return 2
        for code in lang_codes:
            path = draft_language(code)
            print(f"Wrote draft phrases: {path}")
        return 0

    if not args.dry_run:
        try:
            ensure_system_deps(for_generate=True)
        except SystemDepsError as exc:
            print("\n".join(exc.messages), file=sys.stderr)
            return 2

    out = args.out or default_out_dir()
    opts = GenerateOptions(
        lang_codes=lang_codes,
        genders=_genders(args.gender),
        out_dir=out,
        device=args.device,
        fmt=args.format,
        mode=args.mode,
        seed=args.seed,
        dry_run=args.dry_run,
        force=args.force,
        allow_unreviewed=args.allow_unreviewed,
        suffix=args.suffix,
        voice_ref_male=args.voice_ref_male,
        voice_ref_female=args.voice_ref_female,
        persona=args.persona,
        persona_checkpoint=args.persona_checkpoint,
        checkpoint_license_confirmed=args.checkpoint_license_confirmed,
    )
    try:
        manifests = generate_packs(opts)
    except SystemExit as exc:
        print(exc, file=sys.stderr)
        return 2

    for m in manifests:
        print(f"Wrote manifest: {m}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())