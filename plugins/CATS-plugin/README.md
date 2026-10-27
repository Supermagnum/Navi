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

## Licence note

**You may listen, but do not transmit without an amateur radio licence.**

## What is tested

| Layer | Covered here / related |
|---|---|
| CI / simulator | Guest select / follow / decision C / demo plans; host FakeBenshiPeer + Hamlib dummy via `navi-cat` |
| Android emulator | Injected RFCOMM only (`BenshiCorridorEmulatorKTest`); no real BT |
| Real hardware | VR-N76 host BlueZ 2026-10-12: reachability + 5 s demos PASS (see `docs/CAT-test-report.md`) |
| Not tested | Hamlib on real radios; tablet↔physical radio; UV-Pro / GA-5WB; a real drive with a user-copied RepeaterBook KML |

RepeaterBook is not fetched. The host reads KML/KMZ/CSV from the drop folder and passes parsed records to this guest. The guest never receives a path. Network choice `auto` follows the nearest programmable network; a named network with no members in the current area falls back to `auto` and the host shows a notice. Switching uses the live position (and dead reckoning when GNSS is lost). A planned route is not required. A drive with a physical radio and a user-copied KML is still a manual check: find `Android/data/no.navi.app/files/repeaters` over MTP, press Update, select the network, and confirm VFO A follows it with no route set.

**Status:** simulator-verified; VR-N76 host BlueZ hardware Steps 5–6 PASS (2026-10-12).
