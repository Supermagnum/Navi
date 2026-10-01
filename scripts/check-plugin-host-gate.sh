#!/usr/bin/env bash
# Gate guards for navi-plugin-host before it may be linked into shipped binaries.
#
# 1) Premature-link guard: navi-ffi / navi-desktop / navi-linux must not depend
#    on navi-plugin-host until docs/plugins.md gate conditions are cleared.
# 2) wasmtime feature guard: plugin-host must only enable cranelift+runtime+gc-drc
#    (no wasi / component-model / winch / default feature set).
#
# Uses POSIX grep (not ripgrep) so GitHub-hosted runners without rg still pass.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail=0

echo "==> premature-link guard (Cargo.toml must not depend on navi-plugin-host)"
for crate in navi-ffi navi-desktop navi-linux; do
  toml="$crate/Cargo.toml"
  if [[ ! -f "$toml" ]]; then
    echo "error: missing $toml" >&2
    fail=1
    continue
  fi
  # Match dependency keys only (not comments).
  if grep -E -q '^[[:space:]]*navi-plugin-host[[:space:]]*=' "$toml"; then
    echo "FAIL: $toml lists navi-plugin-host — gate in docs/plugins.md is not cleared" >&2
    fail=1
  else
    echo "ok: $toml has no navi-plugin-host dependency"
  fi
done

# Also catch accidental path deps with a different key name.
if grep -E -q 'path[[:space:]]*=[[:space:]]*"[^"]*plugin-host"' \
  navi-ffi/Cargo.toml navi-desktop/Cargo.toml navi-linux/Cargo.toml; then
  echo "FAIL: path dependency on plugin-host found in shipped crate Cargo.toml" >&2
  fail=1
fi

echo "==> wasmtime feature guard (plugin-host pin)"
FEATURES="$(cargo tree -p navi-plugin-host -e features -i wasmtime 2>/dev/null || true)"
if [[ -z "$FEATURES" ]]; then
  echo "error: cargo tree returned no wasmtime edges for navi-plugin-host" >&2
  fail=1
else
  echo "$FEATURES"
  # Must mention the allowed features; must not mention wasi / component-model / winch
  # as enabled feature names on the wasmtime package line.
  if ! echo "$FEATURES" | grep -q 'wasmtime'; then
    echo "FAIL: wasmtime missing from feature tree" >&2
    fail=1
  fi
  # cargo tree -e features prints feature names; reject dangerous ones if present
  # as enabled features of wasmtime itself (not just in the crate name path).
  while IFS= read -r line; do
    case "$line" in
      *wasmtime*)
        lower="$(echo "$line" | tr '[:upper:]' '[:lower:]')"
        for bad in wasi component-model winch pooling-allocator; do
          # Match feature token boundaries roughly: ,feature or (feature or feature,
          if echo "$lower" | grep -E -q "(^|[,( ])${bad}([,)]|$)"; then
            echo "FAIL: wasmtime feature tree enables '$bad': $line" >&2
            fail=1
          fi
        done
        ;;
    esac
  done <<< "$FEATURES"
fi

# Confirm Cargo.toml still pins default-features = false with the expected set.
if ! grep -F -q 'wasmtime = { version = "48", default-features = false, features = ["cranelift", "runtime", "gc-drc"] }' \
  plugin-host/Cargo.toml; then
  echo "FAIL: plugin-host/Cargo.toml wasmtime pin/features drifted" >&2
  grep -n 'wasmtime' plugin-host/Cargo.toml || true
  fail=1
else
  echo "ok: plugin-host wasmtime pin is cranelift+runtime+gc-drc, default-features=false"
fi

# No other workspace crate should pull wasmtime independently (feature unification risk).
echo "==> workspace wasmtime uniqueness"
OTHER=""
while IFS= read -r toml; do
  case "$toml" in
    ./plugin-host/Cargo.toml|plugin-host/Cargo.toml) continue ;;
  esac
  hits="$(grep -n '^[^#]*wasmtime' "$toml" || true)"
  [[ -z "$hits" ]] && continue
  while IFS= read -r hit; do
    body="${hit#*:}"
    if [[ "$body" =~ ^[[:space:]]*# ]]; then
      continue
    fi
    OTHER+="${toml}:${hit}"$'\n'
  done <<< "$hits"
done < <(find . -name Cargo.toml ! -path './target/*' ! -path '*/target/*')

if [[ -n "$OTHER" ]]; then
  echo "FAIL: wasmtime referenced outside plugin-host:" >&2
  printf '%s' "$OTHER" >&2
  fail=1
else
  echo "ok: only plugin-host declares wasmtime"
fi

if [[ "$fail" -ne 0 ]]; then
  exit 1
fi
echo "OK: plugin-host gate guards passed"
