#!/usr/bin/env bash
# Serve corridor fixtures to the Android emulator (10.0.2.2 -> host).
# Usage: ./scripts/serve-android-fixtures.sh [port] [fixture-dir]
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${1:-8765}"
DIR="${2:-$ROOT/core/target/integration-fixtures}"
cd "$DIR"
echo "Serving $DIR on 0.0.0.0:$PORT (emulator URL http://10.0.2.2:$PORT/...)"
exec python3 -m http.server "$PORT" --bind 0.0.0.0
