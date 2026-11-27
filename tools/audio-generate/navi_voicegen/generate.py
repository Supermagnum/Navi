"""Clip pack generation orchestration."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path

from navi_voicegen.deps import resolve_device
from navi_voicegen.engines.chatterbox_engine import (
    ChatterboxUnavailable,
    supported_language_ids,
    synthesize_wav,
)
from navi_voicegen.engines.persona_engine import require_persona_args
from navi_voicegen.languages import SPECS, resolve_pack_style
from navi_voicegen.manifest_out import ClipRecord, sha256_file, write_language_manifest
from navi_voicegen.paths import clip_path, default_out_dir, state_hash_path
from navi_voicegen.phrases import load_phrase_file
from navi_voicegen.postprocess import encode_clip
from navi_voicegen.state import load_state, phrase_inputs_hash, save_state, should_skip
from navi_voicegen.voices.config import language_voice_config
from navi_voicegen.voices.fetch import write_pack_attribution
from navi_voicegen.voices.resolve import resolve_voice_ref_path


DEFAULT_VOICE_REF_DIRS = {
    "male": Path("voice-refs") / "male.wav",
    "female": Path("voice-refs") / "female.wav",
}


@dataclass
class GenerateOptions:
    lang_codes: list[str]
    genders: list[str]
    out_dir: Path
    device: str
    fmt: str
    mode: str
    seed: int | None
    dry_run: bool
    force: bool
    allow_unreviewed: bool
    suffix: str | None
    voice_ref_male: Path | None
    voice_ref_female: Path | None
    persona: bool
    persona_checkpoint: Path | None
    checkpoint_license_confirmed: bool


def _voice_ref_for(lang_code: str, gender: str, opts: GenerateOptions) -> tuple[Path | None, dict | None]:
    override = opts.voice_ref_male if gender == "male" else opts.voice_ref_female
    ref, prov = resolve_voice_ref_path(lang_code, gender, override=override)
    if ref is not None:
        return ref, prov
    if gender == "male":
        default = Path(__file__).resolve().parents[1] / DEFAULT_VOICE_REF_DIRS["male"]
        return (default, None) if default.is_file() else (None, None)
    if gender == "female":
        default = Path(__file__).resolve().parents[1] / DEFAULT_VOICE_REF_DIRS["female"]
        return (default, None) if default.is_file() else (None, None)
    return None, None


def _apply_suffix(text: str, suffix: str | None) -> str:
    if not suffix:
        return text
    return f"{text} {suffix}".strip()


def generate_packs(opts: GenerateOptions) -> list[Path]:
    require_persona_args(
        persona=opts.persona,
        persona_checkpoint=opts.persona_checkpoint,
        license_confirmed=opts.checkpoint_license_confirmed,
    )
    if opts.persona:
        raise SystemExit("Persona engine scaffold only; use default Chatterbox path for turn-by-turn.")

    device = resolve_device(opts.device)
    if device == "cpu":
        print("warning: running Chatterbox on CPU — expect slow synthesis", flush=True)

    manifests: list[Path] = []
    try:
        chatterbox_langs = supported_language_ids()
    except ChatterboxUnavailable as exc:
        if opts.dry_run:
            chatterbox_langs = set()
        else:
            raise SystemExit(str(exc)) from exc

    ext = "opus" if opts.fmt == "opus" else "mp3"

    for lang_code in opts.lang_codes:
        spec = SPECS[lang_code]
        if spec.chatterbox_id is None:
            raise SystemExit(
                f"Chatterbox does not support {lang_code} ({spec.folder}); "
                "no silent engine substitute (voice-guidance.md §8)."
            )
        if not opts.dry_run and spec.chatterbox_id not in chatterbox_langs:
            raise SystemExit(
                f"Installed Chatterbox build does not list language_id={spec.chatterbox_id!r} "
                f"for {lang_code}"
            )

        try:
            phrase_file = load_phrase_file(lang_code)
        except FileNotFoundError:
            raise SystemExit(
                f"Missing tools/audio-generate/phrases/{lang_code}.yaml — "
                f"author en.yaml by hand or run --draft-phrases --lang {lang_code}"
            ) from None

        unreviewed = phrase_file.unreviewed_entries()
        if unreviewed and not phrase_file.reviewed_file and not opts.allow_unreviewed:
            raise SystemExit(
                f"{lang_code} has {len(unreviewed)} unreviewed phrases; "
                "pass --allow-unreviewed to emit clips under unreviewed/"
            )

        pack_style = resolve_pack_style(spec, opts.mode)
        lang_dir = opts.out_dir / spec.folder
        state_path = state_hash_path(opts.out_dir, spec.folder)
        state = load_state(state_path)
        lock = json.loads(
            (Path(__file__).resolve().parents[1] / "lock" / "manifest.json").read_text(encoding="utf-8")
        )
        authoring = {
            "tool": "navi-voicegen",
            "engine": "chatterbox-tts",
            "engine_version": lock["engines"]["chatterbox-tts"]["pypi_version"],
            "device": device,
            "seed": opts.seed,
            "format": opts.fmt,
        }

        all_records: list[ClipRecord] = []
        voice_provenance: dict[str, dict] = {}

        for gender in opts.genders:
            ref, prov = _voice_ref_for(lang_code, gender, opts)
            if ref is None and not opts.dry_run:
                try:
                    lcfg = language_voice_config(lang_code)
                    gcfg = lcfg.male if gender == "male" else lcfg.female
                    blocked = gcfg.blocked_reason if gcfg else None
                except (FileNotFoundError, KeyError):
                    blocked = None
                hint = (
                    f" voices.yaml blocked: {blocked}"
                    if blocked
                    else f" run --fetch-voices --lang {lang_code}"
                )
                raise SystemExit(
                    f"No voice reference for lang={lang_code!r} gender={gender!r}.{hint} "
                    f"Or provide --voice-ref-{gender} PATH."
                )
            if prov:
                voice_provenance[gender] = prov

            entry_tuple = [
                (e.concept_key, e.text, e.reviewed) for e in phrase_file.entries
            ]
            batch_hash = phrase_inputs_hash(
                lang_code=lang_code,
                gender=gender,
                engine_id="chatterbox",
                seed=opts.seed,
                entries=entry_tuple,
                suffix=opts.suffix,
            )

            for entry in phrase_file.entries:
                text = _apply_suffix(entry.text, opts.suffix)
                dest = clip_path(
                    opts.out_dir,
                    spec.folder,
                    gender,
                    entry.concept_key,
                    ext,
                    unreviewed=not entry.reviewed,
                    persona=opts.persona,
                )
                state_key = f"{gender}/{entry.concept_key}"
                if should_skip(state, state_key, batch_hash, dest, force=opts.force):
                    if dest.is_file():
                        rel = dest.relative_to(lang_dir).as_posix()
                        all_records.append(
                            ClipRecord(
                                concept_key=entry.concept_key,
                                gender=gender,
                                relative_path=rel,
                                bytes=dest.stat().st_size,
                                sha256=sha256_file(dest),
                                reviewed=entry.reviewed,
                                phrase_text=text,
                            )
                        )
                    continue

                if opts.dry_run:
                    print(f"DRY-RUN {dest} <- {text!r}")
                    continue

                wav = synthesize_wav(
                    text=text,
                    language_id=spec.chatterbox_id,
                    device=device,
                    voice_ref=ref,
                    seed=opts.seed,
                )
                try:
                    encode_clip(wav, dest, opts.fmt)
                finally:
                    wav.unlink(missing_ok=True)

                rel = dest.relative_to(lang_dir).as_posix()
                rec = ClipRecord(
                    concept_key=entry.concept_key,
                    gender=gender,
                    relative_path=rel,
                    bytes=dest.stat().st_size,
                    sha256=sha256_file(dest),
                    reviewed=entry.reviewed,
                    phrase_text=text,
                )
                all_records.append(rec)
                state.setdefault("clips", {})[state_key] = {"hash": batch_hash, "path": rel}

            state["batch"] = batch_hash
            save_state(state_path, state)

        if opts.dry_run:
            continue

        man = write_language_manifest(
            lang_dir,
            language_folder=spec.folder,
            lang_code=lang_code,
            pack_style=pack_style.value,
            clips=all_records,
            authoring=authoring,
            template_source=phrase_file.template_source,
            template_commit=phrase_file.template_commit,
            voice_provenance=voice_provenance or None,
        )
        if voice_provenance:
            write_pack_attribution(spec.folder, list(voice_provenance.values()), out_dir=opts.out_dir)
        manifests.append(man)
    return manifests