#!/usr/bin/env bash
# Fail when product code changes without bumping app versionCode / versionName.
#
# Product paths exclude androidTest (including the Bevensen campaign runner),
# docs, compiled APKs, and this gate script itself when only docs change.
#
# Usage:
#   scripts/check-version-bump.sh [base-ref]
# Default base-ref: origin/main if present, else main, else HEAD~1.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

resolve_base() {
  if [[ -n "${1:-}" ]]; then
    echo "$1"
    return
  fi
  if [[ -n "${VERSION_BUMP_BASE:-}" ]]; then
    echo "$VERSION_BUMP_BASE"
    return
  fi
  if git rev-parse --verify -q origin/main >/dev/null; then
    echo "origin/main"
    return
  fi
  if git rev-parse --verify -q main >/dev/null; then
    echo "main"
    return
  fi
  echo "HEAD~1"
}

BASE="$(resolve_base "${1:-}")"
if ! git rev-parse --verify -q "$BASE" >/dev/null; then
  echo "error: base ref not found: $BASE" >&2
  exit 2
fi

MERGE_BASE="$(git merge-base HEAD "$BASE")"
echo "==> version bump gate (base=$BASE merge-base=$MERGE_BASE)"

# Paths that require a version bump when touched.
PRODUCT_REGEX='^(app/src/main/|app/build\.gradle\.kts|navi-ffi/|core/|plugin-host/|plugin-sdk/|plugins/|right-to-roam-camping/|driver-break-core/)'

CHANGED="$(git diff --name-only "$MERGE_BASE"...HEAD || true)"
if [[ -z "$CHANGED" ]]; then
  echo "ok: no commits vs $BASE (nothing to check)"
  exit 0
fi

PRODUCT_HITS=""
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" =~ $PRODUCT_REGEX ]]; then
    # androidTest is under app/src/androidTest — not matched by app/src/main/
    PRODUCT_HITS+="$f"$'\n'
  fi
done <<< "$CHANGED"

if [[ -z "$PRODUCT_HITS" ]]; then
  echo "ok: no product-code paths changed vs $BASE (docs/tests/APKs-only allowed without bump)"
  exit 0
fi

echo "product paths changed:"
echo "$PRODUCT_HITS" | sed '/^$/d' | sed 's/^/  /'

BASE_CODE="$(git show "$MERGE_BASE:app/build.gradle.kts" | grep -E 'versionCode[[:space:]]*=' | head -1 | sed -E 's/.*versionCode[[:space:]]*=[[:space:]]*([0-9]+).*/\1/')"
BASE_NAME="$(git show "$MERGE_BASE:app/build.gradle.kts" | grep -E 'versionName[[:space:]]*=' | head -1 | sed -E 's/.*versionName[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/')"
HEAD_CODE="$(grep -E 'versionCode[[:space:]]*=' app/build.gradle.kts | head -1 | sed -E 's/.*versionCode[[:space:]]*=[[:space:]]*([0-9]+).*/\1/')"
HEAD_NAME="$(grep -E 'versionName[[:space:]]*=' app/build.gradle.kts | head -1 | sed -E 's/.*versionName[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/')"

echo "base versionCode=$BASE_CODE versionName=$BASE_NAME"
echo "head versionCode=$HEAD_CODE versionName=$HEAD_NAME"

if [[ -z "$BASE_CODE" || -z "$HEAD_CODE" ]]; then
  echo "FAIL: could not parse versionCode from app/build.gradle.kts" >&2
  exit 1
fi

if ! [[ "$HEAD_CODE" =~ ^[0-9]+$ && "$BASE_CODE" =~ ^[0-9]+$ ]]; then
  echo "FAIL: versionCode not numeric (base=$BASE_CODE head=$HEAD_CODE)" >&2
  exit 1
fi

if (( HEAD_CODE <= BASE_CODE )); then
  echo "FAIL: product code changed but versionCode was not bumped ($HEAD_CODE <= $BASE_CODE)" >&2
  echo "Bump versionCode and versionName in app/build.gradle.kts when shipping code changes." >&2
  exit 1
fi

if [[ "$HEAD_NAME" == "$BASE_NAME" ]]; then
  echo "FAIL: product code changed but versionName was not bumped ('$HEAD_NAME')" >&2
  exit 1
fi

echo "ok: version bumped ($BASE_CODE/$BASE_NAME -> $HEAD_CODE/$HEAD_NAME)"
