# Navi local F-Droid buildability image.
#
# Extends F-Droid's production-like buildserver image with everything the
# successful Podman check needed that is *not* present (or is broken) in the
# upstream container:
#   - build-essential (host `cc` for Rust build-scripts / proc-macros)
#   - rustup + pinned Rust 1.88.0 + Android ABIs used by Navi
#   - working gradlew-fdroid (upstream image only ships broken symlinks)
#   - Android SDK platform 36 + NDK r27c for compileSdk 36
#
# Build (from repo root, no cache for a clean bake):
#   podman build --no-cache -t localhost/navi-fdroid-check:trixie \
#     -f tools/fdroid-check/Dockerfile tools/fdroid-check
#
# Then run:
#   FDROID_IMAGE=localhost/navi-fdroid-check:trixie \
#     ./tools/fdroid-check/run-podman-build.sh
#
# Or use the helper:
#   ./tools/fdroid-check/build-image.sh && ./tools/fdroid-check/run-podman-build.sh

FROM registry.gitlab.com/fdroid/fdroidserver:buildserver-trixie

USER root
ENV DEBIAN_FRONTEND=noninteractive \
    ANDROID_HOME=/opt/android-sdk \
    RUSTUP_HOME=/opt/rustup \
    CARGO_HOME=/opt/cargo \
    PATH=/opt/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/opt/android-sdk/tools:/opt/android-sdk/platform-tools

# Host C toolchain: Rust crates compile build scripts / proc-macros for the
# host (x86_64) even when the final crate target is Android. Without `cc`,
# cargo fails with `linker 'cc' not found`.
RUN apt-get update -qq \
    && apt-get install -y --no-install-recommends \
        build-essential \
        curl \
        ca-certificates \
        pkg-config \
        libssl-dev \
        git \
    && rm -rf /var/lib/apt/lists/*

# Upstream image links /usr/local/bin/gradlew-fdroid -> /home/vagrant/gradlew-fdroid/...
# but does not ship the clone (that path is provisioned on real build VMs).
# F-Droid deletes the project's own gradle wrapper and invokes `gradlew-fdroid`
# (see fdroidserver common.default_config['gradle']).
RUN rm -rf /home/vagrant/gradlew-fdroid \
    && git clone --depth 1 https://gitlab.com/fdroid/gradlew-fdroid.git /home/vagrant/gradlew-fdroid \
    && chmod 0755 /home/vagrant/gradlew-fdroid/gradlew-fdroid \
    && ln -sfn /home/vagrant/gradlew-fdroid/gradlew-fdroid /usr/local/bin/gradlew-fdroid \
    && ln -sfn /home/vagrant/gradlew-fdroid/gradlew-fdroid /usr/local/bin/gradle

# Navi pins Rust 1.88.0 in rust-toolchain.toml. Install that channel plus the
# Android ABIs used by scripts/build-android-native.sh (arm64 + x86_64).
RUN curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs \
        | bash -s -- -y --default-toolchain 1.88.0 --profile minimal --no-modify-path \
    && rustup component add rustfmt clippy \
    && rustup target add aarch64-linux-android x86_64-linux-android \
    && ln -sfn /opt/cargo /root/.cargo \
    && ln -sfn /opt/rustup /root/.rustup \
    && rustc --version \
    && rustup target list --installed

# compileSdk / targetSdk 36 + NDK r27c (metadata `ndk: r27c`).
RUN yes | sdkmanager \
        "platforms;android-36" \
        "build-tools;35.0.0" \
        "ndk;27.2.12479018" \
    || true

# Sanity: tools the build path must see without further setup.
RUN command -v cc \
    && command -v gradlew-fdroid \
    && test -x /home/vagrant/gradlew-fdroid/gradlew-fdroid \
    && test -d /opt/android-sdk/platforms/android-36 \
    && test -d /opt/android-sdk/ndk/27.2.12479018 \
    && test -x /opt/cargo/bin/rustc \
    && /opt/cargo/bin/rustc --version

# Login shells (`bash -lc`) reset PATH from /etc/profile; keep cargo/rustup visible.
RUN printf '%s\n' \
      'export RUSTUP_HOME=/opt/rustup' \
      'export CARGO_HOME=/opt/cargo' \
      'export PATH="/opt/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:${PATH:-}"' \
      > /etc/profile.d/navi-rust.sh \
    && chmod 644 /etc/profile.d/navi-rust.sh
