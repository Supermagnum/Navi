#!/usr/bin/env bash
# Create a local upload keystore for AAB smoke tests (not for Play production).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/app/keystore"
JKS="$DIR/navi-upload.jks"
mkdir -p "$DIR"
if [[ -f "$JKS" ]]; then
  echo "exists: $JKS"
  exit 0
fi
keytool -genkeypair \
  -keystore "$JKS" \
  -alias navi-upload \
  -keyalg RSA \
  -keysize 2048 \
  -validity 10000 \
  -storepass navi-upload-local \
  -keypass navi-upload-local \
  -dname "CN=Navi Local Upload, OU=Dev, O=Navi, L=Local, ST=NA, C=NO"
echo "wrote $JKS"
