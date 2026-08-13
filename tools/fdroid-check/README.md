# F-Droid buildability check (Podman)

Local reproduction of F-Droid's production-like `fdroid build` against this
working tree. Not a submission to [fdroiddata](https://gitlab.com/fdroid/fdroiddata);
metadata here is a check harness only.

## Prerequisites

- Rootless `podman`
- Network (image build pulls rustup, SDK/NDK, crates; the check run downloads crates)
- Host free disk for the image + NDK (~several GB). Prefer storing Podman
  `graphroot` on a large disk (see `~/.config/containers/storage.conf`).

## Quick start (reproducible)

```bash
# From repository root — bake a clean local image (no layer cache):
./tools/fdroid-check/build-image.sh

# Run the full fdroid build check (zero manual setup inside the container):
./tools/fdroid-check/run-podman-build.sh
```

Equivalent expanded form:

```bash
podman build --no-cache \
  -t localhost/navi-fdroid-check:trixie \
  -f tools/fdroid-check/Dockerfile \
  tools/fdroid-check

FDROID_IMAGE=localhost/navi-fdroid-check:trixie \
  ./tools/fdroid-check/run-podman-build.sh
```

Success looks like:

```text
INFO: Successfully built no.navi.app:1
INFO: 1 build succeeded
=== fdroid build finished OK ===
```

APK output (unsigned): `tools/fdroid-check/work/tmp/no.navi.app_1.apk`

`work/`, `logs/`, and `fdroidserver/` are gitignored.

## What the baked image adds

Base: `registry.gitlab.com/fdroid/fdroidserver:buildserver-trixie`

| Bake step | Why |
|---|---|
| `build-essential` | Host `cc` for Rust build-scripts / proc-macros |
| rustup + **Rust 1.88.0** + `aarch64-linux-android` / `x86_64-linux-android` | Matches `rust-toolchain.toml` + `scripts/build-android-native.sh` |
| Clone of [`gradlew-fdroid`](https://gitlab.com/fdroid/gradlew-fdroid) | Upstream image only has broken symlinks under `/usr/local/bin`; F-Droid deletes the project Gradle wrapper and calls `gradlew-fdroid` |
| `platforms;android-36`, `build-tools;35.0.0`, `ndk;27.2.12479018` | compileSdk 36 / metadata `ndk: r27c` |

## What `run-podman-build.sh` does

1. Ensures `localhost/navi-fdroid-check:trixie` exists (builds it if missing)
2. Shallow-clones `fdroid/fdroidserver` if needed and mounts it at
   `/home/vagrant/fdroidserver` (F-Droid Quick Start pattern)
3. Writes `work/container-build.sh` via a **quoted heredoc** (never inline
   `podman … -lc '…'` — nested `'=https'` previously broke quoting and mangled
   later `sed` brace expressions)
4. Snapshots the current working tree into a local git repo and pre-seeds
   `build/no.navi.app`
5. Runs `fdroid build -t --skip-scan --no-tarball -W ignore no.navi.app:1`
6. Writes full logs under `tools/fdroid-check/logs/`

## Recipe notes (metadata)

- Rust via **rustup** in the build `sudo:` block (production buildserver path).
  The local Podman image pre-bakes the same tools because `sudo:` is skipped
  when not on a dedicated F-Droid build VM.
- `.cargo/config.toml` linker paths are rewritten from `$$NDK$$` in `prebuild`.
- Native `libnavi.so` is built from source for `aarch64` and `x86_64` before
  `gradle assembleRelease`. Committed `jniLibs/*/libnavi.so` prebuilts (used by
  GitHub `android-build` which does not cross-compile Rust) are removed in
  `prebuild:` so the F-Droid APK cannot pick them up; the build step asserts they
  are gone before compile and present after `build-android-native.sh`.
- `dependenciesInfo.includeInApk` / `includeInBundle` are `false` in
  `app/build.gradle.kts`.
- `io.opencensus` appears only under AGP Unified Test Platform host configs
  (instrumented-test tooling). It is not a release/runtime dependency.

## Override image

```bash
# Force upstream image (slow; runtime will reinstall missing pieces defensively):
FDROID_IMAGE=registry.gitlab.com/fdroid/fdroidserver:buildserver-trixie \
  ./tools/fdroid-check/run-podman-build.sh
```
