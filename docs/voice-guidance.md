# Voice / speech guidance (planned plugin)

Turn-by-turn spoken directions for Navi. **Not implemented** — this document is
a design/reference brief only. No `/sounds` tree, crates, or playback code are
required by this doc.

**Plugin candidate:** listed as `voice` / `voice_guidance` in
[`plugins.md`](plugins.md). Host owns audio I/O and clip packs; a WASM guest may
help with pack selection / phrase assembly once HostApi caps exist. Pre-recorded
clip packs are the only audio path.

Status: documentation. Implementation will follow separately after an
audio-backend spike (playback of static clips on Android).

---

## 1. Audio sources

Voice guidance uses **pre-recorded audio clips only**. Clips are authored
offline (see [§4](#4-estimated-storage-recorded-clip-packs)) and shipped as
static files — there is **no** on-device TTS engine, **no** TTS toolchain, and
**no** runtime speech synthesis of any kind.

**Scope boundary:** this plugin covers **turn-by-turn maneuver speech**. It does
**not** own road-sign / children-zone / speed-camera alerts (see
[`plugins/custom-alert-sounds-spec.md`](plugins/custom-alert-sounds-spec.md)),
escalating overspeed nags (see
[`plugins/adaptive-speed-warning-spec.md`](plugins/adaptive-speed-warning-spec.md)),
or **spoken command-and-control** (navigate / save place / nearest POI — see
[`plugins/voice-command.md`](plugins/voice-command.md)). Those may share the
host audio device but use different phrase / clip families. Voice command's
on-device TTS pipeline is also distinct from Chatterbox-authored “Bitchin'
Betty” alert clips (authoring-only; not loaded at runtime).

**Missing-language coverage:** if a selected language pack is incomplete or
absent, fall back to **English recordings** — never to any synthesis engine.

---

## 2. Playback approach

Intended Rust playback stack (decode and play **static** clip files only):

| Layer | Crate / role |
|---|---|
| Playback API | [`rodio`](https://crates.io/crates/rodio) |
| Output | [`cpal`](https://crates.io/crates/cpal) (via rodio) |
| Decode | Symphonia (rodio’s usual decoder path for common formats such as MP3/OGG/Opus) |

**Open risk (implementation spike, not resolved here):** `cpal`’s Android audio
backend must be confirmed to **build and link cleanly** for the target ABI
(e.g. `aarch64-linux-android` / `x86_64-linux-android`) before rodio is relied
on in production. If that spike fails, the host may need an Android-native
playback path (Kotlin `AudioTrack` / ExoPlayer) with the same clip-key API —
still out of scope for this document.

---

## 3. File structure (intended; not created in-repo yet)

Stable layout so a new language or voice is a **folder drop-in** with matching
filenames — no code change to add a pack:

```text
/sounds
  /english
    /male
    /female
  /norwegian
    /male
    /female
  /swedish
    /male
    /female
  ...   (additional languages as added later)
```

`male` / `female` here are **voice-actor** folders only (who recorded /
offline-generated the clips). They are **not** a listener-gender or
grammatical-addressee setting — see
[§6](#6-localization).

**Naming:** each concept uses a predictable key as the basename, e.g.:

```text
left.mp3
right.mp3
roundabout.mp3
in.mp3
two.mp3
hundred.mp3
meters.mp3
```

Exact container (MP3 vs OGG/Opus) can be chosen at implementation time; the
**key** (stem) must stay stable across languages and voice-actor folders. Path
resolution sketch:

```text
sounds/<language>/<voice_actor_gender>/<concept_key>.<ext>
```

---

## 4. Estimated storage (recorded clip packs)

Planning estimates for disk cost of shipped **static** clip packs. Generation
is offline only (Chatterbox Multilingual, Style-Bert-VITS2, or human recording)
— never on-device.

### Authoring paths

| Path | Typical inventory | When to use |
|---|---|---|
| **Fragment-based** | ~50 short clips per gender | Viable for English-style concatenation |
| **Whole pre-composed phrases** | ~150–200 clips per gender | Needed where word order / case / gender agreement breaks naive concat (e.g. Norwegian, Russian, Ukrainian, Arabic) |

Hybrid packs are allowed ([§6](#6-localization)):
concatenate where safe; use composed phrases for awkward templates.

### Per-clip size (recommended codecs)

Rough sizes at a recommended encode:

| Clip kind | Duration (typical) | Size (order of magnitude) |
|---|---|---|
| Fragment | ~1 s | ~8 KB (mono Opus ~32 kbps **or** MP3 64 kbps) |
| Whole phrase | ~2.5 s | ~10–20 KB (same codecs) |

### Per-language total (both genders)

| Pack style | Rough total |
|---|---|
| Fragment-based language | **~0.4–1 MB** |
| Whole-phrase language | **~1.5–8 MB** (depends on codec and phrase count) |

### Scaling

Storage scales roughly **linearly per language**: total for *N* languages is
approximately *N* × (per-language estimate above), plus **~20–30%** overhead for
per-file container / filesystem cost across hundreds of small files.

### Published inventory (measured) — none yet

**Aggregate server space used for Navi voice / language packs today: 0 bytes.**
No voice clip packs are published yet. Per-pack byte sizes are therefore
**unknown** until the first packs are generated and hosted.

Checked 2026-11-01 (do not invent figures beyond this):

| Source | Result |
|---|---|
| Repo tree | No `/sounds` (or equivalent) clip inventory; `compiled/` holds APKs only |
| Docs / manifests | Planning estimates in this section only; no size table with measured bytes |
| `https://navigate-me.duckdns.org/` | Host serves **routing** packs (`current.json` → `/packs/...`); homepage describes graph/POI packs, not voice |
| Probe paths on that host | `sounds/`, `voice/`, `voices/`, `speech/`, `audio/`, `language-packs/`, `language_packs/`, `packs/voice/`, `packs/sounds/` → **404** (or 403 for bare `/packs/`); `current.json` has **no** voice/sound keys |

When packs exist, record **Content-Length** (or manifest `size` fields) per pack and
the **sum** here as the authoritative aggregate. Until then, use the planning
ranges below — clearly labeled as estimates.

### Install model (planned)

Users **choose which voice pack(s) to install** (same consent / Tools download
pattern as region packs / PMTiles). Do **not** assume every language ships in
the APK. Device cost is the sum of installed packs only; server cost is the sum
of all published packs (male + female voice-actor folders, plus any optional
persona packs from [§10a](#10a-optional-persona-voice-packs-whole-voice-alternative)).

### Per-pack sizes (planned languages)

One **voice pack** here means one language with **both** planned voice-actor
folders (`male` + `female`), matching [§3](#3-file-structure-intended-not-created-in-repo-yet).
Measured download size: **not published**. Planning range from the tables above:

| Language | Tier | Measured size | Planning estimate (both genders) |
|---|---|---|---|
| English | 1 | not published | ~0.4–1 MB (fragment) or ~1.5–8 MB (whole-phrase) |
| French | 1 | not published | same range |
| German | 1 | not published | same range |
| Russian | 1 | not published | prefer whole-phrase end of range |
| Arabic | 1 | not published | prefer whole-phrase end of range |
| Chinese (Mandarin / Simplified) | 1 | not published | same range; confirm phrase inventory |
| Japanese | 1 | not published | same range; confirm phrase inventory |
| Ukrainian | 1 | not published | prefer whole-phrase end of range |
| Hindi | 1 | not published | same range; confirm phrase inventory |
| Norwegian | 2 | not published | prefer whole-phrase end of range |
| Swedish | 2 | not published | prefer whole-phrase end of range |
| Danish | 2 | not published | same range |
| Finnish | 2 | not published | prefer whole-phrase end of range |
| Polish | 2 | not published | prefer whole-phrase end of range |
| Dutch | 2 | not published | same range |
| Turkish | 2 | not published | same range |
| Icelandic | 3 | not published | same range; research in [§8.1](#81-tier-3-offline-tts--dataset-research) |
| Greek | 3 | not published | same range |
| Swahili | 3 | not published | same range |
| Malay | 3 | not published | same range |
| Czech | 3 | not published | prefer whole-phrase end of range |
| Hungarian | 3 | not published | prefer whole-phrase end of range |
| Romanian | 3 | not published | prefer whole-phrase end of range |
| Vietnamese | 3 | not published | same range |
| Latvian | 3 | not published | prefer whole-phrase end of range |
| Serbian | 3 | not published | prefer whole-phrase end of range |
| Georgian | 3 | not published | prefer whole-phrase end of range |
| Kazakh | 3 | not published | prefer whole-phrase end of range |
| Nepali | 3 | not published | same range |

Optional persona packs ([§10a](#10a-optional-persona-voice-packs-whole-voice-alternative))
would add roughly **another full language-pack size each** if published; none
exist yet.

### Planning aggregate (all listed languages, not measured)

If **all 29** languages above were published once each (both genders, no
persona packs), planning totals using [§4](#4-estimated-storage-recorded-clip-packs)
ranges:

| Assumption | Aggregate (order of magnitude) |
|---|---|
| All fragment-style (~0.4–1 MB each) | **~12–29 MB** |
| All whole-phrase (~1.5–8 MB each) | **~44–232 MB** |
| Mixed (typical) | somewhere between those bands, plus ~20–30% small-file overhead |

These are **not** server measurements. Replace with a real sum when packs land
on the host.

### Caveats

These are **pre-generation estimates for planning only**. Actual figures depend
on the finalized fragment / phrase inventory ([§5](#5-required-word--phrase-fragment-list)–[§6](#6-localization))
and codec choice, and should be **re-measured** once the first real language
pack is generated.

This footprint is separate from — and much smaller than — any on-device TTS
model file would have been. That size (and complexity) gap is part of why TTS
was dropped as an option ([§1](#1-audio-sources)), not only a licensing
simplification.

---

## 5. Required word / phrase fragment list

Minimum fragment set **per** `language` / voice-actor folder. Filenames should
map 1:1 to these concepts (examples in parentheses).

### Maneuvers

| Concept | Example key |
|---|---|
| Roundabout | `roundabout` |
| First / second / third (exit ordinal) | `first`, `second`, `third` |
| Cross / crossing | `cross` or `crossing` |
| Left | `left` |
| Right | `right` |
| U-turn | `u_turn` |
| Straight / continue | `straight` or `continue` |
| Exit | `exit` |
| Merge | `merge` |
| Keep left | `keep_left` |
| Keep right | `keep_right` |
| Arrive | `arrive` |
| Destination | `destination` |

### Numbers

| Concept | Notes |
|---|---|
| Digits / number words | `one` … enough coverage for full distance announcements (at least through the digit set needed for hundreds/thousands assembly) |
| Hundred | `hundred` |
| Thousand | `thousand` |

Exact digit inventory (e.g. whether `zero` / teens / tens are separate clips)
is finalized with phrase-assembly design.

### Connectors

| Concept | Role |
|---|---|
| Units | `meters`, `kilometers` (and locale-specific unit names as needed) |
| Linking words | e.g. `in`, `then` — enough to assemble “in N units, then maneuver” |

The **exact** connector set is deliberately open: finalize once phrase-assembly
logic is designed (and after the per-language grammar decision in
[§6](#6-localization)).

---

## 6. Localization

Turn-by-turn **spoken** localization is still **design-only** (no `/sounds`
packs, no `voice_speak` host path, no voice language setting in `app_config`).
This section records what exists today and the locked recommendations for when
packs are authored.

### What exists today (checked 2026-11-01)

| Concern | Reality in Navi |
|---|---|
| **Voice guidance plugin** | Spec / docs only ([`plugins.md`](plugins.md) §6). No clip tree, no rodio/cpal playback wired for guidance. |
| **Android / OS TTS for maneuvers** | **Not used** and **not planned** for this feature ([§1](#1-audio-sources)). Conversational TTS belongs to the separate [voice-command](plugins/voice-command.md) sandbox. |
| **UI locale** | Per-app Android locales `en` + `nb` (`locales_config.xml`). A minority of strings use `stringResource` (approach off-route / exit ordinals / CAT / via-point, etc.); most Compose UI remains English literals. Device locale drives those resources and place-search name formatting (`setPlaceSearchLocale`). The WASM `i18n` / `ui_locale` pack plugin is **not** implemented ([`i18n-translation-spec.md`](plugins/i18n-translation-spec.md)). |
| **Written approach UI** | Implemented: icon + formatted distance + next-street (and roundabout exit ordinal strings). **Not** full OSRM/Valhalla sentence templates. Shared `NavGuidance` state is documented as the future voice trigger bus ([`approach-instructions.md`](approach-instructions.md) §5). Ferrostar SDK is **not** wired. |
| **Offline voice packs** | **None** published or downloadable ([§4](#4-estimated-storage-recorded-clip-packs)). |

UI string catalogs and spoken clip packs are **different products**. The i18n
spec already states they may share a preference later but are not the same
files.

### Recommendation: UI locale vs voice pack language

**Keep them independent settings** when voice ships:

| Option | Behavior | Tradeoff |
|---|---|---|
| **A — Independent (recommended)** | Persist `voice_language` (+ voice-actor gender) beside Drive settings. On first enable, **default** the voice pack to the device locale if that pack is installed, else English. User may keep English UI and Norwegian voice (or the reverse). | Slightly more settings surface; matches “UI packs ≠ voice packs.” |
| **B — Always mirror UI / OS locale** | Voice language locked to `ui_locale` or device locale. | Simpler UX; breaks bilingual drivers and anyone whose preferred spoken language differs from menus. |

Do **not** infer voice language from GPS or SIM country (same rule as UI
locale). Missing or incomplete packs fall back to **English recordings**, never
to Android TTS ([§1](#1-audio-sources)).

### Recommendation: fragment concat vs whole phrases

Naive **word-by-word concatenation** (e.g. `in` + `two` + `hundred` + `meters` +
`left`) can sound acceptable in English but is **not guaranteed** elsewhere
(number formation, word order, case / gender agreement).

**Locked process (still per language at authoring time):**

1. Start from vetted phrase wording ([§7](#7-phrase-translation-source)).
2. Have a **native speaker** review concatenated fragments for that language
   (and each voice-actor folder if prosody differs).
3. If concatenation fails naturalness or grammar, ship **whole pre-composed
   phrase recordings** (e.g. one clip: “om to hundre meter, sving til venstre”).
4. Hybrid packs are allowed: concatenate where safe; use composed phrases for
   awkward distance+maneuver templates.

**Default pack-style bias (until native review flips it):**

| Bias | Languages (from [§8](#8-language-priority)) |
|---|---|
| Prefer **fragments** first | English (and other languages that pass native concat review) |
| Prefer **whole-phrase / hybrid** first | Norwegian, Swedish, Finnish, Russian, Ukrainian, Polish, Czech, Serbian, Arabic, and others with heavy agreement / free word order called out in [§4](#4-estimated-storage-recorded-clip-packs) |

Per-language **sign-off** (which keys are fragments vs whole phrases) remains
open until the first pack for that language is recorded and reviewed. Do not
assume Nordic packs match the English fragment model without that review.

### Voice-actor gender vs grammatical addressee gender

These are **separate axes**. Do not conflate them:

| Axis | What it means | Navi today |
|---|---|---|
| **Voice-actor gender** | Who speaks: the `male` / `female` sound folders (timbre / recorded or offline-generated voice). | Planned user-selectable pack path; **not** implemented yet. |
| **Grammatical addressee gender** | How some languages conjugate imperatives and other forms by the **gender of the person being addressed**, not the speaker (e.g. Arabic). | **No listener-gender setting.** |

**Default for grammatically gendered languages:** always use the
**masculine / gender-neutral imperative** (and matching agreement) in phrase
templates and recordings, **regardless** of which voice-actor folder
(`male` / `female`) plays the clip. That matches Google Maps and most
commercial nav apps.

**Deferred:** if a future **listener-gender** (addressee) setting is added,
revisit feminine/other imperative variants per language. Until then, do not
derive grammatical forms from the voice-actor folder name.

---

## 7. Phrase translation source

Full navigation phrases that combine distance and maneuver — e.g. *“in 100
meters, take the second exit at the roundabout”* — must **not** be generated
from raw machine translation of English templates, and must **not** be spoken
by synthesizing those strings at runtime (clips only — [§1](#1-audio-sources)).

### Role in Navi

OSRM / Valhalla templates are an **authoring-time wording source** for:

- scripts used to record or offline-generate whole-phrase clips;
- deciding fragment / connector wording where concat packs are viable;
- keeping spoken phrasing aligned with industry turn-by-turn conventions.

They are **not** embedded engines in the APK today, and Navi’s on-screen
approach box does **not** render these full sentences (icon + distance + street
name only — [`approach-instructions.md`](approach-instructions.md)).

### Preferred sources

| Source | License / notes | Checked inventory (2026-11-01) |
|---|---|---|
| [OSRM Text Instructions](https://github.com/Project-OSRM/osrm-text-instructions) `languages/translations/` | **BSD-2-Clause**; Transifex community translations; Slavic grammar-case handling | `ar`, `ca`, `da`, `de`, `en`, `eo`, `es` / `es-ES`, `fi`, `fr`, `he`, `hu`, `id`, `it`, `ja`, `ko`, `my`, `nl`, `no`, `pl`, `pt-BR` / `pt-PT`, `ro`, `ru`, `sl`, `sv`, `tr`, `uk`, `vi`, `yo`, `zh-Hans` |
| [Valhalla `locales/`](https://github.com/valhalla/valhalla/tree/master/locales) | Human-reviewed locale JSON (+ `.po`) in-tree | Among others: `ar-SA`, `cs-CZ`, `da-DK`, `de-DE`, `el-GR`, `en-US` / `en-GB` / `en-AU`, `fi-FI`, `fr-FR`, `hi-IN`, `hu-HU`, `ja-JP`, `nb-NO`, `nl-NL`, `pl-PL`, `ro-RO`, `ru-RU`, `sv-SE`, `tr-TR`, `uk-UA`, `vi-VN` — **no** `zh-*`, **no** Icelandic / Swahili / Malay / Latvian / Serbian / Georgian / Kazakh / Nepali |

### Recommendation (source selection)

1. **Primary:** use OSRM and/or Valhalla templates for any language present in at
   least one catalog. Prefer the locale tag that matches the voice pack
   (e.g. Valhalla `nb-NO` over OSRM’s broader `no` for Norwegian Bokmål packs;
   OSRM `zh-Hans` for Mandarin Simplified — Valhalla has no Chinese locale;
   Valhalla `hi-IN` for Hindi — OSRM has no Hindi file).
2. **When both exist:** either catalog is acceptable; pick one per language for
   consistency, and note the choice in the pack’s authoring notes. Prefer the
   more specific BCP 47 tag when they disagree.
3. **Fallback:** only use raw MT for languages **absent from both** sources
   (several Tier 3 languages — see matrix below). Flag MT-sourced languages as
   lower-confidence and pending native-speaker review ([§8](#8-language-priority)).

### Coverage vs Navi priority list

| Language | OSRM | Valhalla | Authoring source (recommendation) |
|---|---|---|---|
| English | yes (`en`) | yes (`en-US` / `en-GB` / …) | Either |
| French | yes | yes (`fr-FR`) | Either |
| German | yes | yes (`de-DE`) | Either |
| Russian | yes | yes (`ru-RU`) | Either (prefer whole-phrase clips) |
| Arabic | yes (`ar`) | yes (`ar-SA`) | Either; MSA + masculine/neutral imperatives ([§6](#6-localization)) |
| Chinese (Simplified) | yes (`zh-Hans`) | **no** | OSRM |
| Japanese | yes | yes (`ja-JP`) | Either |
| Ukrainian | yes (`uk`) | yes (`uk-UA`) | Either (prefer whole-phrase) |
| Hindi | **no** | yes (`hi-IN`) | Valhalla |
| Norwegian | yes (`no`) | yes (`nb-NO`) | Prefer Valhalla `nb-NO` for Bokmål packs |
| Swedish | yes | yes (`sv-SE`) | Either |
| Danish | yes | yes (`da-DK`) | Either |
| Finnish | yes | yes (`fi-FI`) | Either (prefer whole-phrase / hybrid) |
| Polish | yes | yes (`pl-PL`) | Either (prefer whole-phrase) |
| Dutch | yes | yes (`nl-NL`) | Either |
| Turkish | yes | yes (`tr-TR`) | Either |
| Hungarian | yes | yes (`hu-HU`) | Either |
| Romanian | yes | yes (`ro-RO`) | Either |
| Vietnamese | yes | yes (`vi-VN`) | Either |
| Czech | **no** | yes (`cs-CZ`) | Valhalla |
| Greek | **no** | yes (`el-GR`) | Valhalla |
| Icelandic, Swahili, Malay, Latvian, Serbian, Georgian, Kazakh, Nepali | **no** | **no** | **MT + native review** (or skip until a vetted template appears) |

**Still open per pack:** which exact template keys map onto Navi’s fragment /
whole-phrase inventory ([§5](#5-required-word--phrase-fragment-list)), and the
native-speaker sign-off that freezes wording before recording.

---

## 8. Language priority

Build order for **recorded** voice packs. Catalog presence for Tier 1–3 is
summarized in [§7](#7-phrase-translation-source); still run a native-speaker
pass on the chosen templates before recording, and do not treat “present in
OSRM/Valhalla” as a finished Navi pack.

### Tier 1 (major — build first)

English, French, German, Russian, Arabic, Chinese, Japanese, Ukrainian, Hindi.

| Language | Notes |
|---|---|
| **Arabic** | Target **MSA (Modern Standard Arabic)** register for nav phrasing. Classical Arabic is **not** the target. Apply masculine/gender-neutral imperatives per [§6](#6-localization). |
| **Chinese** | Target the most common variant: **Mandarin in Simplified script** (`zh-CN` / Putonghua). Traditional Chinese (Taiwan / Hong Kong) and other Sinitic varieties are out of scope for the first pack unless later prioritized separately. Template source: OSRM `zh-Hans` (Valhalla has no Chinese locale) — [§7](#7-phrase-translation-source). |
| **Japanese** | Templates present in both OSRM and Valhalla ([§7](#7-phrase-translation-source)); still requires native review before recording. |
| **Ukrainian** | Prefer offline-generated or human recordings for the clip pack. Templates exist in OSRM/Valhalla; **not** equal-effort to the rest of Tier 1 until voice inventory + native review are done. |
| **Hindi** | Template source: Valhalla `hi-IN` (absent from OSRM) — [§7](#7-phrase-translation-source); native review before recording. |

### Tier 2 (smaller — ranked by confirmed translation-template quality, best first)

Norwegian, Swedish, Danish, Finnish, Polish, Dutch, Turkish.

Tier 2 languages in this list all have at least one OSRM or Valhalla template
file ([§7](#7-phrase-translation-source)). Still require native review before
recording; prefer Valhalla `nb-NO` for Norwegian Bokmål packs.

### Tier 3 (smaller — unconfirmed resources; research required before ranking or building)

Icelandic, Greek, Swahili, Malay, Czech, Hungarian, Romanian, Vietnamese,
Latvian, Serbian, Georgian, Kazakh, Nepali.

**Icelandic:** no confirmed vetted phrase-template source — treat as **lowest
priority** pending dedicated research. Offline TTS/dataset findings for the
whole tier are in [§8.1](#81-tier-3-offline-tts--dataset-research).

### 8.1 Tier 3 offline TTS / dataset research

Checked 2026-11-01 against public FOSS catalogs. This is **authoring research**
only: Navi still ships **pre-recorded clip packs** at runtime
([§1](#1-audio-sources)) — Piper / RHVoice / eSpeak-NG / Coqui are **not**
bundled for on-device synthesis.

**Legend**

| Label | Meaning |
|---|---|
| **yes** | Confirmed downloadable offline voice **model** (or engine voice pack) for that language |
| **partial** | Engine lists the language and/or a speech **dataset** exists, but no strong neural pack — or only robotic formant TTS |
| **no** | No confirmed public offline TTS pack/model found for that language in the sources below |
| *Navi pack* | Always **not shipped** today for every Tier 3 language (separate from upstream availability) |

**Primary catalogs**

| Project | Link | Role |
|---|---|---|
| Piper voices | [VOICES.md](https://github.com/rhasspy/piper/blob/master/VOICES.md) / [rhasspy/piper-voices](https://huggingface.co/rhasspy/piper-voices) | Offline neural TTS models (MIT engine; voice licenses vary — verify per voice) |
| RHVoice | [Languages](https://rhvoice.org/languages/) | Offline voices; GPLv3-family stack |
| eSpeak-NG | [languages.md](https://github.com/espeak-ng/espeak-ng/blob/master/docs/languages.md) | Compact formant TTS; all Tier 3 codes present; quality typically robotic |
| Coqui XTTS-v2 | [docs](https://docs.coqui.ai/en/latest/models/xtts.html) / [HF](https://huggingface.co/coqui/XTTS-v2) | Multilingual offline model; **cs** and **hu** among Tier 3; Coqui project archived — license/redistribution review required |
| Mozilla Common Voice | [Data Collective](https://mozilladatacollective.com/organization/cmfh0j9o10006ns07jq45h7xk) | CC0 speech datasets for training / authoring — **not** a ready Navi clip pack |

**Per language**

| Language | Available? | Project + link | Note |
|---|---|---|---|
| Icelandic | **yes** (model); Navi pack: no | Piper `is_IS` (bui, salka, steinn, ugla) — [VOICES.md](https://github.com/rhasspy/piper/blob/master/VOICES.md); eSpeak-NG `is`; Common Voice Scripted Speech 27.0 `is` (~15 MB corpus listing) | Strong Piper coverage. CV corpus is small. Phrase templates for nav still unconfirmed ([§8](#8-language-priority)). |
| Greek | **yes** (model); Navi pack: no | Piper `el_GR` rapunzelina — HF `el/`; eSpeak-NG `el`; Common Voice `el` | Confirmed Piper pack. RHVoice: not listed. |
| Swahili | **yes** (model); Navi pack: no | Piper `sw_CD` lanfrica — HF `sw/`; eSpeak-NG `sw`; Common Voice Scripted Speech `sw` (large) | Confirmed Piper pack. RHVoice: not listed. |
| Malay | **partial**; Navi pack: no | eSpeak-NG `ms`; Common Voice Scripted Speech `ms` + spontaneous Bahasa Malay / Sabah Malay; Piper HF path `ms/` → **404** (no Malay voice; Malayalam `ml_IN` is a different language). HF discussion: [Support Malaysia language](https://huggingface.co/rhasspy/piper-voices/discussions/65) | No confirmed Piper/RHVoice/Coqui Malay neural pack. Dataset + formant TTS only. |
| Czech | **yes** (model); Navi pack: no | Piper `cs_CZ` jirka; RHVoice [Czech](https://rhvoice.org/cs-voices/); Coqui XTTS-v2 `cs`; eSpeak-NG `cs`; Common Voice `cs` | Multiple offline engines. Prefer whole-phrase nav clips for Slavic case. |
| Hungarian | **yes** (model); Navi pack: no | Piper `hu_HU` (anna, berta, imre); Coqui XTTS-v2 `hu`; eSpeak-NG `hu`; Common Voice `hu` | No RHVoice listing. Strong Piper + XTTS. |
| Romanian | **yes** (model); Navi pack: no | Piper `ro_RO` mihai; RHVoice [Romanian](https://rhvoice.org/ro-voices/); eSpeak-NG `ro`; Common Voice `ro` | Confirmed Piper + RHVoice. |
| Vietnamese | **yes** (model); Navi pack: no | Piper `vi_VN` (25hours_single, vais1000, vivos); RHVoice [Southern Vietnamese](https://louderpages.org/vi-vu) (LouderPages); eSpeak-NG `vi`; Common Voice `vi` | Confirmed neural packs. Dialect (Northern vs Southern) must be chosen deliberately. |
| Latvian | **yes** (model); Navi pack: no | Piper `lv_LV` aivars; eSpeak-NG `lv`; Common Voice `lv` | Confirmed Piper. RHVoice: not listed. |
| Serbian | **yes** (model); Navi pack: no | Piper `sr_RS` serbski_institut; RHVoice [Serbian](https://rhvoice.org/sr-voices/); eSpeak-NG `sr`; Common Voice `sr` | Confirmed Piper + RHVoice. Prefer whole-phrase for Slavic grammar. |
| Georgian | **yes** (model); Navi pack: no | Piper `ka_GE` natia; RHVoice [Georgian](https://rhvoice.org/ka-voices/) Natia; eSpeak-NG `ka`; Common Voice `ka` | Same “Natia” name appears in Piper and RHVoice lineages — verify license/source before shipping derived clips. |
| Kazakh | **yes** (model); Navi pack: no | Piper `kk_KZ` (iseke, issai, raya); eSpeak-NG `kk`; Common Voice `kk` | Confirmed Piper. RHVoice: not listed (Kyrgyz is separate). |
| Nepali | **yes** (model); Navi pack: no | Piper `ne_NP` (chitwan, google); RHVoice via [LouderPages Dina](https://louderpages.org/dina); eSpeak-NG `ne`; Common Voice `ne-NP` | Confirmed Piper + RHVoice community voice. Check per-voice commercial/redistribution terms. |

**Summary:** twelve of thirteen Tier 3 languages have at least one **confirmed**
public offline neural TTS voice (mostly Piper). **Malay** is the clear gap
(dataset + eSpeak-NG only). None of these are Navi clip packs yet — upstream
availability only lowers authoring risk; phrase templates ([§7](#7-phrase-translation-source))
and native review still gate shipping.

---

## 9. Integration points (documented, not built)

| Concern | Plan |
|---|---|
| **Trigger source** | Ferrostar’s navigation state machine already tracks distance-to-next-maneuver and maneuver type — voice prompts should fire from that state (or an equivalent Navi nav-state layer if Ferrostar is not wired yet). |
| **Audio vs background compute** | Existing design: background routing/compute must not stutter concurrent music. Spoken guidance is a **legitimate foreground interruption** (like any nav app’s directions) and does **not** need to defer to background music the way silent T3/T4 work does. Still avoid starving UI; duck or pause media per platform norms if desired later. |
| **User settings** | Mute / volume for guidance; persisted **language**, **voice-actor gender**, and (if shipped) **persona voice pack** ([§10a](#10a-optional-persona-voice-packs-whole-voice-alternative)); optional **persona suffix** ([§10](#10-persona-sentence-ending-suffixes-optional)). **Pack install:** user chooses which language/voice packs to download ([§4](#4-estimated-storage-recorded-clip-packs)). No listener/addressee-gender setting yet ([§6](#6-localization)). Store with other Drive settings ([README settings](../README.md#settings) / SQLite `app_config`). |
| **Offline** | Installed recorded packs must work fully offline. There is no TTS model to download or cache at runtime. |

---

## 10. Persona sentence-ending suffixes (optional)

Optional **presentation-only** personalization for spoken guidance: a configurable
word or short phrase appended to the end of assembled spoken instructions — for
example `"master,"` or `"nya,"` — for users who want a more playful or
anime-inspired voice character (catgirl/catboy-style speech patterns being the
concrete example driving this request).

This is **only** a trailing suffix on otherwise normal guidance phrases. It is
**not** a full alternate voice pack. Whole-voice persona packs (every clip
re-voiced in a stylized character voice) are a separate, opt-in feature —
see [§10a](#10a-optional-persona-voice-packs-whole-voice-alternative).

### Rules

| Concern | Spec |
|---|---|
| Scope | Pure presentation layer on top of the existing fragment / phrase system. Does **not** change navigation logic, maneuver data, distance triggers, or timing. |
| Default | **Off** — empty suffix. Default voice packs and phrasing are unchanged unless the user deliberately enables a suffix. |
| Config | User-chosen string (or empty). Stored with other voice settings alongside language / voice-actor gender ([§9](#9-integration-points-documented-not-built)). |
| Concatenation | The same per-language caveat as [§6](#6-localization) applies: naive append of a trailing suffix to a concatenated phrase may sound unnatural in some languages. Review per language; prefer whole-phrase packs where needed. |

### Playback

Persona suffixes use **pre-recorded clips only**, same as the default packs.
Apply by either appending a separate short recorded clip per persona-suffix
option after the assembled instruction, or recording whole phrases that already
include the suffix. Choose at implementation time. Keep persona clips clearly
separate from the default voice pack so the default experience stays unaffected
when the setting is empty/off. There is no live TTS path for suffixes.

---

## 10a. Optional persona voice packs (whole-voice alternative)

Distinct from the [sentence-ending suffix](#10-persona-sentence-ending-suffixes-optional)
above, this is a **full alternate voice pack**: an entire language pack’s clips
(per [§3](#3-file-structure-intended-not-created-in-repo-yet)’s folder structure)
voiced in a stylized character voice instead of a neutral human voice.

**Candidate authoring tools** (offline generation only):
[Style-Bert-VITS2](https://github.com/litagin02/Style-Bert-VITS2) and/or
Chatterbox Multilingual — same pipeline as default packs. Style-Bert-VITS2 can
produce English / Norwegian navigation phrases with a Japanese-accented delivery
(the model’s Japanese phonemizer naturally produces this effect when fed
non-Japanese text — this is the **intended** effect, not a defect to fix).

**Generation is authoring-only**: generate offline, ship as static recorded
clips under a new persona subfolder (e.g. `sounds/<language>/persona-anime/`).
Persona packs are **never** synthesized on-device — same rule as default packs
([§1](#1-audio-sources)).

### Licensing

Two separate things to verify, **both** required before shipping any persona
pack, given Navi is free / distributed:

1. **Engine license:** Style-Bert-VITS2 itself is **AGPL-3.0** (the
   `text/user_dict/` module is **LGPL-3.0**). AGPL permits commercial and free
   use but carries a source-disclosure obligation for the combined work if
   distributed — fold authoring-tool AGPL into the project’s open licensing
   decision tracked for GPL assets in [`icons.md`](icons.md) (Navit icon set);
   do **not** treat Style-Bert-VITS2 as a separate, already-settled question.
   Offline authoring that never ships the engine binary is a different packaging
   question from bundling GPL runtime assets — still verify before shipping
   generated clips that derive from AGPL tooling if redistribution terms apply.
2. **Voice-model license:** the trained checkpoint used for the persona voice
   must be verified **independently** of the engine license. Many
   community-shared Style-Bert-VITS2 checkpoints are either marked “research
   only, not for commercial use,” or are trained on copyrighted anime-character
   voice-actor performances without any rights clearance — using either is
   **not** safe for a shipped product regardless of what the uploader’s README
   claims. Only use a checkpoint with a verified, explicit free-for-any-use
   license and rights-clear training data (e.g. a model trained on the
   creator’s own recorded voice with an explicit permissive license statement).

**Default: off.** Persona voice packs are strictly opt-in and **additive** to
the default neutral voice packs, never a replacement.

No code, folder tree, or crates are introduced by this section — planning only,
consistent with this file’s documentation-not-built status.

---

## Related docs

| Doc | Relevance |
|---|---|
| [`architecture.md`](architecture.md) | Thread tiers (T2 UI/audio); keep guidance off the routing pool |
| [`android-build.md`](android-build.md) | ABI / NDK constraints for any native clip-playback spike |
| [`icons.md`](icons.md) | GPL bundling for the Navit icon set; AGPL considerations for offline Style-Bert-VITS2 authoring if applicable |
| [`plugins.md`](plugins.md) | Optional future: voice as `voice_guidance` plugin ([§6](plugins.md#6-voice-guidance-voice--voice_guidance)); voice command is [§16](plugins.md#16-voice-command-voice_command--voice_cmd) |
| [`plugins/voice-command.md`](plugins/voice-command.md) | Conversational navigate/save/POI — separate sandbox; not maneuver clips |
| [`plugins/adaptive-speed-warning-spec.md`](plugins/adaptive-speed-warning-spec.md) | Spoken overspeed tiers reuse `voice_speak` / the same playback stack |
| [`plugins/custom-alert-sounds-spec.md`](plugins/custom-alert-sounds-spec.md) | Road-sign / camera / children-zone **tones** — not turn-by-turn phrases |
| [`plugins/instrument-cluster-agl-spec.md`](plugins/instrument-cluster-agl-spec.md) | Exports the same maneuver + warning state to clusters (no audio) |

## Status checklist

| Item | Status |
|---|---|
| Recorded-voice folder contract | Specified here; tree not created |
| Fragment key list | Minimum set documented; connectors TBD with assembly |
| Per-language concat vs whole phrases | Process + default bias locked in [§6](#6-localization); per-language native sign-off still **open** until first pack |
| UI locale vs voice pack language | Independent settings recommended ([§6](#6-localization)); neither voice setting implemented yet |
| Estimated clip-pack storage | Planning figures + install model in [§4](#4-estimated-storage-recorded-clip-packs); **measured server aggregate = 0** (no packs published); re-measure after first pack |
| Per-pack measured sizes (user-selectable installs) | Table stub in [§4](#4-estimated-storage-recorded-clip-packs); all “not published” until hosted |
| Phrase template source (OSRM/Valhalla vs MT-fallback) | Source policy + coverage matrix in [§7](#7-phrase-translation-source); per-pack key mapping / native sign-off still **open** |
| Voice-actor vs addressee gender (default masculine/neutral; listener setting) | Specified default; listener-gender setting **deferred** |
| Language priority tiers | Specified here ([§8](#8-language-priority)); template catalog presence recorded in [§7](#7-phrase-translation-source) |
| Tier 3 offline TTS / dataset research | Documented in [§8.1](#81-tier-3-offline-tts--dataset-research) (2026-11-01); Malay neural gap noted |
| Persona sentence-ending suffixes | Specified here (optional, off by default); not implemented |
| Persona voice packs (whole-voice) | Specified here ([§10a](#10a-optional-persona-voice-packs-whole-voice-alternative); optional, off by default); not implemented — engine + checkpoint licenses both **open** |
| rodio / cpal on Android (static clip playback) | **Spike required** |
| Implementation / crates in workspace | **Not started** |
