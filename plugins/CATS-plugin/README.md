# CATS plugin (Computer Aided Transceiver)

WASM guest for Navi CAT. All auto-tune / network-follow / UI-state logic for
amateur radio repeater selection lives in this directory and runs inside
wasmtime. Radio I/O is host-only via HostApi:

| Cap | Role |
|---|---|
| `position_read` | Vehicle position |
| `cat_status` | Connection / gating / PTT |
| `repeater_query` | Sites within ≤ 150 km |
| `cat_vfo_set` | Program VFO 1 (host verifies read-back) |
| `cat_network_follow` | Enable / pin / disable LA5MR-style follow |
| `log` | Diagnostics |

See [`docs/CAT.md`](../../docs/CAT.md). Never transmits; never talks to the
radio bytes directly.

Host backends (Hamlib TCP / native **Benshi** GAIA) live in `navi-cat`, not
here. The guest only asks for VFO 1 programs and follow state; the host enforces
allowlists, gating, VFO 2 protection, and never-transmit.
