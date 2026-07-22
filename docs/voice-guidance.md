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

### Candidate crates

| Crate | Notes (as researched) |
|---|---|
| `piper1-rs` | Safe bindings to `libpiper`; Linux-focused; needs **ONNX Runtime** installed separately |
| `piper-rs` | Piper-related Rust wrapper (evaluate maturity / Android support at spike time) |
| `piper-tts-rs` | Needs `libclang-dev`; currently tends to output **raw PCM** needing external conversion for playback |
| `blazen_audio_piper` | Higher-level; part of a larger framework — weigh dependency surface |
| `natural-tts` | Multi-backend abstraction that can include Piper |

### Gating risk (Android)

The real gate for **all** Piper options is whether **ONNX Runtime**
cross-compiles and links for the Android target ABI — not which Rust wrapper is
chosen. Confirm ONNX Runtime + Piper native libs for `aarch64-linux-android`
(and emulator `x86_64-linux-android` if needed) in an implementation spike
before depending on TTS in CI or releases.

### Licensing

**Piper is GPL-licensed.** Bundling Piper (or GPL voice models) must be folded
into the **same open licensing decision** already flagged for the Navit icon set
(GPL asset bundling vs the rest of the repository’s license) — see
[`icons.md`](icons.md). Do not treat Piper as a separate, already-settled
licensing question.

### Fallback

If no Piper/ONNX path builds reliably for Android when this is implemented, ship
**recordings only**. Piper is **additive**; it must not block the recorded-voice
path.

---

## 7. Integration points (documented, not built)

| Concern | Plan |
|---|---|
| **Trigger source** | Ferrostar’s navigation state machine already tracks distance-to-next-maneuver and maneuver type — voice prompts should fire from that state (or an equivalent Navi nav-state layer if Ferrostar is not wired yet). |
| **Audio vs background compute** | Existing design: background routing/compute must not stutter concurrent music. Spoken guidance is a **legitimate foreground interruption** (like any nav app’s directions) and does **not** need to defer to background music the way silent T3/T4 work does. Still avoid starving UI; duck or pause media per platform norms if desired later. |
| **User settings** | Mute / volume for guidance; persisted **language** and **voice gender** (and later: recorded vs Piper). Store with other Drive settings ([README settings](../README.md#settings) / SQLite `app_config`). |
| **Offline** | Recorded packs must work fully offline. Piper models, if used, should be on-device and opt-in by size. |

---

## Related docs

| Doc | Relevance |
|---|---|
| [`architecture.md`](../architecture.md) | Thread tiers (T2 UI/audio); keep guidance off the routing pool |
| [`android-build.md`](android-build.md) | ABI / NDK constraints for any native audio or ONNX spike |
| [`icons.md`](icons.md) | GPL bundling decision shared with Piper |
| [`plugins.md`](plugins.md) | Optional future: voice as `voice_guidance` plugin ([§6](plugins.md#6-voice-guidance-voice--voice_guidance)) |

## Status checklist

| Item | Status |
|---|---|
| Recorded-voice folder contract | Specified here; tree not created |
| Fragment key list | Minimum set documented; connectors TBD with assembly |
| Per-language concat vs whole phrases | **Open** |
| rodio / cpal on Android | **Spike required** |
| Piper / ONNX on Android | **Spike required**; optional |
| Implementation / crates in workspace | **Not started** |
