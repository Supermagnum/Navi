# Contributing to Navi

Thank you for considering a contribution. Navi is a **solo-maintained**,
**AI-assisted** offline navigation project (see the [AI assistance](../README.md#ai-assistance)
note in the README — that background is intentional and ongoing). Contributions
of **all kinds** are welcome: code, testing, documentation, translations,
jurisdiction packs, plugins, icons, and bug reports from real driving.

Start with these orientation docs before changing code:

| Doc | Use it for |
|---|---|
| [`architecture.md`](architecture.md) | How the pieces fit together (`core`, `plugin-host` / `plugin-sdk`, Android `app`, UniFFI) |
| [`codebase-map.md`](codebase-map.md) | Where to change a given feature (files for zoom, HUD, routing, rest, map styles, …) |
| [`API.md`](API.md) | Callable surfaces: UniFFI host API, plugin HostApi, and what is not an API |

---

## Fork from `dev` and basic GitHub usage

The public repo is [github.com/Supermagnum/Navi](https://github.com/Supermagnum/Navi).

**Branches:** New work lands on **`dev`**. **`main`** is GitHub’s default clone
target. You fork the **whole repository**, then work from **`dev`**. GitHub
does not fork a single branch by itself; after the fork, check out `dev`.

If you only want to **build or install** and will not open a pull request, clone
the upstream repo and `git checkout dev` as in the [README](../README.md#building-and-installing).
That checkout cannot receive your pull requests. Use a **fork** for contributions.

### 1. Register a GitHub account (if needed)

You need a free GitHub account to fork the repo and open pull requests.

1. Open [github.com/signup](https://github.com/signup) (or **Sign up** on
   [github.com](https://github.com)).
2. Enter an email you can access, a password, and a username. Confirm you are
   not a robot when asked.
3. Verify the account from the email GitHub sends (check spam if it is missing).
4. Optional but recommended: turn on
   [two-factor authentication](https://docs.github.com/en/authentication/securing-your-account-with-two-factor-authentication-2fa)
   under **Settings → Password and authentication**.
5. You do not need a paid plan for contributing to Navi.

Official help: [Creating an account on GitHub](https://docs.github.com/en/get-started/start-your-journey/creating-an-account-on-github).

### 2. Fork the repository

1. Sign in to GitHub.
2. Open [github.com/Supermagnum/Navi](https://github.com/Supermagnum/Navi).
3. Click **Fork** (top right). Keep the name `Navi` unless you have a reason
   not to. You now have `https://github.com/<your-user>/Navi`.
4. On **your** fork, use the branch dropdown and select **`dev`**. The fork’s
   default branch is usually **`main`** (same as upstream). Do not start work
   from `main` unless you mean to.

### 3. Clone your fork and check out `dev`

Install Git if needed (`sudo apt install git` on Debian/Ubuntu; other systems:
[`build-linux.md`](build-linux.md#getting-the-code)).

```bash
git clone https://github.com/<your-user>/Navi.git
cd Navi
git checkout dev
```

A plain `git clone` checks out **`main`**. Always switch to **`dev`** before
new work. One-step:

```bash
git clone -b dev https://github.com/<your-user>/Navi.git
cd Navi
```

If `dev` is missing locally: `git fetch origin` then
`git checkout -b dev origin/dev`.

### 4. Add `upstream` (the original repo)

Your clone’s `origin` is **your fork**. Add the Navi repo as `upstream` so you
can pull new commits:

```bash
git remote add upstream https://github.com/Supermagnum/Navi.git
git fetch upstream
git remote -v
```

`origin` should be `https://github.com/<your-user>/Navi.git`.  
`upstream` should be `https://github.com/Supermagnum/Navi.git`.

### 5. Keep your `dev` in sync

Before starting a change (and when GitHub shows your fork is behind):

```bash
git checkout dev
git fetch upstream
git merge upstream/dev
git push origin dev
```

On the GitHub website you can also open your fork and use **Sync fork**.
Do not force-push to `dev` or `main`.

### 6. Branch, push, and open a pull request

1. Create a topic branch **from latest `dev`**, not from `main`:

   ```bash
   git checkout dev
   git fetch upstream
   git merge upstream/dev
   git checkout -b my-change
   ```

2. Make the change, commit, then push to **your fork**:

   ```bash
   git push -u origin my-change
   ```

3. On GitHub, open a **Pull request**.
   - **base repository:** `Supermagnum/Navi`
   - **base branch:** **`dev`** (not `main`)
   - **compare:** your `my-change` branch on your fork

You cannot push directly to `Supermagnum/Navi` unless the maintainer has given
you write access. Fork plus pull request is the normal path.

### When to use Issue, Discussion, or Pull request

| Use | For |
|---|---|
| **Issue** | Bugs, hardware reports, small requests |
| **Discussion** | Design questions before large work (see below) |
| **Pull request** | A proposed change; must target **`dev`** |

---

## Ways to contribute (beyond code)

These match the project’s **actual current needs**, not a generic “PRs welcome”
list.

### Hardware testing

Development has been **emulator-heavy**. Real Android Automotive / head-unit
hardware (Xtrons, Atoto, Joying, and similar) and rugged/tablet form factors
differ for GPS, MapLibre, Vulkan/GLES, and performance. That gap is explicit —
use the checklist in [`real-hardware-testing.md`](real-hardware-testing.md)
and report what you ran, on which device, and what failed.

### Regional testing

Most routing / POI / eco evidence so far is **Norway-centric** (Østlandet
extracts, DNT-style networks). Accuracy testing outside Norway — other Geofabrik
regions, local road classes, rest/overnight POIs, official trail networks —
is highly valuable. Prefer real regional PBFs and real GPS when practical.

### Translation / localization

- **UI language packs** are specified but not shipped (English-only UI today):
  [`plugins/i18n-translation-spec.md`](plugins/i18n-translation-spec.md)
  and the working catalog [`plugins/translations.csv`](plugins/translations.csv)
  with sense notes in
  [`plugins/translations-context.md`](plugins/translations-context.md).
  The English column header lists countries/regions; dialect columns use
  `country, - area, - dialect`. Do not infer UI language from GPS or SIM
  country; do not add a language toggle until that plugin exists. Fallback to
  English is part of the spec.
- **Voice guidance** is planned as a plugin; phrase design must respect
  per-language **concatenation vs whole-phrase** recordings — see
  [`voice-guidance.md`](voice-guidance.md).
- Parallel documentation (`docs/Norwegian.md`, `docs/bilder.md`, …) is welcome;
  that is **docs**, not in-app i18n.

### Jurisdiction rule sets

Country/region packs (driving-hour families, right-to-roam / outdoor-access,
future horse-access style rules) follow the pattern in
[`jurisdiction-rules.md`](jurisdiction-rules.md). Adding a pack for a
jurisdiction you know well — with honest “decline rather than guess” behaviour
when rules are unclear — is a first-class contribution.

### Plugin development

The **plugin host is implemented and tested**; **product content plugins are
not shipped yet on purpose**. Specs exist for contributors to pick up (camping /
allemannsretten, safety resupply, instrument cluster / AGL, UI translation,
animated icons, custom alert sounds, adaptive speed warning, horse trekking,
voice, APRS/CAT, …). See [`plugins.md`](plugins.md)
and files under [`plugins/`](plugins/). This is an open invitation, not
an incomplete core feature.

### Signs and icons

Custom static icons: SVG / SVGZ, Inkscape workflow, semantic key naming —
[`icons.md`](icons.md). Animated icons: Synfig → frame packs —
[`plugins/animated-icons-spec.md`](plugins/animated-icons-spec.md).
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
| Where to edit a feature | [`codebase-map.md`](codebase-map.md) |
| UniFFI / plugin HostApi | [`API.md`](API.md) |
| Crate wiring / databases | [`architecture.md`](architecture.md) |
| Rust core, host integration tests, optional gpsd/IMU (Linux) | [`build-linux.md`](build-linux.md) |
| macOS build host (tools, Android NDK, adb) | [`build-macos.md`](build-macos.md) |
| Windows build host (MSVC, tools, Android NDK, adb) | [`build-windows.md`](build-windows.md) |
| Native `libnavi.so`, UniFFI Kotlin, Gradle APK, AAOS emulator | [`android-build.md`](android-build.md) |
| Host + Android debug loops | [`debugging.md`](debugging.md) |
| Launch / install on emulator | `./scripts/launch-navi-emulator.sh` (see Android build doc) |

Workspace layout is summarized in the README and
[`architecture.md`](architecture.md).

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
  — [`icons.md`](icons.md).
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

Fork from **`dev`** first ([Fork from `dev` and basic GitHub usage](#fork-from-dev-and-basic-github-usage)).
Then:

- Keep PRs focused; one concern per PR when practical.
- Include the evidence you used (test names, device, region, screenshot paths).
- Target pull requests at **`dev`** (that is where new work lands). Do not open
  PRs against `main` unless the maintainer asks.
- **CI must pass locally before you open a PR.** Run the commands in
  [CI expectations](#ci-expectations-github-actions) and do not open the PR
  until they succeed. GitHub Actions is a second check, not a substitute for a
  local run.
- Do not force-push to `main` or `dev`; do not skip hooks unless the maintainer asks.
- The maintainer may ask clarifying questions before merge — that is normal for
  a solo-maintained tree.

### CI expectations (GitHub Actions)

Every push to **`main`** or **`dev`**, and every PR, runs
[`.github/workflows/ci.yml`](../.github/workflows/ci.yml):

| Job | What it checks |
|---|---|
| `rust-checks` | `cargo fmt --check`, Clippy (`-D warnings`), `cargo test --workspace` (default set only; excludes `navi-desktop` and wasm example guests), plugin-host `isolation` tests, `cargo deny`, `cargo audit` |
| `linux-build` | Workspace `cargo build` on Linux (headless `navi-desktop` features; excludes WebKit and wasm guests) |
| `kotlin-checks` | ktlint, detekt, `./gradlew :app:testDebugUnitTest` |
| `regression-guards` | Curated host JVM guards for past download/restore bugs (parallel; no emulator). See below. |
| `android-build` | `./gradlew :app:assembleDebug` |

**`regression-guards`** (per-PR, distinct job so rust/kotlin are not serialized behind it):

- `OfflinePmtilesBootstrapTest` / `OfflineDataIntegrityRestoreTest` — mz12 fixture must not complete or offer production restore (the download-completion gap).
- `CoordinateInputTest` — lat/lon parse (was instrumented-only).
- `PlaceSearchHintTest` — skip live graph work while a foreground plan is active.
- `motor_access_barrier` / `wetland_apply_identity` / `wetland_pack_identity` — Torggata/Kirkebyskogen access and wetland pack-vs-PBF against checked-in mini PBFs under `core/tests/fixtures/`.

Rust counterparts already in `rust-checks` / `regression-guards` (not `#[ignore]`): `bike_suitability_route`, wetland tag/precedence unit tests, `download::pbf_priority` (cone/plan skip), `basemap::extract::validate_rejects_mz12_large_region_fixture`, `motor_access_barrier`, and wetland pack-vs-PBF (`wetland_apply_identity` / `wetland_pack_identity`) against checked-in mini PBFs under `core/tests/fixtures/` (~1.6 MiB; regenerate via `scripts/cut-corridor-extract.py`).

**Not** in the per-PR gate (run locally or via manual workflow dispatch):

- Rust `#[ignore]` OSM/DEM integration tests (need fixtures under
  `core/target/integration-fixtures` — see [`build-linux.md`](build-linux.md)).
  Cached full Ostlandet (~450 MB PBF + graph build) is still multi-minute
  (`kongsvinger_lillehammer_integration` ~232 s after fixtures) and live-network
  extracts stay off the PR gate.
- Android instrumented tests — [`.github/workflows/android-instrumented.yml`](../.github/workflows/android-instrumented.yml)
  (`workflow_dispatch` only; not a required check — see
  [`real-hardware-testing.md`](real-hardware-testing.md#github-hosted-instrumented-ci)).
  Screenshot classes are evidence captures, not golden pixel-diff; GitHub
  SwiftShader is not a trusted MapLibre renderer. Host stand-ins for the
  completion-guard live in `regression-guards` instead of booting an emulator.

**Required** before opening a PR: the same gate as GitHub Actions must already
be green **on your machine**. Do not open a PR hoping CI will catch failures.
Run at least:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets \
  --exclude navi-plugin-log-hello --exclude navi-plugin-busy-loop --exclude navi-desktop \
  -- -D warnings
cargo test --workspace \
  --exclude navi-desktop --exclude navi-plugin-log-hello --exclude navi-plugin-busy-loop
cargo test -p navi-plugin-host --test isolation
cargo deny check
cargo audit
./gradlew :app:ktlintCheck :app:detekt :app:testDebugUnitTest :app:assembleDebug
# Optional explicit subset (also covered by testDebugUnitTest):
# ./gradlew :app:testDebugUnitTest --tests no.navi.app.OfflinePmtilesBootstrapTest \
#   --tests no.navi.app.OfflineDataIntegrityRestoreTest
```

Rust toolchain is pinned by `rust-toolchain.toml` (**1.88** reproducibility
pin — not a proven MSRV matrix; see [`build-linux.md`](build-linux.md)).

## Dependency maintenance watch

Run a short dependency freshness pass **at least once per quarter** (or before
any release / store submission). Checklist:

1. **Rust (crates.io)** — `cargo outdated -w` (or equivalent) on the workspace;
   skim `cargo deny check` / `cargo audit` (already in CI).
2. **Near-term flags** (called out by the 2026-07 future-proofing audit):
   - `wee_alloc` (example WASM guests only; `RUSTSEC-2022-0054` allowed in
     `deny.toml`) — **schedule replacement review** before shipping production
     guests that need a custom allocator; host does not use it. Swap can wait
     until a guest allocator is actually needed in a released plugin.
   - `gpsd_proto` (`core` feature `gpsd`) — last-release age was notably stale at
     audit time; **review for maintained fork / alternate** when next touching
     Linux GPS wiring. Keep until a drop-in is validated (pure TCP/JSON, no
     `libgps`).
3. **Lower-priority periodic checks:** `osm4routing`, `geotiff` — low churn;
   verify still compile and license-clean when bumping related map/DEM work.
4. **Gradle / Android** — Compose BOM, AGP, Kotlin, MapLibre; see
   [`android-api36-plan.md`](android-api36-plan.md) for the API 36 bump
   plan. Prefer one coordinated bump PR over silent drift.
5. Record the review date in the PR or in
   [`future-proofing-audit-2026-07.md`](future-proofing-audit-2026-07.md)
   priority-6 notes when closing a watch cycle.

This is a **process** deliverable: do not mass-upgrade dependencies in the same
PR as unrelated features.
