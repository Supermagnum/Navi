#!/usr/bin/env bash
# F-Droid buildability check for Navi via Podman (rootless).
#
# Prefer the baked local image (see Dockerfile + build-image.sh). Falls back to
# the upstream buildserver image only if FDROID_IMAGE is set explicitly to it.
#
# Quoting note: an earlier revision embedded the container body in
#   podman ... -lc '... curl --proto '=https' ...'
# The nested single quote after --proto terminated the outer -lc string early,
# which mangled later sed brace expressions (`unmatched '{'`). The container
# body is therefore always written via a quoted heredoc to a file and executed
# as a script path — never as an inline -lc string.
#
# Usage (from repo root or any cwd):
#   ./tools/fdroid-check/build-image.sh   # once / after Dockerfile changes
#   ./tools/fdroid-check/run-podman-build.sh
#
# Logs land in tools/fdroid-check/logs/ on the host (mounted volume).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECK="$ROOT/tools/fdroid-check"
LOG_DIR="$CHECK/logs"
WORK="$CHECK/work"
IMAGE="${FDROID_IMAGE:-localhost/navi-fdroid-check:trixie}"
FDROIDSERVER_SRC="${FDROIDSERVER_SRC:-$CHECK/fdroidserver}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
LOG="$LOG_DIR/fdroid-build-$STAMP.log"

mkdir -p "$LOG_DIR" "$WORK/metadata" "$WORK/tmp" "$WORK/unsigned" "$WORK/repo" "$WORK/srclibs" "$WORK/build"

if ! podman image exists "$IMAGE" 2>/dev/null; then
  echo "Image $IMAGE not found. Building it now (this takes a while)..."
  "$CHECK/build-image.sh"
fi

if [[ ! -d "$FDROIDSERVER_SRC/.git" ]]; then
  echo "Cloning fdroidserver into $FDROIDSERVER_SRC ..."
  git clone --depth=1 https://gitlab.com/fdroid/fdroidserver.git "$FDROIDSERVER_SRC"
fi

cp -f "$CHECK/metadata/no.navi.app.yml" "$WORK/metadata/no.navi.app.yml"

# Minimal fdroid config (local unsigned builds only).
cat > "$WORK/config.yml" <<'EOF'
repo_url: https://example.invalid/fdroid/repo
repo_name: Navi F-Droid check
repo_description: Local buildability check (not a public repo)
archive_older: 0
gradle: /usr/local/bin/gradlew-fdroid
EOF

echo "=== Navi F-Droid Podman build check ===" | tee "$LOG"
echo "image=$IMAGE" | tee -a "$LOG"
echo "root=$ROOT" | tee -a "$LOG"
echo "log=$LOG" | tee -a "$LOG"

# Non-login probe (login shells can drop ENV PATH); profile.d covers bash -lc.
podman run --rm --entrypoint /bin/bash "$IMAGE" -c \
  'export PATH="/opt/cargo/bin:$PATH"; command -v cc; command -v gradlew-fdroid; command -v rustc; rustc --version; rustup target list --installed' \
  2>&1 | tee -a "$LOG"
touch "$WORK/.write_probe" && rm -f "$WORK/.write_probe"

# Container body as a file (see quoting note in the header).
INNER="$WORK/container-build.sh"
cat > "$INNER" <<'INNER_EOF'
#!/bin/bash
set -euo pipefail
export LOG=/build/fdroiddata/logs/container-inner.log
mkdir -p /build/fdroiddata/logs /build/fdroiddata/tmp
# Prefer image-baked toolchains; allow volume overrides for cargo registry cache.
export TMPDIR=/build/fdroiddata/tmp
export CARGO_HOME="${CARGO_HOME:-/opt/cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-/opt/rustup}"
export PATH="${CARGO_HOME}/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:${PATH:-}"
exec > >(tee -a "$LOG") 2>&1

. /etc/profile.d/bsenv.sh 2>/dev/null || true
test -n "${ANDROID_HOME:-}" || export ANDROID_HOME=/opt/android-sdk
export fdroidserver=/home/vagrant/fdroidserver
export PATH="$fdroidserver:$PATH"
export PYTHONPATH="$fdroidserver:${PYTHONPATH:-}"

echo "whoami=$(whoami) ANDROID_HOME=$ANDROID_HOME TMPDIR=$TMPDIR"
echo "java=$(java -version 2>&1 | head -1)"
echo "fdroid=$(command -v fdroid)"
echo "cc=$(command -v cc)"
echo "gradlew-fdroid=$(command -v gradlew-fdroid)"
echo "rustc=$(rustc --version)"
fdroid --version || true

# Defensive: image bake should already provide these. Re-run only if missing so
# a fresh upstream image still works when FDROID_IMAGE points at it.
if ! command -v cc >/dev/null 2>&1; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -qq
  apt-get install -y --no-install-recommends build-essential >/tmp/apt-cc.log
fi
if [[ ! -x /home/vagrant/gradlew-fdroid/gradlew-fdroid ]]; then
  rm -rf /home/vagrant/gradlew-fdroid
  git clone --depth 1 https://gitlab.com/fdroid/gradlew-fdroid.git /home/vagrant/gradlew-fdroid
  chmod 0755 /home/vagrant/gradlew-fdroid/gradlew-fdroid
  ln -sfn /home/vagrant/gradlew-fdroid/gradlew-fdroid /usr/local/bin/gradlew-fdroid
  ln -sfn /home/vagrant/gradlew-fdroid/gradlew-fdroid /usr/local/bin/gradle
fi
if ! command -v rustc >/dev/null 2>&1; then
  curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs \
    | bash -s -- -y --default-toolchain 1.88.0 --profile minimal --no-modify-path
fi
# shellcheck disable=SC1091
. "${CARGO_HOME}/env" 2>/dev/null || true
rustup target add aarch64-linux-android x86_64-linux-android >/dev/null
ln -sfn "$CARGO_HOME" /root/.cargo
ln -sfn "$RUSTUP_HOME" /root/.rustup

# Snapshot the mounted working tree (includes uncommitted changes).
SNAP=/build/fdroiddata/navi-snap
rm -rf "$SNAP"
mkdir -p "$SNAP"
rsync -a \
  --exclude target/ \
  --exclude .gradle/ \
  --exclude app/build/ \
  --exclude tools/fdroid-check/work/ \
  --exclude tools/fdroid-check/logs/ \
  --exclude tools/fdroid-check/fdroidserver/ \
  --exclude app/keystore/ \
  --exclude "*.apk" \
  --exclude "*.aab" \
  /build/navi/ "$SNAP/"
cd "$SNAP"
rm -rf .git
git init -q
git config user.email "fdroid-check@localhost"
git config user.name "fdroid-check"
git add -A
git commit -qm "fdroid-check snapshot"
SNAP_SHA=$(git rev-parse HEAD)
echo "snap_sha=$SNAP_SHA"

# Ensure Android targets exist for the repo-pinned toolchain.
cd "$SNAP"
rustup show
rustup target add aarch64-linux-android x86_64-linux-android
cd /

rm -rf /build/fdroiddata/build/no.navi.app
mkdir -p /build/fdroiddata/build
git clone --local "$SNAP" /build/fdroiddata/build/no.navi.app
cd /build/fdroiddata/build/no.navi.app
git checkout -q "$SNAP_SHA"

if grep -q '^Categories:' /build/fdroiddata/metadata/no.navi.app.yml; then
  awk '
    BEGIN {skip=0}
    /^Categories:/ {skip=1; next}
    /^[A-Za-z]/ {skip=0}
    skip && /^  -/ {next}
    skip {next}
    {print}
  ' /build/fdroiddata/metadata/no.navi.app.yml > /tmp/no.navi.app.yml.tmp
  mv /tmp/no.navi.app.yml.tmp /build/fdroiddata/metadata/no.navi.app.yml
fi

sed -i "s|^Repo: .*|Repo: file://$SNAP|" /build/fdroiddata/metadata/no.navi.app.yml
sed -i "s|PLACEHOLDER_COMMIT|$SNAP_SHA|" /build/fdroiddata/metadata/no.navi.app.yml

cd /build/fdroiddata
fdroid readmeta
fdroid rewritemeta no.navi.app || true
fdroid lint -W ignore no.navi.app || true

echo "=== fdroid build no.navi.app:1 ==="
fdroid build -v -t --skip-scan --no-tarball -W ignore -s no.navi.app:1
echo "=== fdroid build finished OK ==="
ls -la unsigned/ tmp/ 2>/dev/null || true
find tmp unsigned -name "*.apk" 2>/dev/null || true
INNER_EOF
chmod +x "$INNER"

# Use image-baked cargo/rustup by default (not the previous session's volume).
podman run --rm \
  --name "navi-fdroid-check-$STAMP" \
  --entrypoint /bin/bash \
  -v "$ROOT:/build/navi:Z" \
  -v "$WORK:/build/fdroiddata:Z" \
  -v "$FDROIDSERVER_SRC:/home/vagrant/fdroidserver:Z" \
  -e ANDROID_HOME=/opt/android-sdk \
  -e TMPDIR=/build/fdroiddata/tmp \
  -e CARGO_HOME=/opt/cargo \
  -e RUSTUP_HOME=/opt/rustup \
  "$IMAGE" \
  /build/fdroiddata/container-build.sh 2>&1 | tee -a "$LOG"

echo "Host log: $LOG"
find "$WORK" -name "*.apk" 2>/dev/null | head
ls -la "$WORK/unsigned" "$WORK/tmp" 2>/dev/null || true
