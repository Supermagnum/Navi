# Animated icons plugin (specification)

**Status:** specification only — not implemented.  
**Path:** `docs/plugins/animated-icons-spec.md`  
**Architecture:** planned host-side (and optional WASM guest) extension on top of
the existing SVG icon pipeline ([`icons.md`](../icons.md),
[`plugins.md`](../plugins.md)). Frame playback and timeline selection stay out
of the trusted routing core; the core continues to resolve and rasterize
**one SVG document per call**.
**System requirements** (all plugins): user **enable/disable** toggle; any
device link uses host-mediated **USB** / **Bluetooth**
([`plugins.md` — enable/disable](../plugins.md#enable--disable-required),
[USB/Bluetooth](../plugins.md#external-device-io--usb-and-bluetooth-required)).

Working title / id suggestion: `animated_icons` / `icon_anim`.

Static custom icons (Inkscape → `.svg` / `.svgz`) remain documented in
[`icons.md`](../icons.md#adding-custom-icons). This plugin covers **motion**:
authoring in Synfig Studio, packaging frames or timed SVG, and presenting them
in the Android / future hosts.

**Road-sign catalogue icons** (`core/src/icons/road-signs/`, NLOD; see
[`road-signs.md`](../road-signs.md)) stay **static** for v1 approach chrome.
Optional animated urgency emphasis for `no_sign_*` keys is a future overlay on
this plugin — do not re-author the Statens vegvesen catalogue inside Synfig as
the source of truth.

---

## Current product state (do not invent a player)

As of this writing:

- UniFFI `rasterize_icon_png` / core `rasterize_key` render a **single** SVG or
  `.svgz` to a bitmap via `usvg` / `resvg`.
- There is **no** Synfig `.sif` / `.sifz` playback in Rust, and no Compose
  frame-sequence player for map markers or HUD chrome.
- Map and HUD icons that need motion today ship a **still** (key frame or
  representative pose) under the normal override / `core/src/icons` paths.

This plugin is how a future contributor would add real animation **without**
embedding a Synfig runtime in the navigation core.

---

## Goals

1. Author animations in **[Synfig Studio](https://www.synfig.org/)** and export
   interchange formats Navi hosts can consume (SVG frames, SVG stills, or a
   small frame pack).
2. Name assets after the same **semantic keys** as static icons
   (`fuel`, `leaf`, `nav_straight_bk`, …).
3. Keep resolution order compatible with today: override dir → bundled set →
   `unknown.svg`, with animation packs as an **optional overlay**.
4. Never block routing / T2 UI: decode and advance frames on a host animation
   budget only.

## Non-goals

- Implementing Synfig playback inside `driver_break_core`.
- Replacing the static SVG pipeline for POI / maneuver / status icons.
- Shipping proprietary binary icon sources as the on-device format.
- Network fetch of animation packs from a WASM guest (install is host/Tools).

---

## Authoring workflow (Synfig → Navi)

1. Create the animation in **Synfig Studio** (square composition, e.g. 128×128;
   high contrast; few effects that do not survive SVG export).
2. Export for Navi in one of these forms (preferred order for map markers):
   - **Still SVG** — export a key frame (or “SVG” export of a representative
     pose), clean in Inkscape if needed, install like a static icon so the
     existing rasterizer works immediately.
   - **SVG frame sequence** — one plain `.svg` per frame:
     `{key}_f000.svg`, `{key}_f001.svg`, … or a subdirectory
     `{key}/000.svg`, `{key}/001.svg`, …
   - **Optional pack manifest** — list frame files, fps, loop mode (see below).
3. Do **not** expect the core to load `.sif` / `.sifz` at runtime; keep Synfig
   project files in source control or an artist tree only if desired.
4. Place exported SVG under the **override directory** or `core/src/icons/`
   using the semantic key basename (same rules as
   [`icons.md`](../icons.md#adding-custom-icons)).

Day/night: if the static set uses `_bk` / `_wh`, ship matching animated
variants (`nav_turn_left_wh_f000.svg`, …) or document that night falls back to
the static `_wh` still.

---

## Pack layout (proposed)

Offline directory (example):

```text
{dataDir}/icon_anim/
  leaf/
    manifest.json
    f000.svg
    f001.svg
    f002.svg
  fuel/
    manifest.json
    f000.svg
    …
```

`manifest.json` (sketch):

```json
{
  "schema": 1,
  "key": "leaf",
  "fps": 12,
  "loop": true,
  "frames": ["f000.svg", "f001.svg", "f002.svg"]
}
```

Resolution proposal when the plugin is enabled:

1. If `{dataDir}/icon_anim/{key}/manifest.json` exists and the host player is
   active → animate those frames.
2. Else fall through to `resolve_icon` (override → `core/src/icons` →
   `unknown.svg`) and show a static raster as today.

---

## Host capabilities (proposed)

| Capability | Purpose |
|---|---|
| `log` | Diagnostics |
| `icon_anim_query` (new) | List installed animation packs (key, fps, frame count, path/hash) |
| `icon_anim_frame` (new) | Resolve frame SVG bytes or path for `key` + `frame_index` into a guest/host buffer |
| `plugin_kv` / `storage` | Optional: last pack hash, user “reduce motion” preference |

**Preferred split:** the **host** (Compose / MapLibre marker layer) owns the
frame clock and calls existing `rasterizeIconPng` (or an equivalent that reads
SVG bytes) once per visible frame. The WASM guest is optional for validating
packs or choosing which keys animate. A minimal first ship can be **host-only**
frame loading with no WASM.

The guest must **not** open sockets to download packs. Pack install is a
host/Tools action (same pattern as PMTiles / voice / i18n packs).

---

## UI / accessibility

- Respect a host **reduce motion** preference: show frame 0 (or the static
  override) only.
- Cap on-screen animated markers (e.g. only HUD eco leaf + selected POI), not
  every amenity in the viewport.
- Keep map pins readable at 32–64 px; animation must not depend on fine stroke
  detail.

---

## Testing

| Check | Expectation |
|---|---|
| No anim pack | Static SVG path unchanged (`leaf.svg` / Navit set) |
| Pack installed, player on | Frames advance at `fps`; loop respects manifest |
| Reduce motion on | Still = first frame or static override |
| Missing frame file | Fall back to static `resolve_icon`; log once |
| Semantic key mismatch | Pack ignored; no crash |

---

## Relation to static icons

| Concern | Doc |
|---|---|
| Format (SVG / SVGZ), Inkscape steps, override dir, inventory | [`icons.md`](../icons.md) |
| Motion authoring (Synfig), frame packs, host player | This plugin spec |

Artists may still use Synfig only to produce a **still** SVG and install it via
`icons.md` — that does not require this plugin.

---

## Design rules (same family as other plugins)

1. Offline-first; pack install is user-initiated on the host.
2. User **enable/disable** per [`plugins.md`](../plugins.md#enable--disable-required);
   core routing and static icon resolution work with the plugin disabled.
3. No silent mutation of OSM / graph caches.
4. Tier budgets: animation decode stays T0/T1 — never starve guidance audio/UI.
5. Privacy: packs and preferences stay on device.
6. Hardware I/O (if any) is host-mediated **USB** / **Bluetooth** only
   ([`plugins.md`](../plugins.md#external-device-io--usb-and-bluetooth-required)).
