# Plugin system

Navi runs untrusted extension code in a **wasmtime** sandbox (`plugin-host`), not
in the routing / sensor / UI process address space as native code.

Guest plugins compile to `wasm32-unknown-unknown` and call a narrow HostApi via
WASM imports. **WASI filesystem and network are not linked** — only the
capabilities declared in the plugin manifest are wired.

## Crates

| Crate | Role |
|---|---|
| `plugin-host` | Load manifest + `.wasm`, capability gate, fuel + epoch timeout, HostApi |
| `plugin-sdk` | `no_std` guest helpers (`host_log`, `host_position`, …) |
| `plugins/log-hello` | Reference plugin: one log line |
| `plugins/busy-loop` | Reference plugin: infinite loop (isolation tests) |

## Manifest (`plugin.json`)

```json
{
  "name": "log_hello",
  "version": "0.1.0",
  "entry": "plugin_main",
  "capabilities": ["log"],
  "fuel_limit": 5000000,
  "timeout_ms": 500,
  "wasm": "plugin.wasm"
}
```

Capabilities are validated **before** the module is instantiated. Unknown
capability names reject the load. Requested capabilities must be a subset of the
host policy set passed to `PluginHost::load_dir`.

## Capabilities (HostApi)

| Capability | Import | Purpose |
|---|---|---|
| `log` | `navi.log(ptr, len)` | UTF-8 log line to the host |
| `position_read` | `navi.get_position(out_ptr) -> i32` | Write `lat,lon` as two little-endian `f64` |
| `poi_query` | `navi.poi_query(...)` | JSON POI list into guest buffer |
| `poi_write` | `navi.poi_write(ptr, len)` | Upsert POI from guest JSON |

## Isolation limits

- **Fuel**: instruction budget (`DEFAULT_FUEL` = 5_000_000 unless overridden).
- **Wall-clock**: epoch interruption (`DEFAULT_TIMEOUT_MS` = 250 unless overridden).

A plugin that busy-loops is terminated with `CallOutcome::FuelExhausted` or
`CallOutcome::Timeout`. Isolation is covered by
`plugin-host/tests/isolation.rs` (load→call→return for `log-hello`; kill +
host-thread heartbeat for `busy-loop`).

## Building a reference plugin

```bash
cargo build --release --target wasm32-unknown-unknown \
  --manifest-path plugins/log-hello/Cargo.toml
```

Copy the produced `.wasm` next to `plugin.json` as `plugin.wasm`, then:

```bash
cargo test -p navi-plugin-host --test isolation -- --nocapture
```
