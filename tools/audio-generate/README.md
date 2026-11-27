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

## Language codes vs pack folders (`voice-guidance.md` §3)

Use **ISO-style codes** in CLI flags and `voices.yaml` (`nb`, `en`, …). Published packs use **folder names** below.

| Code | `build/sounds/` folder | Tier | Chatterbox `language_id` |
|------|------------------------|------|---------------------------|
| en | english | 1 | en |
| fr | french | 1 | fr |
| de | german | 1 | de |
| ru | russian | 1 | ru |
| ar | arabic | 1 | ar |
| zh | chinese | 1 | zh |
| ja | japanese | 1 | ja |
| uk | ukrainian | 1 | uk |
| hi | hindi | 1 | hi |
| nb | norwegian | 2 | no |
| sv | swedish | 2 | sv |
| da | danish | 2 | da |
| fi | finnish | 2 | fi |
| pl | polish | 2 | pl |
| nl | dutch | 2 | nl |
| tr | turkish | 2 | tr |

Aliases: `--lang norwegian` → `nb`, `--lang english` → `en`. Audition output lives under `build/audition/<folder>/…` (not the ISO code).

`navi-voicegen --list-languages` prints the authoritative matrix including Tier 3 codes.

## System dependencies (no sudo from the tool)

Required on the host:

| Tool | Use |
|---|---|
| `ffmpeg` | loudnorm, mono, Opus/MP3 encode |
| `git`, `git-lfs` | sparse template clones |
| `python3` + `venv` | local `.venv` |

If anything is missing, the CLI prints exact install commands for your distro and exits.

## Voice references (curated public sources)

Chatterbox Multilingual needs a short reference wav per voice-actor folder. The
tool downloads **only** clips catalogued in [`voices.yaml`](voices.yaml) — no ad-hoc
scraping.

### Licence rules (hard)

- **Allowed:** public domain, CC0, CC-BY, CC-BY-SA with attribution recorded.
- **Not allowed:** NC/research-only/unclear licences, YouTube/podcasts/commercial TTS
  output, identifiable celebrity/character scraping.
- **Preference order:** Språkbanken / NST studio CC-0 (nb/sv/da when verified on
  [Nasjonalbiblioteket catalogue pages](https://www.nb.no/sprakbanken/en/resource-catalogue/)) →
  LibriVox (PD, **verify archive `language` metadata** — e.g. `norwegischevolksmahrchenii`
  is `deu`, not Norwegian) → Mozilla Common Voice (CC0; **requires account
  terms** — place clips under `cache/commonvoice/<lang>/` manually) →
  LJ Speech (PD, English female; auto-download via `--fetch-voices` when listed in
  `voices.yaml`) → other explicit CC0/CC-BY datasets.
- **Norwegian male default:** Språkbanken **sbr-15** (22 kHz Eastern Bokmål male).
  Place `channel_1/<utterance>.wav` under `cache/sprakbanken/sbr-15/` or drop the
  official archive under `cache/sprakbanken/archives/sbr-15.zip` before `--fetch-voices --lang nb`.
- If no acceptable clip exists for a language/gender, pack generation stops for that
  gender (see `voices.yaml` `status: blocked` when added).

### Fetch and audition

```bash
navi-voicegen --setup-only          # venv, templates, fetch voices.yaml langs
navi-voicegen --fetch-voices --lang en
navi-voicegen --audition --lang en  # build/audition/<folder>/… + SUMMARY.md
navi-voicegen --lang en --gender both --seed 42
```

Downloads land in gitignored `tools/audio-generate/cache/voice-refs/`. Processed
`ref.wav` files are verified against pinned `download_sha256` in `voices.yaml`.

Optional local overrides still work:

```bash
navi-voicegen --voice-ref-male /path/to/male-ref.wav --voice-ref-female /path.wav
```

### Shipping packs

Published packs must ship **`build/sounds/<language>/ATTRIBUTION.txt`** when any
CC-BY voice reference was used. `manifest.json` includes a `voice_provenance` block
per gender (source URL, dataset, speaker, licence).

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

Native review workflow:

```bash
navi-voicegen --draft-phrases --lang nb
navi-voicegen --review-sheet --lang nb   # → build/review/norwegian/review-sheet.tsv
# edit TSV (set reviewed=yes, fix text)
navi-voicegen --apply-review build/review/norwegian/review-sheet.tsv
```

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
--fetch-voices
--audition
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
--review-sheet
--apply-review FILE
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