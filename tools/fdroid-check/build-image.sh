#!/usr/bin/env bash
# Build the local Navi F-Droid check image from scratch (no layer cache).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TAG="${FDROID_IMAGE_TAG:-localhost/navi-fdroid-check:trixie}"
echo "Building $TAG (podman build --no-cache) ..."
podman build --no-cache \
  -t "$TAG" \
  -f "$ROOT/tools/fdroid-check/Dockerfile" \
  "$ROOT/tools/fdroid-check"
echo "Built $TAG"
podman image inspect "$TAG" --format '{{.Id}} {{.Created}}'
