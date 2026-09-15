# Voice / speech guidance (planned plugin)

Turn-by-turn spoken directions for Navi. **Not implemented** — this document is
a design/reference brief only. No `/sounds` tree, crates, or playback code are
required by this doc.

**Plugin candidate:** listed as `voice` / `voice_guidance` in
[`plugins.md`](plugins.md). Host owns audio I/O and clip packs; a WASM guest may
help with pack selection / phrase assembly once HostApi caps exist. Recorded
packs remain the default offline path.

Status: documentation. Implementation will follow separately after audio-backend
and (optionally) Piper Android spikes.

---

## 1. Audio sources

Voice guidance will support **two interchangeable sources**:

| Source | Role |
|---|---|
| **Pre-recorded human voice clips** | Primary / default. Stable offline path; no TTS toolchain. |
| **Piper TTS-generated speech** | Optional alternative, only if Piper’s toolchain can be built and linked for the Android target ABI (see [§6](#6-piper-tts--candidate-crates-and-known-constraints)). |

The UI should treat them as selectable backends for the same phrase keys: if
Piper is unavailable or disabled, fall back to recordings without changing
maneuver trigger logic.

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

---

## 2. Playback approach

Intended Rust playback stack:

| Layer | Crate / role |
|---|---|
| Playback API | [`rodio`](https://crates.io/crates/rodio) |
| Output | [`cpal`](https://crates.io/crates/cpal) (via rodio) |
| Decode | Symphonia (rodio’s usual decoder path for common formats such as MP3/OGG) |

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

Exact container (MP3 vs OGG) can be chosen at implementation time; the **key**
(stem) must stay stable across languages and genders. Path resolution sketch:

```text
sounds/<language>/<gender>/<concept_key>.<ext>
```

---

## 4. Required word / phrase fragment list

Minimum fragment set **per** `language` / `gender` folder. Filenames should map
1:1 to these concepts (examples in parentheses).

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
logic is designed (and after the per-language grammar decision in [§5](#5-localization--open-design-question)).

---

## 5. Localization — open design question

Naive **word-by-word concatenation** (e.g. `in` + `two` + `hundred` + `meters` +
`left`) can sound acceptable in English but is **not guaranteed** to be
grammatically correct or natural in Norwegian, Swedish, or other languages with
different number formation, word order, or grammatical case/gender agreement.

**Do not assume one assembly strategy for all languages.** When the feature is
built, decide **per language**:

1. Have a **native speaker** review concatenated fragments for that language
   (and for each gender pack if prosody differs).
2. If concatenation fails naturalness or grammar, prefer **whole pre-composed
   phrase recordings** for that language (e.g. one clip: “om to hundre meter,
   sving til venstre”) instead of forcing fragment assembly.
3. Hybrid packs are allowed: concatenate where safe; use composed phrases for
   awkward distance+maneuver templates.

This remains **unresolved per language** until implementation + linguistic
review. English may start with fragments; Nordic packs must not be assumed to
follow the same model without confirmation.

---

## 6. Piper TTS — candidate crates and known constraints

Research summary for a future implementation choice — **not** a decision to add
any crate to `Cargo.toml` now.

### Upstream Piper (archived / forked)

The original [rhasspy/piper](https://github.com/rhasspy/piper) repository was
**archived (October 2025)** and development split into maintained forks:

| Fork | License / notes |
|---|---|
| [OHF-Voice/piper1-gpl](https://github.com/OHF-Voice/piper1-gpl) | **GPL-3.0**, actively maintained. This is what `piper1-rs` binds to. |
| [ayutaz/piper-plus](https://github.com/ayutaz/piper-plus) (MIT-compatible fork) | Own G2P, **no espeak-ng** dependency, better latency — but language coverage is only **JA / EN / ZH / KO / ES / FR / PT / SV**. **No Norwegian**, so it is **not viable as the sole TTS backend** for this project despite the friendlier license. |

Navi’s Nordic recorded packs and any on-device TTS that must speak Norwegian
should assume the **piper1-gpl** line (or recordings), not piper-plus alone.

### Leading Android candidate: `piper-kotlin`

| Item | Detail |
|---|---|
| Repo | [IhorShevchuk/piper-kotlin](https://github.com/IhorShevchuk/piper-kotlin) |
| What it is | Kotlin **JNI** wrapper that bundles **piper1-gpl** + **espeak-ng** + the **ONNX Runtime Android AAR** |
| Build status | Already built and linking for `aarch64-linux-android` / `x86_64-linux-android` |
| API | Exposes `synthesize()` streaming **PCM** chunks — suitable for direct `AudioTrack` / ExoPlayer playback |
| Architecture fit | Host-native (Kotlin/JNI), so it matches the existing **host-owns-audio-I/O** design better than Rust ONNX-binding crates |

This is the **Android-native playback path** already anticipated in
[Fallback](#fallback) below — not a hypothetical spike target. Prefer evaluating
`piper-kotlin` for on-device TTS on Android before investing in unproven Rust
cross-compiles.

**Licensing:** still **GPL-3.0** (piper1-gpl). The fold-in with
[`icons.md`](icons.md)’s GPL bundling decision applies **unchanged**.

### Candidate Rust crates (Linux / secondary)

These remain useful for desktop/Linux experimentation. They do **not** displace
`piper-kotlin` as the leading Android path: several are Linux-only or unproven
on Android (`piper1-rs`’s own docs state it only supports Linux).

| Crate | Notes (as researched) |
|---|---|
| `piper1-rs` | Safe bindings to `libpiper` / **piper1-gpl**; **Linux-only** per project docs; needs **ONNX Runtime** installed separately |
| `piper-rs` | Piper-related Rust wrapper (evaluate maturity; not the preferred Android path) |
| `piper-tts-rs` | Needs `libclang-dev`; currently tends to output **raw PCM** needing external conversion for playback |
| `blazen_audio_piper` | Higher-level; part of a larger framework — weigh dependency surface |
| `natural-tts` | Multi-backend abstraction that can include Piper |

### Licensing

**Piper (piper1-gpl / piper-kotlin) is GPL-licensed.** Bundling Piper (or GPL
voice models) must be folded into the **same open licensing decision** already
flagged for the Navit icon set (GPL asset bundling vs the rest of the
repository’s license) — see [`icons.md`](icons.md). Do not treat Piper as a
separate, already-settled licensing question. piper-plus’s friendlier license
does not remove this issue if Norwegian coverage still requires piper1-gpl.

### Fallback

If Piper is disabled or unavailable, ship **recordings only**. Piper is
**additive**; it must not block the recorded-voice path.

On Android, TTS (when enabled) should use the host **`piper-kotlin` → PCM →
`AudioTrack` / ExoPlayer** path above rather than assuming a Rust ONNX crate
will cross-compile. If that host path is not adopted (e.g. GPL fold-in deferred),
keep recordings-only until licensing and packaging are settled.

---

## 7. Integration points (documented, not built)

| Concern | Plan |
|---|---|
| **Trigger source** | Ferrostar’s navigation state machine already tracks distance-to-next-maneuver and maneuver type — voice prompts should fire from that state (or an equivalent Navi nav-state layer if Ferrostar is not wired yet). |
| **Audio vs background compute** | Existing design: background routing/compute must not stutter concurrent music. Spoken guidance is a **legitimate foreground interruption** (like any nav app’s directions) and does **not** need to defer to background music the way silent T3/T4 work does. Still avoid starving UI; duck or pause media per platform norms if desired later. |
| **User settings** | Mute / volume for guidance; persisted **language** and **voice gender** (and later: recorded vs Piper); optional **persona suffix** ([§8](#8-persona-sentence-ending-suffixes-optional)). Store with other Drive settings ([README settings](../README.md#settings) / SQLite `app_config`). |
| **Offline** | Recorded packs must work fully offline. Piper models, if used, should be on-device and opt-in by size. |

---

## 8. Persona sentence-ending suffixes (optional)

Optional **presentation-only** personalization for spoken guidance: a configurable
word or short phrase appended to the end of assembled spoken instructions — for
example `"master,"` or `"nya,"` — for users who want a more playful or
anime-inspired voice character (catgirl/catboy-style speech patterns being the
concrete example driving this request).

### Rules

| Concern | Spec |
|---|---|
| Scope | Pure presentation layer on top of the existing fragment / phrase system. Does **not** change navigation logic, maneuver data, distance triggers, or timing. |
| Default | **Off** — empty suffix. Default voice packs and phrasing are unchanged unless the user deliberately enables a suffix. |
| Config | User-chosen string (or empty). Stored with other voice settings alongside language / voice gender ([§7](#7-integration-points-documented-not-built)). |
| Concatenation | The same per-language caveat as [§5](#5-localization--open-design-question) applies: naive append of a trailing suffix to a concatenated phrase may sound unnatural in some languages. Review per language; prefer whole-phrase packs where needed. |

### Playback backends

When Piper TTS is eventually wired ([§6](#6-piper-tts--candidate-crates-and-known-constraints)):

| Path | How the suffix is applied |
|---|---|
| **TTS (Piper)** | Include the suffix in the text string passed to synthesis (live). |
| **Pre-recorded clips** | Either append a separate short recorded clip per persona-suffix option after the assembled instruction, or record whole phrases that already include the suffix. |

Both are valid; choose at implementation time. Keep persona clips / strings
clearly separate from the default voice pack so the default experience stays
unaffected when the setting is empty/off.

---

## Related docs

| Doc | Relevance |
|---|---|
| [`architecture.md`](architecture.md) | Thread tiers (T2 UI/audio); keep guidance off the routing pool |
| [`android-build.md`](android-build.md) | ABI / NDK constraints for any native audio or ONNX spike |
| [`icons.md`](icons.md) | GPL bundling decision shared with Piper |
| [`plugins.md`](plugins.md) | Optional future: voice as `voice_guidance` plugin ([§6](plugins.md#6-voice-guidance-voice--voice_guidance)); voice command is [§16](plugins.md#16-voice-command-voice_command--voice_cmd) |
| [`plugins/voice-command.md`](plugins/voice-command.md) | Conversational navigate/save/POI — separate sandbox TTS; not maneuver clips |
| [`plugins/adaptive-speed-warning-spec.md`](plugins/adaptive-speed-warning-spec.md) | Spoken overspeed tiers reuse `voice_speak` / the same playback stack |
| [`plugins/custom-alert-sounds-spec.md`](plugins/custom-alert-sounds-spec.md) | Road-sign / camera / children-zone **tones** — not turn-by-turn phrases |
| [`plugins/instrument-cluster-agl-spec.md`](plugins/instrument-cluster-agl-spec.md) | Exports the same maneuver + warning state to clusters (no audio) |

## Status checklist

| Item | Status |
|---|---|
| Recorded-voice folder contract | Specified here; tree not created |
| Fragment key list | Minimum set documented; connectors TBD with assembly |
| Per-language concat vs whole phrases | **Open** |
| Persona sentence-ending suffixes | Specified here (optional, off by default); not implemented |
| rodio / cpal on Android | **Spike required** |
| Piper / ONNX on Android | **De-risked** via [piper-kotlin](https://github.com/IhorShevchuk/piper-kotlin) (pre-built JNI wrapper); confirm GPL licensing fold-in before adopting |
| Implementation / crates in workspace | **Not started** |
