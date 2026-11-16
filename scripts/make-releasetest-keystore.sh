#!/usr/bin/env bash
# Create a local releasetest keystore (not for Play / F-Droid; gitignored).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/app/keystore"
JKS="$DIR/navi-releasetest.jks"
mkdir -p "$DIR"
if [[ -f "$JKS" ]]; then
  echo "exists: $JKS"
  exit 0
fi
keytool -genkeypair \
  -keystore "$JKS" \
  -alias navi-releasetest \
  -keyalg RSA \
  -keysize 2048 \
  -validity 10000 \
  -storepass navi-releasetest-local \
  -keypass navi-releasetest-local \
  -dname "CN=Navi ReleaseTest, OU=Dev, O=Navi, L=Local, ST=NA, C=NO"
echo "wrote $JKS"
echo "Install with: ./gradlew :app:installReleasetest"
echo "applicationId: no.navi.app.releasetest"
