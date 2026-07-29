# Android API 36 baseline plan

Status: **plan only** (not executed in the 2026-07-29 future-proofing fix pass).
Last updated: 2026-07-29.

Current baseline (`app/build.gradle.kts`): `compileSdk` / `targetSdk` **35**,
`minSdk` **26**, AGP **8.7.3**, Kotlin **2.0.21**, Compose BOM **2024.10.01**.

Target stable platform: Android 16 / API **36**.

## Goals

1. Raise `compileSdk` and `targetSdk` to 36.
2. Align AGP / Kotlin / Compose BOM to versions that officially support SDK 36
   (confirm against Android Studio / AGP release notes at execution time).
3. Re-run full local CI (Rust workspace + Gradle checks) before any push.

## Navi-relevant API 36 behavior (pre-bump review)

Sources: [Behavior changes for apps targeting Android 16+](https://developer.android.com/about/versions/16/behavior-changes-16),
[all-apps changes](https://developer.android.com/about/versions/16/behavior-changes-all).

| Area | Relevance to Navi | Action when bumping |
|---|---|---|
| Health / `BODY_SENSORS` → granular `android.permission.health` | **Low** — manifest currently has location + network only; no `BODY_SENSORS` | Confirm still unused; skip unless IMU/health plugins ship |
| Foreground service location types (`location` / `navigation`) | **Medium** — today location is Activity/`Fused`-style usage in `MainActivity`; no FGS location declared | If a navigation FGS is added later, declare `foregroundServiceType` correctly before target 36 |
| Background location | **Medium** — fine/coarse location requested at runtime; no `ACCESS_BACKGROUND_LOCATION` in manifest today | Keep while-in-use model unless AAOS product requires background; then follow Play/AAOS policy |
| Local network permission (target 36+) | **Medium** — LAN HTTP fixture / optional DIY telemetry / gpsd-style LAN use | Audit LAN clients at bump time; add permission + UX if required |
| Partial photo/video access | **Low** — app is not a media picker host | Skip unless file-pick flows expand |
| `JobInfo#setImportantWhileForeground` inert | **Low** unless WorkManager/JobScheduler paths use it | Grep before bump |
| Edge-to-edge / large-screen / predictive back | **Medium for AAOS** | Visual retest on Automotive AVD + real hardware checklist |

## Suggested version alignment (verify at execution)

Do not treat these as pins until the bump PR measures them against green CI:

| Component | Today | Direction |
|---|---|---|
| `compileSdk` / `targetSdk` | 35 | 36 |
| Android Gradle Plugin | 8.7.3 | Latest 8.x/9.x that supports SDK 36 and this Gradle wrapper |
| Kotlin | 2.0.21 | Stay on a Compose-compiler-compatible release; bump with Compose BOM |
| Compose BOM | 2024.10.01 | A BOM dated after SDK 36 tooling GA |
| Gradle wrapper | (see `gradle/wrapper`) | Match AGP requirements |

## Execution checklist (when doing the bump)

1. Install SDK Platform 36 locally; update CI images if needed.
2. Bump `compileSdk`/`targetSdk` only first; fix compile errors.
3. Bump AGP/Kotlin/Compose as required for clean builds.
4. Grep for deprecated APIs flagged above; fix FGS / permission / JobScheduler hits.
5. Local CI sequence (same as `CONTRIBUTING.md`):

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

6. Emulator smoke on API 36 AVD + AAOS AVD; update `docs/android-build.md` and README SDK lines.
7. Mark priority 5 closed in [`future-proofing-audit-2026-07.md`](future-proofing-audit-2026-07.md) with the bump date.

## Explicit non-goals for the plan-only pass

- No SDK / AGP version change in-tree until the checklist above is run green.
- No Play Console targeting-deadline work beyond documenting this plan.
