# Wire protocols (placeholder)

Navi does not yet ship a stable external wire protocol. On-device IPC today is:

- UniFFI (`navi-ffi`) between Kotlin UI and the Rust core
- WASM HostApi imports between `plugin-host` and guest plugins (see
  [`plugins.md`](plugins.md))

This document exists so the README document index stays complete. When APRS,
CAT, or other radio/telemetry transports land, describe framing and ports here.
