# Voice command plugin (specification)

**Status:** specification only — not implemented.
**Path:** `docs/plugins/voice-command.md`
**Architecture:** planned WASM guest via `plugin-host` / `plugin-sdk` and a
**dedicated, capability-starved HostApi instance** ([`plugins.md`](../plugins.md)).
Until the
[wasmtime upgrade gate](../plugins.md#gate-upgrade-wasmtime-before-shipping-any-product-plugin)
lands, this plugin must not be linked into a shipped binary.
**System requirements** (all plugins): user **enable/disable** toggle. This
plugin uses the microphone and speaker; it does **not** use USB or Bluetooth
accessories
([`plugins.md` — enable/disable](../plugins.md#enable--disable-required)).
Disabled = capture stopped, wake-word unregistered, no HostApi calls.

Working title / id suggestion: `voice_command` / `voice_cmd`.

This plugin is a **complete spoken alternative** to the on-screen UI for a
small, auditable set of navigation intents. It is aimed first at people who
have trouble reading and writing. Voice is not a shortcut on top of a
text-first flow: every supported intent must be completable without looking at
or touching the screen (aside from the one-time enable and language/gender
setup, which should itself be speakable once the plugin is on).

It is **not** turn-by-turn maneuver speech
([`voice-guidance.md`](../voice-guidance.md)), **not** road-sign / camera
alert tones
([`custom-alert-sounds-spec.md`](custom-alert-sounds-spec.md)), **not**
escalating overspeed nags
([`adaptive-speed-warning-spec.md`](adaptive-speed-warning-spec.md)), and
**not** the offline authoring path used to pre-render “Bitchin' Betty”
speed-warning clips (see [§10](#10-explicit-non-goals--boundaries-with-other-navi-features)).

---

## Goals

1. Let a user navigate, save a labelled place, and ask for the nearest
   category POI **entirely by voice**, including spoken yes/no follow-ups.
2. Keep **all** automatic speech recognition (ASR), text-to-speech (TTS), and
   intent parsing **on-device**, with **no network path** in the voice
   pipeline at the capability level.
3. Enforce that guarantee with a **wasmtime sandbox whose host-import table
   cannot grow into network, filesystem, or other plugins' capabilities** —
   even after map-tile downloads and a future `navi.app` routing-pack service
   exist in the same app process.
4. Support multiple languages and, where models allow, regional
   dialects/accents; TTS language follows the OS locale; male and female
   voices are user-selectable per language.
5. Never treat the app's own TTS playback as a user command (v1: hard
   microphone gating).
6. Treat saved voice labels as **sensitive personal data** (they can reveal
   relationships and home addresses): local storage only, never synced.

## Non-goals

- Implementing the plugin in this documentation pass.
- Linking `plugin-host` into `navi-ffi` / the APK / `navi-desktop` before the
  wasmtime gate.
- Always-on ambient listening that uploads, streams, or retains audio beyond
  the bounded buffer needed for wake-word / command detection.
- An on-device LLM (or any cloud LLM) for intent classification.
- NPU-accelerating A* / graph search (not an ML workload; stays on CPU).
- Barge-in (talking over TTS) in v1 — that waits on Acoustic Echo
  Cancellation (AEC); see [§6](#6-self-voice-insulation-design).
- Replacing recorded turn-by-turn packs, alert tones, or the Chatterbox
  authoring tool.
- OS/platform TTS as the primary synthesizer (no offline-model guarantee).

---

## 1. Overview / purpose

Navi is usable today if you can read the map, type a destination, and tap
**Plan route**. That is a barrier for people who struggle with reading and
writing, and for drivers who cannot safely look at a screen.

This plugin exists so those users can operate the supported navigation
actions by speaking, and hear the result spoken back. Design choices are
judged against that use case first:

| Decision class | Accessibility test |
|---|---|
| Activation | Can the user start a command without finding a small on-screen button? |
| Confirmation | Is every disambiguation and yes/no step spoken, with a bounded reply window? |
| Failure | Does the system say what went wrong (no match, timeout, unsupported locale) instead of failing silently on screen? |
| Privacy | Does the user have to trust a cloud ASR vendor in order to use the only UI they can use? |

Privacy is not a secondary preference. Target users may be dictating home
addresses and relationship labels (“mom's”). Those utterances, transcripts,
and saved names must never leave the device.

---

## 2. Architecture

Three layers, with a hard sandbox around inference. Other Navi plugins (weather
HTTPS, future `navi.app` pack mirrors, USB/Bluetooth accessories) **must not
share this WASM instance or its import table**.

### 2.1 Layers

| Layer | Trust | Runs | Owns |
|---|---|---|---|
| **Native orchestration** | Trusted host (`navi-ffi` / Android / desktop) | App process | Mic capture, speaker playback, hard mic-gate, wake-word spotter, enable/disable, model-file load from app storage, NameIndex / POI / `saved_places` / A* execution, audio-focus |
| **Voice WASM instance** | Untrusted guest | Dedicated wasmtime store | ASR inference, intent pattern/slot matching, TTS inference |
| **Navi core** | Trusted | Same process, not in the guest | `NameIndex` FTS5, `PoiIndex`, `saved_places` SQLite, `RouteGraph` A* |

The guest **never** opens the microphone, the speaker, SQLite, the `.osm.pbf`,
or a socket. The host copies a **bounded PCM buffer** in and copies
**text / PCM** out.

### 2.2 Sandbox boundary and host imports

The voice instance is loaded with a **private host policy set**. Capabilities
that exist for other plugins (`poi_query`, `weather_read`, `accessory_*`,
future HTTPS helpers, `vehicle_signal_publish`, …) are **not in this
instance's linker**. Absence from the import table is the control: a guest
cannot call an import the embedder did not register, regardless of what the
manifest asks for.

**Allowed imports (v1 maximum):**

| Import (proposed) | Direction | Purpose |
|---|---|---|
| `navi.voice_audio_in(ptr, len) -> i32` | Host → guest | Copy one PCM frame/utterance (linear PCM, documented sample rate/channel count) into guest memory. Host enforces max length. |
| `navi.voice_text_out(ptr, len)` | Guest → host | UTF-8 transcript and/or structured intent JSON (schema below). Host enforces max length. |
| `navi.voice_pcm_out(ptr, len)` | Guest → host | Linear PCM for TTS playback. Host enforces max length and plays it. |
| `navi.npu_infer` (optional) | Guest → host | Opaque tensor in/out via a native NNAPI/QNN (or CPU ONNX) bridge. **Not** a general syscall. See [§8](#8-npu-usage-and-fallback-path). |
| `log` | Guest → host | Diagnostics. Host **must redact** raw PCM and must not persist full transcripts into USB-visible logs by default (see debug rules below). |

**Forbidden on this instance (structural, not a setting):**

- WASI filesystem and sockets (already not linked for any Navi guest —
  [`plugins.md`](../plugins.md)).
- Any `network_*` / HTTP import, including ones added later for tile or
  `navi.app` pack downloads.
- `poi_query` / `poi_write` / `position_read` / `route_read` / `plugin_kv` /
  `accessory_*` / `vehicle_signal_publish`. Place search and routing stay in
  the host so the inference guest cannot observe GPS or the POI database.
- mmap of arbitrary files. If model weights are too large for guest linear
  memory, the host may expose a **read-only, pre-selected weight blob**
  through a dedicated `navi.model_weight_read(offset, ptr, len)` import that
  can only address the currently loaded ASR/TTS files — not a general
  filesystem. That import is still not a path-based `open()`.

Manifest sketch (names not in ABI yet):

```json
{
  "name": "voice_command",
  "version": "0.1.0",
  "entry": "plugin_main",
  "capabilities": ["voice_audio", "voice_text", "voice_pcm", "log"],
  "fuel_limit": null,
  "timeout_ms": null,
  "wasm": "plugin.wasm"
}
```

`fuel_limit` / `timeout_ms` for this guest **cannot** use the current
defaults (`DEFAULT_FUEL` = 5_000_000, `DEFAULT_TIMEOUT_MS` = 250). Whisper-
class ASR is seconds of work. Isolation policy for this instance is an
[open question](#11-open-questions-requiring-a-decision-before-implementation-begins)
(raised fuel + wall-clock vs. host-driven chunked `npu_infer` steps). Other
plugins keep the tight defaults.

### 2.3 Wake-word placement

**Recommendation: the wake-word spotter runs as trusted native code, outside
the voice WASM instance.** ASR, TTS, and intent parsing stay inside the
sandbox.

Justification:

1. Hands-free activation needs **continuous low-power listening**. The current
   wasmtime isolation model is **per-call fuel + epoch timeout**, which is a
   poor fit for an always-scheduled audio callback.
2. The spotter's job is a boolean (“wake phrase heard”) plus a **short
   trailing capture window**, not transcription. It never produces an address
   or a label.
3. Keeping the spotter native lets the host **hard-gate** the mic path
   ([§6](#6-self-voice-insulation-design)) before any buffer is copied into
   WASM.
4. A compromised ASR guest still cannot reach the network; a native spotter
   is a small, auditable component with **no send path**.

The spotter must still obey the privacy rules: no cloud wake-word service, no
audio uploaded for “improve recognition,” ring buffer sized only for keyword
spotting plus the command window, discarded when the plugin is disabled or
the window closes.

Treating the spotter as another WASM guest is a valid alternative if a
future host grows a long-lived, low-fuel streaming instance. That is **not**
the v1 recommendation: it would keep PCM in guest memory continuously and
force isolation-limit exceptions for a component that does not need
multilingual ASR.

### 2.4 Data flow — one command-to-response cycle

Example: “Hey Navi, navigate to the cabin.”

```text
  microphone
       |
       v
  [native capture] ---- hard gate OPEN (not in TTS playback)
       |
       +---> [native wake-word]  --no match--> drop frame (do not copy into WASM)
       |            |
       |          match
       |            v
       |     open bounded command window (stop after silence / max ms)
       |            |
       |            v
       |     copy PCM into voice WASM via voice_audio_in
       |            |
       |            v
       |     [guest ASR] --> transcript
       |            |
       |            v
       |     [guest pattern/slot matcher] --> VoiceIntent JSON via voice_text_out
       |            |
       |            v
       +----- [native orchestrator]
                    |
                    |  NavigateTo { query: "the cabin" }
                    v
           host NameIndex / saved_places / POI match (offline)
                    |
                    v
           host builds reply text ("Cabin Ridge, 12 kilometres. Start route?")
                    |
                    v
           copy text into guest --> [guest TTS] --> voice_pcm_out
                    |
                    v
           [native playback]  +  hard gate CLOSED for playback + decay tail
                    |
                    v
           if confirmation required: reopen mic for yes/no window only
                    |
                    v
           on yes: host runs existing plan-route path (CPU A*)
           on no / timeout / unclear: speak cancel; close mic
```

Host-side side effects (route plan, `saved_places` insert, POI read) happen
**after** the guest returns a structured intent, using the same UniFFI / SQLite
paths the on-screen UI uses ([`map-marking-saved-places.md`](../map-marking-saved-places.md),
[`poi.md`](../poi.md), corridor pipeline in [`architecture.md`](../architecture.md)).

### 2.5 Debug files

USB/MTP layout per [`plugins.md`](../plugins.md#debug-files-usbmtp):

```text
Documents/debug/voice-command/
```

Default contents: enable/disable events, locale/voice selected, intent enum
(not the raw place string unless a user-facing “include command text”
toggle is on), match counts, timeouts, gate open/close. Do **not** dump PCM
or full transcripts into this tree by default.

---

## 3. Privacy and security model

WASM by itself is not the privacy guarantee. A guest with a `fetch` import
or a shared linker that also serves the weather plugin could still exfiltrate
audio. The guarantee is **capability-based isolation**:

1. **Separate wasmtime store / linker** for voice. Other plugins' HostApi
   functions are not registered. Linking a network import into the weather
   guest later cannot make that import appear on the voice guest.
2. **Minimal import table** ([§2.2](#22-sandbox-boundary-and-host-imports)):
   audio in, text/PCM out, optional opaque NPU infer, redacted log. No
   sockets, no path-based filesystem, no GPS, no POI store.
3. **Host-owned I/O.** Mic and speaker stay in native code. The guest cannot
   keep the capture device open across disable.
4. **No voice-pipeline network path**, including when the device is online
   and when the host is downloading map tiles or talking to a `navi.app`-class
   pack mirror ([`precomputed-index-and-route-cache.md`](../precomputed-index-and-route-cache.md)).
   Model **install** downloads (if used) are a **user-visible host Tools
   action**, the same class as “Download region,” not a call the guest can
   make.
5. **Disable releases capture.** Same rule as USB/Bluetooth plugins closing
   sessions ([`plugins.md`](../plugins.md#enable--disable-required)).
6. **Saved labels** stay in local `saved_places` (`navi.db`). They are out of
   scope for any future cloud sync of routes, settings, or telemetry, even if
   that sync is built for other data.

Always-on listening, if wake-word is enabled, still **must not** transmit or
retain audio beyond the spotter ring buffer and the command window. There is
no “send clips to improve the model” path.

---

## 4. Supported intents and slot / entity requirements

Intent classification is a **lightweight local pattern- and slot-matching
step**, not an LLM. Each supported language ships a **phrase-pattern pack**
that maps utterances onto one shared internal enum. Determinism, auditability,
and per-language porting all beat open-ended parsing for v1.

### 4.1 Shared intent enum (v1)

| Intent | Slots | Spoken examples (English) | Host action |
|---|---|---|---|
| `NavigateTo` | `place_query: string` | “navigate to X”, “take me to X” | Fuzzy-match `place_query` (see [§4.4](#44-fuzzy-place-name-matching)); speak candidate; on confirm, set To and plan route |
| `SaveCurrentLocation` | `label: string` | “save current location as mom's” | Insert/update `saved_places` with GPS fix + label; speak confirmation |
| `NearestPoi` | `category: PoiSlot`, `live_enrichment: bool` (default **false**) | “nearest gas station” | Query **offline** regional POI index; speak name + distance; yes/no for directions |
| `ConfirmYes` | none | “yes”, “yeah”, “ok”, language equivalents | Valid **only** inside an open confirmation window |
| `ConfirmNo` | none | “no”, “cancel”, language equivalents | Close window; do not plan |
| `Cancel` | none | “stop”, “never mind” | Abort in-flight confirmation or command window |
| `Unknown` | `raw: string` (optional, not logged by default) | — | Speak a short “I didn't understand. You can say navigate to, save this place as, or nearest …” |

`ConfirmYes` / `ConfirmNo` / `Cancel` outside a confirmation window map to
`Unknown` (or a short spoken hint), so stray “yes” after the window closes
does not start a route.

### 4.2 `SaveCurrentLocation` — sensitive data

Labels like “mom's”, “work”, or a street nickname **reveal relationships and
addresses**. Rules:

- Persist only in the existing on-device `saved_places` table
  ([`map-marking-saved-places.md`](../map-marking-saved-places.md)).
- Same enable/disable and USB-debug redaction as other voice data.
- **Never** include these rows in any future cloud/backend sync, account
  backup, or `navi.app` upload — even if saved **routes**, settings, or
  telemetry later sync. Treat as a permanent local-only class, not a
  default that a generic “sync all SQLite” pass could pick up.
- Voice must be able to **navigate to a saved label** later via `NavigateTo`
  matching `saved_places.name` first (user's own labels beat OSM homonyms).

### 4.3 `NearestPoi` — offline default, optional live enrichment

**Default:** query only the **offline-downloaded regional POI database**
(`PoiIndex` / OSM tags already on device; [`poi.md`](../poi.md)). No live
pricing, hours, or occupancy.

**Live / online-only facts** (hours, fuel price, “open now”): **out of band
for the voice sandbox**. If offered at all:

- Host-mediated, user-visible network (same bar as the weather plugin).
- **Off** unless the user **explicitly** asks in the utterance (“with live
  hours”) **or** has enabled a clearly named setting such as “Allow online
  POI details for voice.”
- The guest only sets a boolean slot; it never receives a URL or HTTP
  client.
- If live fetch fails, speak the offline result and say that live details
  were not available — do not block navigation on the network.

Spoken category (`PoiSlot`) maps to host categories. **Gap:** core
`PoiCategory` today has Water, Cabin, General, NetworkHut, Restroom,
OvernightFacility, CraftBrewery, TentSite, Fishing, RestArea, Lodging — **not**
`amenity=fuel`. v1 “gas station” needs a host-side Fuel (or equivalent)
category or a dedicated query. That mapping table is an
[open question](#11-open-questions-requiring-a-decision-before-implementation-begins).

### 4.4 Fuzzy place-name matching

Target users may not know official spelling and may use phonetic,
informal, or local names. Matching is a **host** job (needs the place index
and `saved_places`); the guest only supplies `place_query` text.

**Recommended strategy:**

1. **Saved places** exact/fuzzy match on `name` (highest priority).
2. **NameIndex FTS5** (`searchPlaces` / `name_entries`) — already used by the
   To/Via UI. Today's index stores a single `name` column; **OSM alternate
   names are not indexed yet.**
3. **Extend place-index ingest** (host/core, not the voice guest) to also
   index, per region extract: `alt_name`, `loc_name`, `old_name`,
   `short_name`, `official_name`, `int_name`, and `name:xx` (language-tagged
   names, including the OS locale and English). Multiple FTS rows or a
   concatenated search document per `osm_id` are both acceptable; keep
   `osm_id` as the join key.
4. **Phonetic / fuzzy rerank** after FTS: language-appropriate phonetic key
   (e.g. Double Metaphone for English, a Nordic-aware fold on top of the
   existing FTS5 unicode61 behaviour in `NameIndex` tests) plus edit
   distance. Bias remaining ties by distance from the current fix.
5. **Ambiguity:** if two or more candidates score closely, **speak** the top
   two or three (“Did you mean X, about 4 kilometres, or Y, about 12?”) and
   use the yes/no / ordinal follow-up window. Do not require a typed pick
   list. If none are plausible, say so and invite a rephrase.

Do not require the user to spell.

### 4.5 Spoken confirmation follow-ups

After a query that needs a decision (nearest POI, ambiguous `NavigateTo`,
confirm save), the host:

1. Speaks the prompt (TTS).
2. Keeps the mic **hard-gated closed** during that playback ([§6](#6-self-voice-insulation-design)).
3. Opens a **bounded listening window** whose ASR/intent pack is restricted
   to `ConfirmYes` / `ConfirmNo` / `Cancel` (and, when listing candidates,
   short ordinals: “the first one”). Full `NavigateTo` parsing is **off** in
   this window so a place name cannot be misheard as a new command.
4. Closes the window on a clear yes/no/cancel, on silence timeout, or on
   unclassifiable audio after a single retry.

**Timeout / unclear (v1 recommended default):**

| Event | Behaviour |
|---|---|
| No speech before timeout (recommend **6 s** after TTS ends; see open questions) | Speak “I didn't catch that.” **Do not** start a route. Mic stays closed. |
| Unclear audio once | One repeat of the yes/no question, then the same timeout. |
| Still unclear / second timeout | Speak “Cancelled.” Return to idle (wake-word or PTT only). |
| User says cancel / no | Speak a short acknowledgement; idle. |

No unbounded retry loop (that traps users who cannot speak clearly).

### 4.6 Phrase packs

One **shared** `VoiceIntent` enum. Each language (and optional dialect pack)
ships a list of patterns / slot regexes / keyword lists. Translators port
patterns; they do not fork engine logic. Unknown slots fail closed to
`Unknown`.

---

## 5. Activation mode: options, trade-offs, recommendation

This is an **open product decision**. Both modes should be implementable; v1
should not ship without an explicit choice (and likely both, with one
default).

| | **Push-to-talk (PTT)** | **Wake-word (“Hey Navi”)** |
|---|---|---|
| How | User holds or taps a large, reachable control (steering-wheel key, on-screen hold target, headset button) while speaking | Continuous on-device spotter; command window opens after the phrase |
| Accessibility | Requires locating and actuating a control. Harder for users who cannot reliably tap or who cannot look down | **Better fit** for the primary use case: no need to find a button |
| Privacy posture | Mic active only while the control is down — easiest story to audit | Mic is open for spotting; still no transmit path, but continuous capture is easier to misunderstand |
| Engineering | Simpler: no always-on DSP, no per-language wake model, no cabin false-accept tuning | Continuous low-power listening; wake model per language/locale (or a shared “Navi” spotter plus locale ASR); false accepts from radio/passengers; battery/thermal on-device |
| False positives | Low (user-gated) | Cabin noise, commercial radio, other passengers saying “Navi” |
| False negatives | Missed if the user cannot hit the control | Missed if the spotter is too strict or the wake phrase is hard in that language |
| Self-voice | Gate during TTS still required (media or radio could contain the wake phrase too) | Same gate; also ignore the wake phrase in TTS if the prompt ever says “Navi” |

**Recommendation:** ship **wake-word as the default** because the plugin's
reason to exist is hands-free, eyes-free use. **Always offer PTT** as a
first-class alternative (settings + a large on-screen hold target) for
privacy-conscious users, noisy cabins, and false-accept environments.

Do not require wake-word to be available in every language on day one: if a
locale has ASR/TTS but no wake model, **PTT remains usable** and the system
must say that hands-free is unavailable rather than failing silently.

Wake-phrase localization (English “Hey Navi” vs. translated phrases) is
unresolved; see [§11](#11-open-questions-requiring-a-decision-before-implementation-begins).

---

## 6. Self-voice insulation design

The system must not treat its own TTS as a user command (wake-word or ASR).

### 6.1 v1 primary mechanism: hard microphone gating

During TTS playback, the **microphone input path is disconnected** from the
wake-word and ASR pipeline — not merely output-muted or capture-volume zeroed.

Concretely:

1. Orchestrator sets `gate = Closed` **before** the first TTS PCM is written
   to the speaker.
2. While `Closed`, native capture may still run for AEC experiments later,
   but **no frames are delivered** to the spotter or to `voice_audio_in`.
3. After playback ends, keep `Closed` for a short **acoustic decay tail**
   (recommend 200–400 ms; cabin-dependent, tunable) so reverberation of the
   prompt is not transcribed.
4. Only then open the yes/no window or return to idle spotting / PTT.

“Mute” that still feeds ASR is **not** sufficient.

### 6.2 Future: Acoustic Echo Cancellation (barge-in)

AEC would let the user **interrupt** a prompt (“nearest gas station is…” —
“yes”) without waiting for playback to finish. It is **not** v1.

Notes for a later revision:

- AEC needs a **reference signal** (the outgoing TTS mix) aligned with the
  mic. That signal is native audio-path data. If AEC stays in the host HAL
  (recommended), the voice WASM import table does not need to grow.
- If a guest ever ran AEC, the host would have to **import the TTS reference
  PCM into the sandbox**, widening the audio surface. Avoid that unless
  there is a strong reason.
- Car-cabin acoustics (loudspeaker placement, road noise, open windows,
  multiple occupants) make AEC **non-trivial**; laptop-grade AEC will not
  transfer unchanged. Budget a dedicated in-car spike, not a library drop-in.

### 6.3 Considered and rejected: speaker-embedding discrimination

Reject for v1 and as the primary insulation method:

- Cabin noise and a far-field mic make speaker embeddings **fragile**.
- The plugin **requires multiple TTS voices** (male/female per language);
  embeddings would need to track every installed voice and still fail for
  unseen voices.
- A second passenger with a similar pitch could be suppressed or, worse,
  the owner's command could be dropped.

Embeddings may be researched later as an extra signal; they must not replace
hard gating.

---

## 7. Multilingual and dialect strategy

### 7.1 Locale and voice selection

| Setting | Rule |
|---|---|
| **ASR language** | Follows OS locale (BCP 47), with an in-plugin override for users whose spoken language differs from the UI/OS locale ([`i18n-translation-spec.md`](i18n-translation-spec.md) still does not infer language from GPS/SIM — same rule here). |
| **TTS language** | **Follows the device OS locale** for the spoken reply language, unless the user has set the voice-command override above. |
| **TTS gender** | **Male and female** options per supported language; user-selectable; persist with other Drive / plugin settings. |
| **Dialects** | Prefer models that cover regional accents inside one language (Whisper-class) and/or **explicit dialect packs** (e.g. Indian English in Vosk) when those packs exist and the locale matches. Do not claim dialect support the model does not have. |

UI for first-time gender/language pick must be reachable without reading if
the plugin is already enabled (spoken setup wizard is in scope for
accessibility; exact script is not frozen here).

### 7.2 Model bundling (open choice — recommended hybrid)

| Strategy | Pros | Cons |
|---|---|---|
| **Bundle all supported languages at install** | Works fully offline from first run; no extra download UX | APK/payload size explodes (ASR+TTS×gender×language); bad for F-Droid/Play and devices with small storage |
| **Download-on-demand only** | Small base APK | First-run voice needs a **host** download (user-visible, not guest network). Users with no connectivity and an unmatched locale are stuck. Conflicts with “complete alternative” if the user cannot read the download screen |
| **Hybrid (recommended default)** | Bundle **OS-locale pair** (ASR + TTS male+female) when that locale is in the supported set, plus a **documented fallback locale** (recommend **English**) so a device with an unsupported locale can still speak *something*. Additional languages via Tools → download, same consent pattern as region PBF / PMTiles | Two voices × two languages still cost tens to hundreds of MB depending on the TTS/ASR choice; locale change after install may need another download |

Downloads, if any, are **host Tools actions** with explicit user start. The
voice WASM instance still has no network import. Failed download: keep the
fallback pack; speak that fallback is in use.

Re-verify size numbers against the chosen models at implementation time.

### 7.3 Fallback when no matching voice / ASR exists

1. If OS locale has a bundled or already-downloaded pack → use it.
2. Else if a **same-language, different-region** pack exists (`nb` vs `nn`,
   `en-GB` vs `en-IN`) → use it and speak (in that pack's language) that a
   regional pack was substituted.
3. Else → **fallback locale TTS+ASR** (English unless a later decision
   picks another). Speak a one-time notice in the fallback language that
   the device language is not installed, and that a pack can be downloaded
   from Tools when the user has someone who can help tap — imperfect, but
   better than a silent on-screen error.
4. If even the fallback pack is missing (broken install) → **PTT/wake
   disabled**; plugin shows the existing enable-toggle error path; do not
   pretend to listen.

Never call a cloud TTS/ASR API as a fallback.

---

## 8. NPU usage and fallback path

| Workload | Accelerator | Notes |
|---|---|---|
| **A* / corridor / graph search** | **CPU only** | Not an ML graph. Out of scope for NPU. Stays in `driver-break-core`. |
| **ASR inference** | NPU when the **host** exposes it; else CPU | Guest calls optional `npu_infer` (or a CPU `infer`) import; it does not bind to a vendor SDK itself. |
| **TTS inference** | Same as ASR | Piper-class ONNX or equivalent, via the same host bridge. |

**Standard WASM has no NPU device.** Do not design the guest to `#ifdef`
Hexagon or NNAPI. Practical options:

| Path | Role |
|---|---|
| **Host-function bridge (recommended for wasmtime embedder)** | Guest submits a model id + tensor; host runs NNAPI / vendor QNN / ONNX Runtime on CPU or NPU and returns the output tensor. The guest never receives a GPU/NPU handle. |
| **CPU-only fallback (required)** | Same import, host executes on CPU. Devices without an exposed NPU — the **common case** from inside WASM — must still run the plugin, slower. |
| **WebNN** | Emerging **browser** API. Relevant only if a future Web embed existed. `navi-desktop` / Android wasmtime is **not** a browser; do not block v1 on WebNN. |

The sandbox **must not hard-depend** on NPU availability. Feature-detect on
the host at load; never fail plugin enable solely because NNAPI/QNN is
missing.

---

## 9. Candidate technology comparison

Library maturity and WASM support **move quickly**. Re-verify against current
crates.io / upstream before implementation. Tables below are a **comparison,
not a binding choice**.

Shared constraints for every row: must be runnable **fully offline**; must be
wrappable behind the voice import table; Android `aarch64` (and desktop
hosts) matter more than a Linux-only demo.

### 9.1 ASR

| Candidate | Coverage | Dialect notes | Size (order of magnitude) | WASM / embed notes | Maturity / risk |
|---|---|---|---|---|---|
| **`whisper-rs`** (whisper.cpp) | Broadest multilingual; one multilingual model can decode many languages | Accents often handled **implicitly** inside one model; not a per-dialect file | Large vs Vosk (hundreds of MB class for bigger models; tiny/base are smaller) | **whisper.wasm** and wasm32 builds exist as precedent | Most proven multilingual path in this list; still confirm Android + wasmtime fuel/chunking. Re-verify. |
| **`vosk-rs`** (Vosk API) | 20+ languages | Several **explicit** dialect models (e.g. Indian English as its own model) | Small (~50 MB/language class) — better for hybrid bundling | Native C library linkage. **WASM compilation is not assumed** — verify before committing | Attractive if dialect packs are a v1 requirement and WASM/static-link on Android checks out. Re-verify. |
| **`parakeet-rs`** (NVIDIA Parakeet) | TDT: ~25 languages with auto-detect; Cohere-backed offline mode: ~14 languages | Streaming-chunk design (fits bounded windows better than full-utterance Whisper) | Check per published checkpoint | Newer Rust wrapping; NPU story may be NVIDIA-centric | **Needs a maturity check** (API stability, Android, license, offline checkpoints). Re-verify. |
| **`voirs-recognizer`** | Wrapper over Whisper / DeepSpeech / Wav2Vec2 | Explicit **custom phoneme sets** — interesting for dialects | Depends on backend | Plugin-style API | **Younger project; needs a maturity check.** Re-verify. |

**Not in table, rejected as primary ASR:** any cloud streaming API; OS
speech recognizer that can route to a vendor cloud.

### 9.2 TTS

“Piper-rs” is **not** one crate. Pin a concrete target in the
implementation spike; until then the comparison is:

| Candidate | Coverage | Size | WASM / embed notes | Maturity / risk |
|---|---|---|---|---|
| **Piper via `piper1-rs`** (bindings to `libpiper` + **ONNX Runtime**) | Per-language, per-voice neural models; male/female where upstream voices exist | Small models; good fit for hybrid bundling | Same **ONNX-on-Android** gate already documented in [`voice-guidance.md`](../voice-guidance.md) | **Primary candidate to spike first**, aligned with voice-guidance research. GPL: same open licensing decision as other GPL assets ([`icons.md`](../icons.md), voice-guidance §6). Re-verify. |
| **Other Piper wrappers** (`piper-rs`, `piper-tts-rs`, `blazen_audio_piper`, `natural-tts`) | Same Piper voices in principle | Same models | Mix of ONNX bindings vs experimental pure-Rust/RTen | **Do not treat as interchangeable.** `piper-tts-rs` has been raw-PCM oriented; `piper-rs` maturity/Android must be checked independently. Re-verify. |
| **`any-tts`** (Candle; Kokoro, Qwen3-TTS, VibeVoice, …) | Strong named multilingual coverage | **Heavier** than Piper | Candle/Rust; WASM and Android NPU story unclear | **Fallback family** only if Piper voice coverage is too thin for a required v1 language. Re-verify. |
| **OS-level `tts` crate** (delegates to platform TTS) | Whatever the device has installed | Zero extra models | Trivial wrap | **Rejected as primary.** Does not guarantee on-device models or “no network path.” Vendor TTS may hit the cloud. Mentioned only as a why-not. |

If Piper/ONNX cannot be linked for Android, this plugin cannot silently fall
back to platform cloud TTS. Options are: CPU ONNX, a different local engine
from the table, or delay the plugin. Recorded clips are **not** a substitute
here (unbounded place names).

Voice-guidance may still use **recorded maneuver packs** as its default;
this plugin's TTS is a **separate pipeline** that may share an ONNX runtime
and even Piper voices if licensing and memory allow, but it must not block
on guidance clip design.

---

## 10. Explicit non-goals / boundaries with other Navi features

| Surface | Relationship |
|---|---|
| **Turn-by-turn voice guidance** ([`voice-guidance.md`](../voice-guidance.md)) | Maneuver prompts (recorded packs, optional Piper). Different phrase keys and triggers. May share the host playback device and audio focus; mix policy is host-owned (safety prompts vs. this plugin's replies). |
| **Custom alert sounds** | Short urgency-phase tones, not commands. |
| **Adaptive speed warning** | Spoken overspeed tiers from HUD limit, not this intent enum. |
| **Chatterbox TTS** | **Offline authoring tool only.** Used to **pre-generate** fixed speed-warning alert audio clips for a separate “Bitchin' Betty” voice-warning feature. Chatterbox **does not run in the app**, is **not** started from Navi, and has **no connection** to this plugin's on-device TTS (Piper/any-tts/etc.). Do not merge the two systems, share their models at runtime, or load Chatterbox as a plugin. |
| **“Bitchin' Betty” clips** | Playback of those **pre-rendered** files belongs with the warning/alert audio path, not with conversational TTS. |
| **POI live data** | Voice **defaults to offline** OSM/regional index. Live hours/pricing only via explicit opt-in, host-mediated, never from the voice guest ([§4.3](#43-nearestpoi--offline-default-optional-live-enrichment)). |
| **Saved locations** | Voice-created labels use `saved_places` and **must not** be pulled into a future cloud sync ([§4.2](#42-savecurrentlocation--sensitive-data)). |
| **Weather / tiles / `navi.app` packs** | Host may grow HTTPS for those features. The **voice WASM instance must remain unable to call them.** |
| **Routing NPU** | Out of scope ([§8](#8-npu-usage-and-fallback-path)). |
| **Platform / cloud ASR-TTS** | Rejected as primary ([§9](#9-candidate-technology-comparison)). |

---

## 11. Open questions requiring a decision before implementation begins

Do not treat recommended defaults below as already decided. Each needs an
explicit implementation-time choice.

| # | Question | Recommended default | Trade-offs |
|---|---|---|---|
| 1 | **Activation:** PTT only, wake-word only, or both? | **Both; wake-word default; PTT always available** | Accessibility vs. engineering cost and false accepts. PTT-only is faster to ship but fails the primary use case. |
| 2 | **Wake-word in WASM vs native?** | **Native spotter** ([§2.3](#23-wake-word-placement)) | Native is a larger trusted computing base; WASM would need a streaming isolation model. |
| 3 | **Wake phrase(s)** per language vs. one “Hey Navi”? | Start with **one proper-name phrase** plus localized packs when false-reject is high | One phrase is simpler; some locales will need a local greeting. |
| 4 | **v1 language list** | Not frozen. Spike should name a **minimum set** (at least OS-locale target markets + English fallback) | Coverage vs. APK size and test burden. |
| 5 | **Bundling:** all / on-demand / hybrid? | **Hybrid** ([§7.2](#72-model-bundling-open-choice--recommended-hybrid)) | Size vs. first-run offline vs. accessibility of the download UI. |
| 6 | **ASR engine** | Spike **`whisper-rs` first** for multilingual; evaluate **`vosk-rs`** if dialect packs + size dominate; do not commit Parakeet/voirs without a maturity check | Accuracy vs. size vs. WASM/Android link reality. Re-verify all four. |
| 7 | **TTS engine / which Piper crate?** | Spike **`piper1-rs` + ONNX** first (same gate as voice-guidance); pin **one** crate, not the name “piper-rs” | GPL, ONNX-on-Android, voice coverage. `any-tts` only if Piper lacks a required language. |
| 8 | **Voice WASM fuel / timeout** | Host-driven **chunked infer** with per-chunk epoch limits, or a documented raised budget **only** on this instance | Tight defaults kill ASR; unbounded fuel weakens isolation. |
| 9 | **Confirmation timeout** | **6 s**, one retry, then cancel ([§4.5](#45-spoken-confirmation-follow-ups)) | Short timeouts strand slow speakers; long timeouts leave the mic open. |
| 10 | **`NavigateTo` auto-start vs. always confirm?** | **Always speak the resolved destination and ask yes/no** before planning | Extra step vs. sending someone to the wrong “Springfield.” Accessibility still satisfied if yes/no is spoken. |
| 11 | **Fuel / gas-station `PoiCategory`** | Add a host **Fuel** (OSM `amenity=fuel`) category (or equivalent query) before advertising the utterance | Without it, “nearest gas station” cannot be honest. Scope of other spoken categories (parking, hospital, …) also needs a v1 list. |
| 12 | **NameIndex alt-name ingest** | Required for fuzzy matching quality ([§4.4](#44-fuzzy-place-name-matching)); schedule as a core search change, not a guest hack | Index size and rebuild cost for regional PBF. |
| 13 | **Share Piper runtime with voice-guidance?** | **Allowed to share ONNX/runtime**, not required; guidance stays recordings-first | Memory vs. duplication. Do not couple ship dates. |
| 14 | **Live POI enrichment UX** | Default **off**; enable only via explicit spoken modifier **or** a dedicated setting, never both silently | Discoverability vs. accidental network use. |
| 15 | **AEC / barge-in** | **v1: no**; gate only | Interruptibility vs. cabin AEC cost. |
| 16 | **Log redaction vs. supportability** | Default: intent enum + timings; opt-in command-text debug | Debugging mishears vs. leaving “mom's” on an MTP volume. |

---

## Host capabilities (proposed)

Declare in the voice plugin's **own** policy set only after HostApi exists.
Do **not** add these symbols to the global linker used by camping/weather
guests unless those guests need them (they should not).

| Capability | Purpose |
|---|---|
| `voice_audio` | `voice_audio_in` — bounded PCM from host |
| `voice_text` | `voice_text_out` — transcript / intent JSON |
| `voice_pcm` | `voice_pcm_out` — TTS PCM to host playback |
| `npu_infer` (optional) | Opaque host ML infer (CPU or NPU) |
| `model_weight_read` (optional) | Read-only window into the loaded weight blob |
| `log` | Redacted diagnostics |

Orchestration uses existing host/UniFFI surfaces, **not** guest imports:
`searchPlaces` / NameIndex, `PoiIndex`, `saved_places`, corridor plan, OS
locale, audio device. Proposed caps from other specs (`voice_speak` for
guidance clips, `poi_query` for camping) stay on **those** instances.

---

## Implementation checklist (future)

1. Wasmtime upgrade gate ([`plugins.md`](../plugins.md)).
2. Dedicated `PluginHost` / linker with the voice-only import table; tests
   that network/fs/`poi_query` imports are **not** callable (negative tests).
3. Native capture + hard gate + playback; isolation tests that TTS PCM never
   reaches ASR.
4. PTT path; then wake-word spike (native) with disable releasing the mic.
5. ASR/TTS crate spike on Android aarch64 (CPU first, then NNAPI bridge).
6. Phrase-pattern packs + shared `VoiceIntent` enum; no LLM.
7. Host matching: `saved_places` + NameIndex; alt_name ingest; Fuel POI
   category if still missing.
8. Confirmation state machine and timeouts.
9. Locale / gender settings; hybrid pack install UX; fallback speech.
10. USB debug tree under `Documents/debug/voice-command/` with redaction.
11. Enable/disable; core nav works with the plugin off.

---

## Related docs

| Doc | Why |
|---|---|
| [`plugins.md`](../plugins.md) | Host, capabilities, wasmtime gate, enable/disable |
| [`architecture.md`](../architecture.md) | Core vs plugin-host, search/route data paths |
| [`voice-guidance.md`](../voice-guidance.md) | Turn-by-turn speech; Piper/ONNX research; **not** this plugin |
| [`map-marking-saved-places.md`](../map-marking-saved-places.md) | Local `saved_places` |
| [`poi.md`](../poi.md) | Offline POI categories |
| [`i18n-translation-spec.md`](i18n-translation-spec.md) | UI locale vs GPS; parallel to voice locale rules |
| [`precomputed-index-and-route-cache.md`](../precomputed-index-and-route-cache.md) | Future `navi.app` pack mirror — must stay off the voice import table |
