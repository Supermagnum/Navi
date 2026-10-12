# CATS plugin (Computer Aided Transceiver)

WASM guest for Navi CAT. Selection, network follow (hysteresis / dwell / pin),
leave-network **decision C**, and developer **demo-mode sequencing** live in
this directory and run inside wasmtime. Radio I/O is host-only via HostApi:

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

## Guest vs host

| In this crate (WASM guest) | In `navi-cat` (host) |
|---|---|
| NFM selection (`select`) | Hamlib TCP / native Benshi GAIA I/O |
| Follow hysteresis, dwell, pin | Never-transmit allowlist |
| Leave-network decision C | Interlocks (PTT / squelch) |
| Demo-mode step plan (5 s cadence) | Read-back verification, VFO 2 fingerprint |
| | SPLAT talk-coverage **test scorer** only |

Corridor replays H/I, coverage score J, and FakeBenshiPeer demo runs are
integration tests under `tests/` (crate-type includes `rlib` for that).

Host backends (Hamlib TCP / native **Benshi** GAIA) live in `navi-cat`, not
here. Android RFCOMM (`BenshiRfcommTransport`) injects a socket in tests — no
real Bluetooth in CI.

**Status:** simulator-verified, hardware pending.
