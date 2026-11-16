# Vehicle limits on the MWM path: design note (not built)

Decision 15a of `docs/mwm-phase6b-drape.md`: truck, mobile home, electric car and motorcycle routes are
planned as the CoMaps car profile and the UI says "limits not applied on this map type". No truck model is
built in Phase 7. This note records how the limits could be added later without a CoMaps vehicle model,
and what it would cost, so the owner can decide.

## Idea: a blocking hook, like the wetland Hard class

Wetland Hard segments are already blocked inside the CoMaps edge-cost path by `navi_wetland_cost`
(`patches/comaps/navi_wetland_cost.{hpp,cpp}`), fed by the `wetland_edges` section of the per-leaf sidecar.
Vehicle limits fit the same shape: a per-segment fact that either forbids the segment for this vehicle or
does not.

### Server side (navi-server sidecar)

A new optional NVSC section `vehicle_limits` next to `wetland_edges` (format change = new section id, old
clients skip unknown optional sections; see `docs/sidecar-format.md` on navi-server):

| Field | Type | Unit | Source OSM tags |
|---|---|---|---|
| `feature_id` | u32, sorted, delta coded | MWM feature id (per leaf) | way split into MWM features |
| `max_height_cm` | u16 (0 = none) | cm | `maxheight` (also `maxheight:physical`) |
| `max_weight_10kg` | u16 (0 = none) | 10 kg | `maxweight`, `maxweight:hgv`, `maxgcw` |
| `max_width_cm` | u16 (0 = none) | cm | `maxwidth`, `maxwidth:physical` |
| `max_length_cm` | u16 (0 = none) | cm | `maxlength` |
| `max_axleload_10kg` | u16 (0 = none) | 10 kg | `maxaxleload` |
| `flags` | u8 | bit 0 `hgv=no`, bit 1 `hgv=destination`, bit 2 `motorcycle=no`, bit 3 `conditional` (a `:conditional` variant exists, not evaluated) | `hgv`, `motorcycle`, `*:conditional` |

Rules for the generator: only values that parse to a plain number (with unit `m`, `ft in`, `t`, `kg`, `lbs`)
are emitted; `none`, `default`, `below_default` and text values are dropped (and counted in the sidecar's
attribution / inspect output). One record per MWM feature the way maps to (a way split by the generator gets
the value on each part). Fingerprint rule unchanged: the section is only applied when the sidecar matches
the leaf MWM.

### Client side

1. UI: the vehicle settings that already exist (`vehicle_height_m`, `vehicle_width_m`, `vehicle_length_m`,
   `vehicle_total_weight_kg`, `vehicle_axle_weight_kg`, `vehicle_bogie_weight_kg`, saved in `MapHudPrefs`) are
   handed to the shim with the route options (`comapsSessionSetVehicle(limits)`).
2. `SidecarReader` gains a `vehicle_limits` lookup (binary search over the sorted feature ids, same as
   wetland). The existing callback from CoMaps into Rust (`wetland lookup`) is extended with a second
   function `vehicle_limit(leaf, feature_id) -> record`.
3. `navi_wetland_cost.cpp` (or a sibling `navi_vehicle_cost.cpp`) multiplies the segment weight by
   infinity (the router's "forbidden" value, as for wetland Hard) when any limit of the record is below the
   vehicle's value, `hgv=no` applies to a truck, or `motorcycle=no` to a motorcycle. A zero limit means
   unknown and never blocks: the hook may only remove roads that are provably too small, it cannot prove
   that a road is passable.
4. When the hook blocks the only way to a destination the router returns `ROUTE_NOT_FOUND`; the planner
   already maps that to a readable message. A truck-specific hint ("a height or weight limit may block the
   way") is a UI string.
5. Honest limits of the approach, to keep in the UI wording when it is built: no bridge or tunnel clearance
   that is not tagged, no `:conditional` evaluation (time windows, seasons), no turn restrictions that only
   apply to trucks, no truck-specific speeds (CoMaps car speeds stay), and the "unknown = allowed" rule.

### Parity and tests

- Host: unit tests for the record codec (like `sidecar_tests.rs`), the unit parser on real tag samples, and a
  fixture of ten known Norwegian restrictions (for example a low bridge with `maxheight=3.6`).
- Device: the Phase 4 / 6b parity suite gets a `truck` column: routes that must avoid a restricted segment,
  and routes that must still succeed because the limit does not apply.

## Size estimate

Measured on `norway-latest.osm.pbf` (1.37 GB, the file used by the Phase 4 fixtures), counting OSM ways with a
`highway` tag and the limit tags above (`vlcount`, a throw-away reader over the same PBF):

| Item | Count |
|---|---|
| Highway ways in Norway | 1,939,928 |
| Ways with any of the limit tags | 7,603 (0.39 %) |
| `maxheight` / `maxlength` / `maxweight` / `maxwidth` / `maxaxleload` | 3,339 / 2,970 / 1,043 / 116 / 264 |
| `hgv=no` or `hgv=destination` | 290 |

Assuming about 1.5 MWM features per way (an assumption: the real split depends on the generator) that is
roughly 11,500 records for all of Norway. A record is 4 B feature id (delta coded, about 2 B) + 5 x 2 B limits + 1 B flags = about 13 B,
so **about 160 kB raw, about 60 to 90 kB after zstd, for the whole country**, spread over the Norway leaves
(Østlandet's nine leaves would carry well under 100 kB together). For other countries the tag rate is not measured here; assuming two to
three times the Norwegian rate, so a worldwide release (1,159 routable leaves) stays under **10 MB of
sidecar data in total**, against about 2.1 GB for the current sidecar tree. The download cost for a user is
negligible. This is an extrapolation from one country; the generator would print the real numbers per
release.

Client memory: sorted `Vec` of 13 B records, binary search, same as the wetland section: a few hundred
kB for any realistic leaf set.

## Effort

| Part | Work |
|---|---|
| navi-server generator + NVSC section + docs | about 3 days (OSM unit parsing is most of it) |
| `SidecarReader` section + tests | about 1 day |
| CoMaps hook (about 60 lines C++ in the wetland patch) + FFI setter | about 1 day |
| UI wiring + wording + removing the "not applied" label for the supported profiles | about 1 day |
| Parity suite and device run | about 2 days |

Roughly 8 working days, no CoMaps vehicle model, no change of the MWM files.

## What stays as it is in Phase 7

- Profiles truck, mobile home, electric car and motorcycle keep the label "Vehicle limits (height, weight,
  axle load) are not applied on this map type" (`MwmProductionPlanner.limitsNotApplied`).
- The function matrix keeps the four PARTIAL rows with that reason.
