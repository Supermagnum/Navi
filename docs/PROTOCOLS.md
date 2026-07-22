# Wire protocols

Navi’s trusted core does not speak vehicle or radio buses directly. Hosts and
future plugins own the wire; the core consumes normalized snapshots (energy,
position, tracks).

## On-device IPC (implemented)

| Path | Role |
|---|---|
| UniFFI (`navi-ffi`) | Kotlin UI ↔ Rust core |
| WASM HostApi | `plugin-host` ↔ guest plugins — see [`plugins.md`](plugins.md) |

## External / vehicle / radio (documented; mostly not implemented)

| Document | Topic | Implementation status |
|---|---|---|
| [`ECU.md`](ECU.md) | OBD-II (ELM327), SAE J1939, MegaSquirt → `LiveEnergySnapshot` | Extension point only (`driver_break_core::ecu`) |
| [`APRS.md`](APRS.md) | APRS information fields; `TrackStore` range filtering | Protocol + display/range; RF ingest not shipped |
| [`APRS-SDR.md`](APRS-SDR.md) | APRS AFSK DSP stages; RTL-SDR IF offset; **`rtl-sdr-rs`** | Planned IQ front-end / DSP (not in-tree) |
| [`CAT.md`](CAT.md) | Amateur radio CAT; NFM repeater auto-tune → VFO 1 (≤150 km) | Specified; not implemented |

When a new transport lands, add framing, ports/baud, and examples here or in a
dedicated doc linked from this table.
