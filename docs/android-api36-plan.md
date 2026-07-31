# Android API 36 baseline plan

Status: **executed** (2026-07-31). Last updated: 2026-07-31.

Current baseline (`app/build.gradle.kts`): `compileSdk` / `targetSdk` **36**,
`minSdk` **26**, AGP **8.9.1**, Kotlin **2.0.21**, Compose BOM **2024.10.01**,
Gradle wrapper **8.11.1**.

`dependenciesInfo { includeInApk = false; includeInBundle = false }` is set.

## Goals

1. Raise `compileSdk` and `targetSdk` to 36. **Done.**
2. Align AGP / Kotlin / Compose BOM to versions that officially support SDK 36.
   AGP bumped **8.7.3 → 8.9.1** (minimum for API 36). Kotlin / Compose BOM kept.
3. Re-run full local CI (Rust workspace + Gradle checks) after the bump.

## Navi-relevant API 36 behavior (pre-bump review)

Sources: [Behavior changes for apps targeting Android 16+](https://developer.android.com/about/versions/16/behavior-changes-16),
[all-apps changes](https://developer.android.com/about/versions/16/behavior-changes-all).

| Area | Relevance to Navi | Action when bumping |
|---|---|---|
| Health / `BODY_SENSORS` → granular `android.permission.health` | **Low** — manifest currently has location + network only; no `BODY_SENSORS` | Confirmed still unused |
| Foreground service location types (`location` / `navigation`) | **Medium** — today location is Activity/`Fused`-style usage in `MainActivity`; no FGS location declared | No FGS added; keep while-in-use model |
| Background location | **Medium** — fine/coarse location requested at runtime; no `ACCESS_BACKGROUND_LOCATION` in manifest today | Kept while-in-use; no background permission |
| Local network permission (target 36+) | **Medium** — LAN HTTP fixture / optional DIY telemetry / gpsd-style LAN use | No new LAN permission required for current APK surface; revisit if DIY telemetry ships |
| Partial photo/video access | **Low** — app is not a media picker host | Skip |
| `JobInfo#setImportantWhileForeground` inert | **Low** unless WorkManager/JobScheduler paths use it | Grep clean |
| Edge-to-edge / large-screen / predictive back | **Medium for AAOS** | Visual retest on Automotive AVD + real hardware checklist (ongoing) |

## Execution checklist

1. Install SDK Platform 36 locally. **Done** (`platforms;android-36`).
2. Bump `compileSdk`/`targetSdk` to 36 + AGP 8.9.1. **Done.**
3. Add `dependenciesInfo` omit flags + local upload signing for AAB smoke. **Done.**
4. Build signed AAB; validate with `bundletool`; install APK set on SM-P613. **Done**
   (`targetSdk=36` confirmed via `dumpsys`).
5. Local CI sequence (same as `CONTRIBUTING.md`) after bump — see run log.
6. F-Droid Podman buildability check: `tools/fdroid-check/`.

## AAB smoke (host)

```bash
./scripts/make-upload-keystore.sh
./scripts/build-android-native.sh aarch64-linux-android release
./scripts/build-android-native.sh x86_64-linux-android release
./gradlew :app:bundleRelease
java -jar bundletool.jar validate --bundle=app/build/outputs/bundle/release/app-release.aab
java -jar bundletool.jar build-apks --bundle=... --output=/tmp/navi.apks \
  --ks=app/keystore/navi-upload.jks --ks-pass=pass:navi-upload-local \
  --ks-key-alias=navi-upload --key-pass=pass:navi-upload-local
# Uninstall any debug-signed build first, then:
java -jar bundletool.jar install-apks --apks=/tmp/navi.apks
```

Upload keystore under `app/keystore/` is gitignored (local smoke only — not Play
production signing).
