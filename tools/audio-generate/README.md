# navi-voicegen (authoring only)

Offline toolchain to generate **turn-by-turn** static clip packs for Navi
(`sounds/<language>/<voice_actor_gender>/<concept_key>.<ext>`). This does **not**
ship on-device TTS, rodio/cpal playback, or runtime synthesis — see
[`docs/voice-guidance.md`](../../docs/voice-guidance.md).

## Quick start

```bash
cd tools/audio-generate
python3 -m navi_voicegen --setup-only   # venv under .venv/, sparse OSRM/Valhalla cache
source .venv/bin/activate
navi-voicegen --list-languages
navi-voicegen --dry-run --lang en --gender male
```

Default output: repo-root `build/sounds/` (gitignored).

## System dependencies (no sudo from the tool)

Required on the host:

| Tool | Use |
|---|---|
| `ffmpeg` | loudnorm, mono, Opus/MP3 encode |
| `git`, `git-lfs` | sparse template clones |
| `python3` + `venv` | local `.venv` |

If anything is missing, the CLI prints exact install commands for your distro and exits.

## Voice references (rights are your responsibility)

Chatterbox Multilingual uses **your** reference wav for each voice-actor folder:

```bash
navi-voicegen --lang en \
  --voice-ref-male /path/to/male-ref.wav \
  --voice-ref-female /path/to/female-ref.wav \
  --seed 42
```

The tool **never downloads** voice samples. Optional defaults:

- `tools/audio-generate/voice-refs/male.wav`
- `tools/audio-generate/voice-refs/female.wav`

Only use references you have rights to clone/redistribute in generated packs.

## Engines

| Engine | Role |
|---|---|
| **Chatterbox Multilingual** (`chatterbox-tts==0.1.7`, `t3_model=v3`) | Default turn-by-turn synthesis |
| **Style-Bert-VITS2** | Optional **persona** packs only (`--persona` + checkpoint flags) |

Locked versions: [`lock/manifest.json`](lock/manifest.json).

Unsupported Chatterbox languages **fail loudly** — no Piper/RHVoice/eSpeak fallback.

### Persona (Style-Bert-VITS2)

Requires **all** of:

- `--persona`
- `--persona-checkpoint PATH` (local; never auto-downloaded)
- `--checkpoint-license-confirmed` after reading §10a in `voice-guidance.md`

## Phrases

Per-language YAML: [`phrases/<lang>.yaml`](phrases/).

| File | Policy |
|---|---|
| `en.yaml` | Hand-written, `reviewed: true`, full §5 fragment set |
| Other langs | `--draft-phrases` → `reviewed: false` from OSRM/Valhalla cache |

Generation refuses unreviewed YAML unless `--allow-unreviewed` (writes under
`unreviewed/`). Languages with **no** OSRM/Valhalla template refuse drafting.

**Gendered languages:** phrase text uses masculine / gender-neutral imperatives
regardless of `male`/`female` folder (plugins.md / voice-guidance §6).

## Post-processing

ffmpeg two-pass style chain on each clip:

- Trim (source wav length)
- **loudnorm** target **−16 LUFS** integrated (see `lock/manifest.json`)
- Mono
- Default **Opus ~32 kbps** (`--format mp3` → 64 kbps MP3)

## CLI flags

```
--setup-only
--lang CODE|tier1|tier2|tier3|all   (default en)
--list-languages
--gender male|female|both
--mode auto|fragment|whole_phrase|hybrid
--format opus|mp3
--out PATH
--device auto|cpu|cuda
--seed INT
--dry-run
--force
--draft-phrases
--allow-unreviewed
--voice-ref-male PATH
--voice-ref-female PATH
--suffix TEXT
--persona --persona-checkpoint PATH --checkpoint-license-confirmed
```

## Manifest

Each language writes `build/sounds/<language>/manifest.json` with per-clip
`bytes`, `sha256`, `reviewed`, authoring engine metadata, and template commit.

## Tests

```bash
cd tools/audio-generate
python3 -m pip install -e '.[engines]' pytest
pytest
```

## Out of scope

- Bitchin' Betty / alert / overspeed phrase families
- On-device playback crates
- Committing generated audio, models, venvs, or template clones