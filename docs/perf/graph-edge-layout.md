# GraphEdge layout (496 bytes per edge)

Status: note for a later project. No change planned now; the emulator planning
peak bound is 1400 MB (follow-up 29).

## Where the bytes go

`GraphEdge` (`core/src/routing/graph/builder.rs`) is 496 bytes inline on
x86_64, before any heap data:

| Fields | Count | Bytes |
| --- | ---: | ---: |
| `String` / `Option<String>` (id, highway, name, road_ref, maxspeed_type, three conditionals) | 8 x 24 | 192 |
| `Option<f64>` (eco weight, maxspeed, practical, advisory, minspeed, weight/axle/bogie, height/width/length, ferry interval) | 12 x 16 | 192 |
| `f64` (length, base weight, cost multiplier, start/end lat/lon) | 7 x 8 | 56 |
| `shape: Vec<(f64, f64)>` | 1 x 24 | 24 |
| `source`, `target` (`i64`) | 2 x 8 | 16 |
| bools, `lanes`, `SurfaceQuality`, padding | | 16 |

Heap strings and shapes come on top. In the corridor stage (follow-up 28),
534k merged edges took 252 MB inline. The heaviest detailed hop on the
emulator (about 377k nodes) peaks at about 1.3 GB against about 350 MB idle.

## What compacting would involve

- Hot/cold split: keep only what A* and snap read per expansion (source,
  target, length, base weight, cost multiplier, highway class, flags, speeds)
  in a dense array; move rare tags (dimension limits, conditionals, ferry
  interval, maxspeed type) to a sparse side table keyed by edge index.
- Strings as `u32` indices into a per-graph string pool; edge ids derived
  from way id and segment index instead of stored.
- Bools packed into one bit set; highway as a small enum.
- Endpoint coordinates read from the node table instead of stored per edge;
  shapes in one shared buffer addressed by offset and length.
- Keep `f64` for length and weights so costs, and therefore routes, stay
  bit-identical; `f32` would risk changing route choice.

Rough size of the hot record: 48 to 64 bytes per edge.

## Cost and risk

- `GraphEdge` is a public struct built directly in about 85 places across
  27 files; it would need a constructor or builder and accessor methods.
- Every graph is affected (tile packs, overlays, corridor skeleton, hop
  graphs, desktop and FFI), not only long trips.
- Pack readers that fill `GraphEdge` from rkyv must map into the new layout;
  the on-disk format itself need not change.
- Acceptance: the regression gate with identical routes on every case, plus
  emulator peak and wall time before and after.
