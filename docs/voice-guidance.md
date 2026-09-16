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
[§6](#6-localization--open-design-question).

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

Hybrid packs are allowed ([§6](#6-localization--open-design-question)):
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

### Caveats

These are **pre-generation estimates for planning only**. Actual figures depend
on the finalized fragment / phrase inventory ([§5](#5-required-word--phrase-fragment-list)–[§6](#6-localization--open-design-question))
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
[§6](#6-localization--open-design-question)).

---

## 6. Localization — open design question

Naive **word-by-word concatenation** (e.g. `in` + `two` + `hundred` + `meters` +
`left`) can sound acceptable in English but is **not guaranteed** to be
grammatically correct or natural in Norwegian, Swedish, or other languages with
different number formation, word order, or grammatical case/gender agreement.

**Do not assume one assembly strategy for all languages.** When the feature is
built, decide **per language**:

1. Have a **native speaker** review concatenated fragments for that language
   (and for each voice-actor pack if prosody differs).
2. If concatenation fails naturalness or grammar, prefer **whole pre-composed
   phrase recordings** for that language (e.g. one clip: “om to hundre meter,
   sving til venstre”) instead of forcing fragment assembly.
3. Hybrid packs are allowed: concatenate where safe; use composed phrases for
   awkward distance+maneuver templates.

This remains **unresolved per language** until implementation + linguistic
review. English may start with fragments; Nordic packs must not be assumed to
follow the same model without confirmation.

### Voice-actor gender vs grammatical addressee gender

These are **separate axes**. Do not conflate them:

| Axis | What it means | Navi today |
|---|---|---|
| **Voice-actor gender** | Who speaks: the `male` / `female` sound folders (timbre / recorded or offline-generated voice). | User-selectable pack path. |
| **Grammatical addressee gender** | How some languages conjugate imperatives and other forms by the **gender of the person being addressed**, not the speaker (e.g. Arabic). | **No listener-gender setting.** |

**Default for grammatically gendered languages:** always use the
**masculine / gender-neutral imperative** (and matching agreement) in phrase
templates and recordings, **regardless** of which voice-actor folder
(`male` / `female`) plays the clip. That matches Google Maps and most
commercial nav apps.

**Open item:** if a future **listener-gender** (addressee) setting is added,
revisit feminine/other imperative variants per language. Until then, do not
derive grammatical forms from the voice-actor folder name.

---

## 7. Phrase translation source

Full navigation phrases that combine distance and maneuver — e.g. *“in 100
meters, take the second exit at the roundabout”* — must **not** be generated
from raw machine translation of English templates.

**Preferred sources** (human-reviewed, purpose-built for turn-by-turn phrasing):

| Source | Notes |
|---|---|
| [OSRM Text Instructions](https://github.com/Project-OSRM/osrm-text-instructions) | **BSD-2-Clause**; community-translated via Transifex; dedicated grammar-case handling for Slavic languages. |
| [Valhalla `locales/`](https://github.com/valhalla/valhalla) | Human-reviewed locale templates in-tree. |

Use OSRM and/or Valhalla phrase templates as the primary translation source for
spoken guidance strings (and as the wording basis for whole-phrase recordings
where packs use composed clips).

**Fallback:** only use raw MT for languages **absent from both** sources. Any
MT-sourced language must be **explicitly flagged** as lower-confidence and
pending native-speaker review (see [§8](#8-language-priority) and the status
checklist).

---

## 8. Language priority

Build order for **recorded** voice packs. **Confirm** OSRM Text Instructions
and/or Valhalla `locales/` coverage before treating a language as
vetted-phrase-ready — do not assume presence from this list alone.

### Tier 1 (major — build first)

English, French, German, Russian, Arabic, Chinese, Japanese, Ukrainian, Hindi.

| Language | Notes |
|---|---|
| **Arabic** | Target **MSA (Modern Standard Arabic)** register for nav phrasing. Classical Arabic is **not** the target. Apply masculine/gender-neutral imperatives per [§6](#6-localization--open-design-question). |
| **Chinese** | Target the most common variant: **Mandarin in Simplified script** (`zh-CN` / Putonghua). Traditional Chinese (Taiwan / Hong Kong) and other Sinitic varieties are out of scope for the first pack unless later prioritized separately. Confirm OSRM/Valhalla template coverage before assuming vetted phrases; otherwise MT-fallback + native review ([§7](#7-phrase-translation-source)). |
| **Japanese** | Confirm OSRM/Valhalla template coverage before assuming vetted phrases; if absent from both, treat as **MT-sourced** and flag for native review ([§7](#7-phrase-translation-source)). |
| **Ukrainian** | Prefer offline-generated or human recordings for the clip pack. **Not** equal-effort to the rest of Tier 1 until a vetted phrase source and voice inventory are confirmed. |
| **Hindi** | Confirm OSRM/Valhalla template coverage **before** assuming vetted phrases. If absent from both, treat as **MT-sourced** and flag for native review ([§7](#7-phrase-translation-source)). |

### Tier 2 (smaller — ranked by confirmed translation-template quality, best first)

Norwegian, Swedish, Danish, Finnish, Polish, Dutch, Turkish.

For **each** language in this tier: confirm actual OSRM and/or Valhalla template
presence before build. Some entries are plausible but **unconfirmed** and must
be checked, not assumed.

### Tier 3 (smaller — unconfirmed resources; research required before ranking or building)

Icelandic, Greek, Swahili, Malay, Czech, Hungarian, Romanian, Vietnamese,
Latvian, Serbian, Georgian, Kazakh, Nepali.

**Icelandic:** no confirmed vetted phrase-template source — treat as **lowest
priority** pending dedicated research.

---

## 9. Integration points (documented, not built)

| Concern | Plan |
|---|---|
| **Trigger source** | Ferrostar’s navigation state machine already tracks distance-to-next-maneuver and maneuver type — voice prompts should fire from that state (or an equivalent Navi nav-state layer if Ferrostar is not wired yet). |
| **Audio vs background compute** | Existing design: background routing/compute must not stutter concurrent music. Spoken guidance is a **legitimate foreground interruption** (like any nav app’s directions) and does **not** need to defer to background music the way silent T3/T4 work does. Still avoid starving UI; duck or pause media per platform norms if desired later. |
| **User settings** | Mute / volume for guidance; persisted **language**, **voice-actor gender**, and (if shipped) **persona voice pack** ([§10a](#10a-optional-persona-voice-packs-whole-voice-alternative)); optional **persona suffix** ([§10](#10-persona-sentence-ending-suffixes-optional)). No listener/addressee-gender setting yet ([§6](#6-localization--open-design-question)). Store with other Drive settings ([README settings](../README.md#settings) / SQLite `app_config`). |
| **Offline** | Recorded packs must work fully offline. There is no TTS model to download or cache at runtime. |

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
| Concatenation | The same per-language caveat as [§6](#6-localization--open-design-question) applies: naive append of a trailing suffix to a concatenated phrase may sound unnatural in some languages. Review per language; prefer whole-phrase packs where needed. |

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
| Per-language concat vs whole phrases | **Open** |
| Estimated clip-pack storage | Planning figures in [§4](#4-estimated-storage-recorded-clip-packs); re-measure after first pack |
| Phrase template source verified (OSRM/Valhalla vs MT-fallback) — per language | **Open** |
| Voice-actor vs addressee gender (default masculine/neutral; listener setting) | Specified default; listener-gender setting **open** if added later |
| Language priority tiers | Specified here ([§8](#8-language-priority)); per-language template presence still to confirm |
| Persona sentence-ending suffixes | Specified here (optional, off by default); not implemented |
| Persona voice packs (whole-voice) | Specified here ([§10a](#10a-optional-persona-voice-packs-whole-voice-alternative); optional, off by default); not implemented — engine + checkpoint licenses both **open** |
| rodio / cpal on Android (static clip playback) | **Spike required** |
| Implementation / crates in workspace | **Not started** |
