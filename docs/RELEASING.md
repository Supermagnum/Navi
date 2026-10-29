# Releasing Navi (F-Droid + GitHub APK)

This documents the reproducible release path: F-Droid builds from source, then
verifies the unsigned APK content against the GitHub release asset
(`Binaries` + `AllowedAPKSigningKeys`).

## Versioning

- `versionName` / `versionCode` live in `app/build.gradle.kts`.
- Fastlane changelog: `fastlane/metadata/android/en-US/changelogs/<versionCode>.txt`.
- Git tag on **main**: `v<versionName>` (example: `v0.3.29-beta`).
- Release asset filename: `navi-<versionName>.apk` (example: `navi-0.3.29-beta.apk`).

Do **not** push the tag until the order below says so.

## Create the release keystore (once)

Run this yourself (never commit the `.jks` or passwords):

```bash
keytool -genkeypair -v \
  -keystore navi-release.jks \
  -keyalg RSA -keysize 2048 -validity 10000 \
  -alias navi-release \
  -storepass 'YOUR_STORE_PASSWORD' \
  -keypass 'YOUR_KEY_PASSWORD' \
  -dname 'CN=Navi Release, OU=Navi, O=Navi, L=Local, ST=NA, C=NO'
```

## GitHub repository secrets

Settings → Secrets and variables → Actions. Add:

| Secret | Value |
|---|---|
| `NAVI_KEYSTORE_BASE64` | `base64 -w0 navi-release.jks` (no newlines) |
| `NAVI_KEYSTORE_PASSWORD` | keystore password |
| `NAVI_KEY_ALIAS` | `navi-release` (or the alias you chose) |
| `NAVI_KEY_PASSWORD` | key password |

Never print these in CI logs or commit them.

## Certificate SHA-256 for `AllowedAPKSigningKeys`

After you have a signed APK (or from the keystore via a one-off sign):

```bash
apksigner verify --print-certs navi-0.3.29-beta.apk
```

Take **Signer #1 certificate SHA-256 digest**, lowercase, **no colons**
(F-Droid `AllowedAPKSigningKeys` format). Example shape:

```text
8de69b0ac58303e2b1237173265d8ddee41cd821ec1be66e44562c9fb8c66e7b
```

(That example is the local *upload* sideload key — the F-Droid release key will
differ if you generated a new `navi-release.jks`.)

## What the release workflow does

`.github/workflows/release.yml` triggers on tags `v*` and on manual
`workflow_dispatch`:

Shared build (both triggers):

1. Installs Rust **1.98.0** (matches `rust-toolchain.toml`) and NDK **r27c**.
2. Deletes any `app/src/main/jniLibs/*/libnavi.so` prebuilts.
3. Runs `scripts/build-android-native.sh all release` (builds Hamlib from
   `scripts/hamlib-android.lock` when missing; Cargo profile
   `release-android` with `panic = "abort"`, reproducible `RUSTFLAGS`).
4. Asserts `libhamlib.so` is present for both ABIs, then
   `./gradlew :app:assembleRelease -Pnavi.unsignedRelease=true`
   (`SOURCE_DATE_EPOCH=1700000000`).

Tag path (`v*` only):

5. `zipalign` + `apksigner` with the secrets (not Gradle `signingConfig`).
6. Uploads `navi-<versionName>.apk` to the GitHub Release for that tag.

Manual `workflow_dispatch` path:

5. Uploads the unsigned APK as a **workflow artifact only** (no signing, no
   GitHub Release). Use this to compare CI vs local unsigned APKs
   (`apksigcopier compare` / `diffoscope`) before tagging.

## Local F-Droid recipe check

```bash
./tools/fdroid-check/build-image.sh    # once / after Dockerfile changes
./tools/fdroid-check/run-podman-build.sh
```

## Release order

1. All release changes are committed and **pushed to `dev`**, and reproducibility
   checks pass (Podman / dual-path unsigned APK compare).
2. Merge **`dev` into `main`** and push `main` (releases are tagged on `main`).
3. Create the keystore and GitHub secrets; record the cert SHA-256.
4. Open the **fdroiddata** MR with the YAML from the release notes / agent
   report (`Binaries`, `AllowedAPKSigningKeys`, Builds entry). Wait until it
   merges.
5. Push tag `v<versionName>` **on `main`**. The workflow attaches the signed
   APK. F-Droid builds, verifies against it, and publishes.

## Reproducibility notes

- `scripts/build-android-native.sh all release` builds with Cargo profile
  `release-android` (`panic = "abort"`) so path-sensitive `.gcc_except_table`
  LSDA encodings are not emitted; remaps `$CARGO_HOME`, `$RUSTUP_HOME`, the
  source root, the rustc sysroot, and `$ANDROID_NDK_HOME`; passes
  `-C link-arg=-Wl,--build-id=none` and `-C llvm-args=-threads=1`.
- `scripts/build-plugin-wasm.sh` also remaps those path prefixes so plugin
  `.wasm` panic strings do not embed `/home/<user>/.cargo`.
- Pin **NDK r27c** (`27.2.12479018`) to match F-Droid `ndk: r27c` and CI
  (`nttld/setup-ndk`). A newer host NDK (e.g. 27.3) changes packaged
  `libc++_shared.so` and breaks Binaries compare.
- **Hamlib is required** in release APKs: `lib/arm64-v8a/libhamlib.so` and
  `lib/x86_64/libhamlib.so`, with `libnavi.so` linked `DT_NEEDED libhamlib.so`
  via the `hamlib-ffi` feature. Release builds auto-run
  `scripts/build-hamlib-android.sh` (tag from `scripts/hamlib-android.lock`)
  when `libhamlib.so` is missing, and fail if it is still absent.
- `CARGO_INCREMENTAL=0` and `[profile.release] incremental = false`.
- Gradle: `vcsInfo { include = false }`, `-Pnavi.unsignedRelease=true` for the
  F-Droid/CI unsigned APK, `android.enableResourceOptimizations=false`.
- Prefer `SOURCE_DATE_EPOCH` in CI (set in the release workflow).
- Dual-path checks must use a frozen source tree (no concurrent edits to
  Kotlin/Compose sources between path A and path B); otherwise `classes*.dex`
  can differ while `libnavi.so` still matches.
