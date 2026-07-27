# Contributing to Navi

Thank you for considering a contribution. Navi is a **solo-maintained**,
**AI-assisted** offline navigation project (see the [AI assistance](README.md#ai-assistance)
note in the README — that background is intentional and ongoing). Contributions
of **all kinds** are welcome: code, testing, documentation, translations,
jurisdiction packs, plugins, icons, and bug reports from real driving.

Start with [`docs/architecture.md`](docs/architecture.md) for how the pieces fit
together (`core`, `plugin-host` / `plugin-sdk`, Android `app`, UniFFI). For
“where do I change X?”, use [`docs/codebase-map.md`](docs/codebase-map.md).

---

## Ways to contribute (beyond code)

These match the project’s **actual current needs**, not a generic “PRs welcome”
list.

### Hardware testing

Development has been **emulator-heavy**. Real Android Automotive / head-unit
hardware (Xtrons, Atoto, Joying, and similar) and rugged/tablet form factors
differ for GPS, MapLibre, Vulkan/GLES, and performance. That gap is explicit —
use the checklist in [`docs/real-hardware-testing.md`](docs/real-hardware-testing.md)
and report what you ran, on which device, and what failed.

### Regional testing

Most routing / POI / eco evidence so far is **Norway-centric** (Østlandet
extracts, DNT-style networks). Accuracy testing outside Norway — other Geofabrik
regions, local road classes, rest/overnight POIs, official trail networks —
is highly valuable. Prefer real regional PBFs and real GPS when practical.

### Translation / localization

- **UI language packs** are specified but not shipped (English-only UI today):
  [`docs/plugins/i18n-translation-spec.md`](docs/plugins/i18n-translation-spec.md).
- **Voice guidance** is planned as a plugin; phrase design must respect
  per-language **concatenation vs whole-phrase** recordings — see
  [`docs/voice-guidance.md`](docs/voice-guidance.md).
- Parallel documentation (`Norwegian.md`, `docs/bilder.md`, …) is welcome;
  that is **docs**, not in-app i18n.

### Jurisdiction rule sets

Country/region packs (driving-hour families, right-to-roam / outdoor-access,
future horse-access style rules) follow the pattern in
[`docs/jurisdiction-rules.md`](docs/jurisdiction-rules.md). Adding a pack for a
jurisdiction you know well — with honest “decline rather than guess” behaviour
when rules are unclear — is a first-class contribution.

### Plugin development

The **plugin host is implemented and tested**; **product content plugins are
not shipped yet on purpose**. Specs exist for contributors to pick up (camping /
allemannsretten, safety resupply, instrument cluster / AGL, UI translation,
animated icons, voice, APRS/CAT, …). See [`docs/plugins.md`](docs/plugins.md)
and files under [`docs/plugins/`](docs/plugins/). This is an open invitation, not
an incomplete core feature.

### Signs and icons

Custom static icons: SVG / SVGZ, Inkscape workflow, semantic key naming —
[`docs/icons.md`](docs/icons.md). Animated icons: Synfig → frame packs —
[`docs/plugins/animated-icons-spec.md`](docs/plugins/animated-icons-spec.md).
Regional road **warning / traffic sign** artwork and licensing review are
welcome; any new asset must document provenance the same way as the Navit /
APRS sets (see Licensing below).

### Bug reports from real usage

Especially issues that only appear on **real hardware** or **real driving**
(GPS drift, GPU quirks, thermal throttling, head-unit permissions). Unit tests
alone have missed several of these; a short reproduction with device model,
OS build, region PBF, and logcat / screenshots helps far more than “it broke.”

---

## Development environment

Do not duplicate long build recipes here — use the maintained guides:

| Goal | Doc |
|---|---|
| Rust core, host integration tests, optional gpsd/IMU | [`docs/build-linux.md`](docs/build-linux.md) |
| Native `libnavi.so`, UniFFI Kotlin, Gradle APK, AAOS emulator | [`docs/android-build.md`](docs/android-build.md) |
| Host + Android debug loops | [`docs/debugging.md`](docs/debugging.md) |
| Launch / install on emulator | `./scripts/launch-navi-emulator.sh` (see Android build doc) |

Workspace layout is summarized in the README and
[`docs/architecture.md`](docs/architecture.md).

---

## Code contribution expectations

These are the standards the project has held to in practice:

1. **Evidence over claims**  
   Compiling or a green unit test is not “done” for user-facing behaviour.
   Prefer route-level or on-device evidence: screenshots, log excerpts with
   numbers, or a described real test run (emulator GPS / physical device).

2. **Real GPS / real data where practical**  
   Prefer live or emulator GPS fixes and real region extracts over hardcoded
   synthetic corridors when a real fixture is reasonably obtainable. Synthetic
   stubs are for unit geometry only, not as a substitute for routing PASS.

3. **Full command output in issues / PRs**  
   Do not paste truncated shell output via `tail` / `head` (or similar) when
   sharing failures — full relevant output preferred so context is not lost.

4. **Document known limitations honestly**  
   If something is deferred, stub, simplified, or emulator-only, say so in
   docs or comments. Overstated “Done” labels have already had to be corrected
   in-tree; accuracy beats polish.

5. **Match existing style**  
   Follow nearby code and docs (plain professional text, no emojis in code or
   documentation). Ask before large refactors unrelated to the change.

Repository code license: see root `LICENSE` (GPL-3.0-or-later unless noted).

---

## Licensing awareness (assets)

Icon and symbol provenance is tracked explicitly:

- Bundled POI / nav / status icons under `core/src/icons`: **Navit-derived GPL v2**
  — [`docs/icons.md`](docs/icons.md).
- Custom SVG overrides: document **your** license next to the override set.
- APRS symbol sets: per-symbol notes under `core/src/icons/aprs/COPYRIGHT.md`
  (and the app asset copy).

Contributed artwork must include the same tracking. Do not drop opaque binary
icon packs without license text.

---

## Discuss before large work

Before starting a **substantial** effort (new plugin, new jurisdiction pack,
major UI surface, new radio/ECU ingest path), please open a **GitHub Discussion**
or **Issue** first. That avoids duplicated work and misalignment with project
direction — especially important here because AI assistance is used to help turn
**clear maintainer direction** into code. A short design note up front saves
everyone time.

Small fixes, docs typos, and “I tested device X and found Y” reports do not need
a prior discussion — open a PR or issue directly.

---

## Pull requests

- Keep PRs focused; one concern per PR when practical.
- Include the evidence you used (test names, device, region, screenshot paths).
- Do not force-push to `main`; do not skip hooks unless the maintainer asks.
- The maintainer may ask clarifying questions before merge — that is normal for
  a solo-maintained tree.

### CI expectations (GitHub Actions)

Every push/PR runs [`.github/workflows/ci.yml`](.github/workflows/ci.yml):

| Job | What it checks |
|---|---|
| `rust-checks` | `cargo fmt --check`, Clippy (`-D warnings`), `cargo test --workspace` (default set only; excludes `navi-desktop` and wasm example guests), plugin-host `isolation` tests, `cargo deny`, `cargo audit` |
| `linux-build` | Workspace `cargo build` on Linux (headless `navi-desktop` features; excludes WebKit and wasm guests) |
| `kotlin-checks` | ktlint, detekt, `./gradlew :app:testDebugUnitTest` |
| `android-build` | `./gradlew :app:assembleDebug` |

**Not** in the per-PR gate (run locally or via the scheduled workflow):

- Rust `#[ignore]` OSM/DEM integration tests (need fixtures under
  `core/target/integration-fixtures` — see [`docs/build-linux.md`](docs/build-linux.md)).
- Android instrumented tests — [`.github/workflows/android-instrumented.yml`](.github/workflows/android-instrumented.yml)
  (nightly / `workflow_dispatch` with an emulator).

Before opening a PR, prefer running at least:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets \
  --exclude navi-plugin-log-hello --exclude navi-plugin-busy-loop --exclude navi-desktop \
  -- -D warnings
cargo test --workspace \
  --exclude navi-desktop --exclude navi-plugin-log-hello --exclude navi-plugin-busy-loop
cargo test -p navi-plugin-host --test isolation
cargo deny check
./gradlew :app:ktlintCheck :app:detekt :app:testDebugUnitTest :app:assembleDebug
```

Rust toolchain is pinned by `rust-toolchain.toml` (MSRV **1.88**).
