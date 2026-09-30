#!/usr/bin/env bash
# Gate guards for navi-plugin-host after the Android product link.
#
# 1) Premature-link guard: navi-desktop / navi-linux must not depend on
#    navi-plugin-host until those hosts also wire the camping (or other) product
#    plugin. navi-ffi (Android libnavi.so) is allowed — Phase 5a lifted that gate.
# 2) wasmtime feature guard: plugin-host must only enable cranelift+runtime
#    (no wasi / component-model / winch / gc / default feature set).
#
# Uses POSIX grep (not ripgrep) so GitHub-hosted runners without rg still pass.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail=0

echo "==> premature-link guard (desktop/linux must not depend on navi-plugin-host yet)"
for crate in navi-desktop navi-linux; do
  toml="$crate/Cargo.toml"
  if [[ ! -f "$toml" ]]; then
    echo "error: missing $toml" >&2
    fail=1
    continue
  fi
  if grep -E -q '^[[:space:]]*navi-plugin-host[[:space:]]*=' "$toml"; then
    echo "FAIL: $toml lists navi-plugin-host — desktop/linux product link not cleared" >&2
    fail=1
  else
    echo "ok: $toml has no navi-plugin-host dependency"
  fi
done

if grep -E -q 'path[[:space:]]*=[[:space:]]*"[^"]*plugin-host"' \
  navi-desktop/Cargo.toml navi-linux/Cargo.toml; then
  echo "FAIL: path dependency on plugin-host found in desktop/linux Cargo.toml" >&2
  fail=1
fi

if ! grep -E -q '^[[:space:]]*navi-plugin-host[[:space:]]*=' navi-ffi/Cargo.toml; then
  echo "FAIL: navi-ffi must depend on navi-plugin-host after Phase 5a gate lift" >&2
  fail=1
else
  echo "ok: navi-ffi depends on navi-plugin-host (Android gate lifted)"
fi

echo "==> wasmtime feature guard (plugin-host pin)"
FEATURES="$(cargo tree -p navi-plugin-host -e features -i wasmtime 2>/dev/null || true)"
if [[ -z "$FEATURES" ]]; then
  echo "error: cargo tree returned no wasmtime edges for navi-plugin-host" >&2
  fail=1
else
  echo "$FEATURES"
  if ! echo "$FEATURES" | grep -q 'wasmtime'; then
    echo "FAIL: wasmtime missing from feature tree" >&2
    fail=1
  fi
  while IFS= read -r line; do
    case "$line" in
      *wasmtime*)
        lower="$(echo "$line" | tr '[:upper:]' '[:lower:]')"
        for bad in wasi component-model winch pooling-allocator wat cache profiling coredump demangle addr2line gc-drc gc-copying; do
          if echo "$lower" | grep -E -q "(^|[,( ])${bad}([,)]|$)"; then
            echo "FAIL: wasmtime feature tree enables '$bad': $line" >&2
            fail=1
          fi
        done
        ;;
    esac
  done <<< "$FEATURES"
fi

if ! grep -F -q 'wasmtime = { version = "48", default-features = false, features = ["cranelift", "runtime"] }' \
  plugin-host/Cargo.toml; then
  echo "FAIL: plugin-host/Cargo.toml wasmtime pin/features drifted" >&2
  grep -n 'wasmtime' plugin-host/Cargo.toml || true
  fail=1
else
  echo "ok: plugin-host wasmtime pin is cranelift+runtime, default-features=false"
fi

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
echo "OK: plugin-host gate guards passed (Android link allowed)"
