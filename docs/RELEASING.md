# Releasing Navi (F-Droid + GitHub APK)

This documents the reproducible release path: F-Droid builds from source, then
verifies the unsigned APK content against the GitHub release asset
(`Binaries` + `AllowedAPKSigningKeys`).

## Versioning

- `versionName` / `versionCode` live in `app/build.gradle.kts`.
- Fastlane changelog: `fastlane/metadata/android/en-US/changelogs/<versionCode>.txt`.
  That file is **user-facing** store text (plain sentences about what changed for
  the user). Do not put version headers, test-case names, branch names, unit-test
  or sideload wording, or held-back features in it.
- Choose `versionCode` as **upstream fdroiddata `CurrentVersionCode` + 1**.
  Upstream currently has **31** (`0.3.26-beta`), so the next release must use
  **versionCode 32 or higher** (never reuse 30 or 31).
- `tools/fdroid-check/metadata/no.navi.app.yml` must stay **byte-identical** to
  `metadata/no.navi.app.yml` on [fdroid/fdroiddata](https://gitlab.com/fdroid/fdroiddata)
  `master`. Paste the next MR recipe from the YAML block below, not from the
  mirror file.
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

## Paste-ready fdroiddata recipe (next MR)

Copy the YAML below into the fdroiddata MR for the next release. It keeps
upstream's VC 30 and VC 31 builds unchanged, appends VC 32 with the reproducible
`Binaries` / `AllowedAPKSigningKeys` draft (Hamlib/autoconf deps and build
gates), and sets `CurrentVersion` / `CurrentVersionCode`. Replace
`PLACEHOLDER_TAG_COMMIT` with the full 40-character hash of the tagged commit
on `main`, and `AllowedAPKSigningKeys` with the release cert SHA-256
(lowercase, no colons). Do **not** put this draft into
`tools/fdroid-check/metadata/no.navi.app.yml`.

```yaml
AntiFeatures:
  TetheredNet:
    en-US: Optional online features talk to specific hosts that are not trivial to
      replace without changing the app or a config the user does not see. Hosts include
      map packs from navigate-me.duckdns.org (Geofabrik-derived extracts), OpenFreeMap
      basemap tiles, Mapterhorn elevation, DATEX traffic feeds, and amateur-radio
      repeater bundles from the same pack server. Optional online place search (off
      by default) queries a Nominatim-compatible geocoder. Routing and navigation
      run offline after map data is on the device.
Categories:
  - Navigation
License: GPL-3.0-or-later
AuthorName: Navi contributors
WebSite: https://github.com/Supermagnum/Navi
SourceCode: https://github.com/Supermagnum/Navi
IssueTracker: https://github.com/Supermagnum/Navi/issues
Changelog: https://github.com/Supermagnum/Navi/releases
Donate: https://github.com/Supermagnum/Navi#support-navi

AutoName: Navi

RepoType: git
Repo: https://github.com/Supermagnum/Navi.git

Binaries: https://github.com/Supermagnum/Navi/releases/download/v%v/navi-%v.apk
AllowedAPKSigningKeys: REPLACE_WITH_RELEASE_CERT_SHA256_LOWERCASE_NO_COLONS

Builds:
  - versionName: 0.3.25-beta
    versionCode: 30
    commit: f5e923580e72408b4f72edde26c8ee0e15351fde
    timeout: 14400
    subdir: app
    sudo:
      - apt-get update
      - apt-get install -y --no-install-recommends build-essential curl ca-certificates
        python3 pkg-config libssl-dev rustup
    gradle:
      - yes
    prebuild:
      - rustup toolchain install 1.98.0 --profile minimal
      - rustup default 1.98.0
      - rustup target add aarch64-linux-android x86_64-linux-android wasm32-unknown-unknown
      - CLANG_PRE="$$NDK$$/toolchains/llvm/prebuilt/linux-x86_64/bin"
      - mkdir -p ../.cargo
      - echo '[target.x86_64-linux-android]' > ../.cargo/config.toml
      - echo "linker = \"$CLANG_PRE/x86_64-linux-android34-clang\"" >> ../.cargo/config.toml
      - echo 'rustflags = ["-C", "link-arg=-lc++_shared"]' >> ../.cargo/config.toml
      - echo '[target.aarch64-linux-android]' >> ../.cargo/config.toml
      - echo "linker = \"$CLANG_PRE/aarch64-linux-android34-clang\"" >> ../.cargo/config.toml
      - echo 'rustflags = ["-C", "link-arg=-lc++_shared"]' >> ../.cargo/config.toml
      - printf 'sdk.dir=%s\nndk.dir=%s\n' "$$SDK$$" "$$NDK$$" > ../local.properties
    scanignore:
      - core/src/long_trip/data/region_adjacency.bin
      - core/src/routing/elevation/data/country_polys.bin
    build:
      - export ANDROID_NDK_HOME="$$NDK$$"
      - export ANDROID_HOME="$$SDK$$"
      - ../scripts/build-android-native.sh all release
      - ../scripts/build-plugin-wasm.sh
    ndk: r27c

  - versionName: 0.3.26-beta
    versionCode: 31
    commit: 7583d1bb2c6c046c99fc8959db594dccbadc6b26
    timeout: 14400
    subdir: app
    sudo:
      - apt-get update
      - apt-get install -y --no-install-recommends build-essential curl ca-certificates
        python3 pkg-config libssl-dev rustup
    gradle:
      - yes
    prebuild:
      - rustup toolchain install 1.98.0 --profile minimal
      - rustup default 1.98.0
      - rustup target add aarch64-linux-android x86_64-linux-android wasm32-unknown-unknown
      - CLANG_PRE="$$NDK$$/toolchains/llvm/prebuilt/linux-x86_64/bin"
      - mkdir -p ../.cargo
      - echo '[target.x86_64-linux-android]' > ../.cargo/config.toml
      - echo "linker = \"$CLANG_PRE/x86_64-linux-android34-clang\"" >> ../.cargo/config.toml
      - echo 'rustflags = ["-C", "link-arg=-lc++_shared"]' >> ../.cargo/config.toml
      - echo '[target.aarch64-linux-android]' >> ../.cargo/config.toml
      - echo "linker = \"$CLANG_PRE/aarch64-linux-android34-clang\"" >> ../.cargo/config.toml
      - echo 'rustflags = ["-C", "link-arg=-lc++_shared"]' >> ../.cargo/config.toml
      - printf 'sdk.dir=%s\nndk.dir=%s\n' "$$SDK$$" "$$NDK$$" > ../local.properties
    scanignore:
      - core/src/long_trip/data/region_adjacency.bin
      - core/src/routing/elevation/data/country_polys.bin
    build:
      - export ANDROID_NDK_HOME="$$NDK$$"
      - export ANDROID_HOME="$$SDK$$"
      - ../scripts/build-android-native.sh all release
      - ../scripts/build-plugin-wasm.sh
    ndk: r27c

  - versionName: 0.3.29-beta
    versionCode: 32
    commit: PLACEHOLDER_TAG_COMMIT
    timeout: 14400
    subdir: app
    sudo:
      - apt-get update
      - apt-get install -y --no-install-recommends build-essential curl ca-certificates
        python3 pkg-config libssl-dev rustup autoconf automake libtool
    gradle:
      - yes
    prebuild:
      - rustup toolchain install 1.98.0 --profile minimal
      - rustup default 1.98.0
      - rustup target add aarch64-linux-android x86_64-linux-android wasm32-unknown-unknown
      - rustc --version
      - NDK="$$NDK$$"
      - test -n "$NDK"
      - test -d "$NDK"
      - CLANG_PRE="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin"
      - mkdir -p ../.cargo
      - echo '[target.x86_64-linux-android]' > ../.cargo/config.toml
      - echo "linker = \"$CLANG_PRE/x86_64-linux-android34-clang\"" >> ../.cargo/config.toml
      - echo 'rustflags = ["-C", "link-arg=-lc++_shared"]' >> ../.cargo/config.toml
      - echo '[target.aarch64-linux-android]' >> ../.cargo/config.toml
      - echo "linker = \"$CLANG_PRE/aarch64-linux-android34-clang\"" >> ../.cargo/config.toml
      - echo 'rustflags = ["-C", "link-arg=-lc++_shared"]' >> ../.cargo/config.toml
      - printf 'sdk.dir=%s\nndk.dir=%s\n' "$$SDK$$" "$$NDK$$" > ../local.properties
      - export SOURCE_DATE_EPOCH=1700000000
    scanignore:
      - core/src/long_trip/data/region_adjacency.bin
      - core/src/routing/elevation/data/country_polys.bin
    build:
      - rustc --version
      - rustup target list --installed
      - export ANDROID_NDK_HOME="$$NDK$$"
      - export ANDROID_HOME="$$SDK$$"
      - export SOURCE_DATE_EPOCH=1700000000
      - test ! -e ../app/src/main/jniLibs/arm64-v8a/libnavi.so
      - test ! -e ../app/src/main/jniLibs/x86_64/libnavi.so
      - ../scripts/build-android-native.sh all release
      - test -f ../app/src/main/jniLibs/arm64-v8a/libnavi.so
      - test -f ../app/src/main/jniLibs/x86_64/libnavi.so
      - test -f ../app/src/main/jniLibs/arm64-v8a/libhamlib.so
      - test -f ../app/src/main/jniLibs/x86_64/libhamlib.so
      - ../scripts/build-plugin-wasm.sh
      - test -f ../app/src/main/assets/plugins/right_to_roam_camping/plugin.wasm
      - test -f ../app/src/main/assets/plugins/imu/plugin.wasm
      - test -f ../app/src/main/assets/plugins/cat/plugin.wasm
      - test -f ../app/src/main/res/font/liberation_sans.ttf
      - sha256sum ../app/src/main/jniLibs/arm64-v8a/libnavi.so ../app/src/main/jniLibs/x86_64/libnavi.so
      - sha256sum ../app/src/main/jniLibs/arm64-v8a/libhamlib.so ../app/src/main/jniLibs/x86_64/libhamlib.so
      - strings ../app/src/main/jniLibs/arm64-v8a/libnavi.so | grep -F 'VR-N76'
      - strings ../app/src/main/jniLibs/arm64-v8a/libnavi.so | grep -F 'native Benshi'
      - sha256sum ../app/src/main/assets/plugins/right_to_roam_camping/plugin.wasm
        ../app/src/main/assets/plugins/imu/plugin.wasm ../app/src/main/assets/plugins/cat/plugin.wasm
    ndk: r27c

MaintainerNotes: |
  Paste-ready next release recipe (keep tools/fdroid-check mirror identical to
  upstream master). VC 30/31 match fdroiddata. VC 32 adds Binaries +
  AllowedAPKSigningKeys. sudo includes Debian rustup plus autoconf/automake/libtool
  for Hamlib. prebuild installs Rust 1.98.0 (rust-toolchain.toml). SOURCE_DATE_EPOCH
  for zip determinism. build uses scripts/build-android-native.sh all release
  (Cargo profile release-android, panic=abort) then build-plugin-wasm.sh, with
  Hamlib/libnavi presence gates. UpdateCheckMode allows -beta tags.

AutoUpdateMode: Version
UpdateCheckMode: Tags ^v[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9]+)?$
UpdateCheckData: app/build.gradle.kts|versionCode\s=\s(\d+)|.|versionName\s=\s"(.*)"
CurrentVersion: 0.3.29-beta
CurrentVersionCode: 32
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
- **VR-N76 / Benshi is required** in release APKs: UniFFI
  `cat_plugin_connect_benshi_lab` keeps `navi-cat::benshi` (allowlist
  `0x0002`/`0x0103`) linked through release LTO; Kotlin
  `BenshiRfcommTransport` ships in DEX. CI greps `libnavi.so` for `VR-N76`
  and `native Benshi`.
- `CARGO_INCREMENTAL=0` and `[profile.release] incremental = false`.
- Gradle: `vcsInfo { include = false }`, `-Pnavi.unsignedRelease=true` for the
  F-Droid/CI unsigned APK, `android.enableResourceOptimizations=false`.
- Prefer `SOURCE_DATE_EPOCH` in CI (set in the release workflow).
- Dual-path checks must use a frozen source tree (no concurrent edits to
  Kotlin/Compose sources between path A and path B); otherwise `classes*.dex`
  can differ while `libnavi.so` still matches.
