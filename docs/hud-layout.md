# Drive HUD and menu layout

How to change the **size** and **placement** of the top/bottom drive bars and
the menus that sit with them. All of this is Compose UI in the Android host;
there is no runtime “theme size” setting yet — edit the Kotlin sources and
rebuild the APK.

Reference screenshot (map + bars only): see the
[README Screenshots section](../README.md#screenshots)
(`docs/images/hud/hud_idle_both_bars.png`).

## Collapsed vs expanded (default behaviour)

Both drive bars are **collapsed by default**.

| Bar | Collapsed content | Tap |
|---|---|---|
| Top (`TopDriveHud`) | Map label, altitude, rotation hint | Toggles `MapSettingsSheet` (rotation, Trip ETA, Breaks, Auto-zoom, experimental 3D) |
| Bottom (`BottomDriveHud`) | Zoom −/+, **Recenter** (when the user has panned away from GPS follow), **current street** (`Currently on …`, low weight), **speed / limit** (`hud_current_speed`; error colour when `OverspeedHud.isOverspeed`), trip ETA, eco leaf; **break countdown (time or distance) only when a route is planned** | Status area toggles `DriveSettingsSheet` (rest / fuel / eco / break display mode) |

Altitude: when a DEM tile covers the fix, the HUD shows terrain height from
on-disk Copernicus/SRTM (~MSL), not `Location.altitude` (AVD/network providers
often report a plausible but wrong value). GPS altitude is used only if no DEM
tile is present. Fixes without usable vertical data and no DEM show `Alt --`.
Instrumented screenshots inject `NaviMapTestHooks.gpsAltitudeM` for a stable
non-zero reading.

Sheets close on Save/Close, or by tapping the same bar again. Tapping the
map does not open or close either sheet. Every menu (Map settings, Drive
settings, Tools, Profile, Vehicle, Saved routes, Saved places) exposes **Save**
and **Close** where applicable.
**Delete route** clears the active planned corridor from the Route panel (when a
polyline is on the map) and removes a stored entry from **Saved routes**
(per-row **Delete route**). Saved routes also offer **Delete planned route**
while a corridor is active on the map.
**Saved places** (sibling panel under Saved routes) stores named single
coordinates; row actions set **From** / **Via** / **To**, or Rename / Delete.
Map long-press (~4 s) opens a mark sheet — see
[`map-marking-saved-places.md`](map-marking-saved-places.md).
**Close** on Profile / Vehicle / Saved routes / Saved places, or the **Close**
next to Tools in the planning chrome, dismisses the whole route-planning panel
so the map is usable; reopen with the **Route** button under the top Map bar.

Route planning progress is visible in logcat:

```bash
adb logcat -s NaviRouting:I
```

Lines include `planning_progress pct=… eco=…`, then after 100%
`planning_done duration_ms=…` and `planning_pois count=… names=…`.
The on-screen plan bar reads **plan** progress only (`planProgressSnapshot`);
indexed-map convert and speed-limit cone use separate native channels so they
do not move the plan percent.

**Zoom:** the app owns **one** zoom −/+ set on the bottom bar. AAOS system chrome
often shows separate climate − N + controls; those are not map zoom.

**Eco:** rasterized `leaf.svg` (via `eco-mode` / icon pipeline) on the **bottom**
bar only when eco-mode is active — not a text “ECO” label.

**Turn stubs:** not on the bottom bar. See [`approach-instructions.md`](approach-instructions.md)
(temporary approach box). **Current street** is on the bottom bar — see
[`current-street.md`](current-street.md).

**Status toast:** bottom-**end** chip (`status_toast`), above the bottom bar —
never bottom-left over MapLibre/OSM attribution (covers OpenFreeMap and
Protomaps offline styles alike). Basemap style selection is documented in
[`map-styles.md`](map-styles.md).

**Settings sheets:** floating overlays (`zIndex` above map + bars), not siblings
that insert into the top/bottom chrome columns.

Garmin reference proportions (~14% top instruction / ~6.4% bottom strip) inform
collapsed `heightIn(min = …)` floors and the future approach box — size by
content first.

**Measured on emulator (2026-07-22)** from
`docs/images/hud/hud_idle_both_bars.png` (1280×720): collapsed top
**6.67%** (48 px), collapsed bottom **8.89%** (64 px). See
[`android-test-results.md`](android-test-results.md) for historical toast / leaf
notes.

## Screen stack (placement)

`NaviMapScreen` in [`app/src/main/java/no/navi/app/MainActivity.kt`](../app/src/main/java/no/navi/app/MainActivity.kt)
is a full-screen `Box`:

| Layer | Alignment | Role |
|---|---|---|
| `CorridorMapView` | fill | MapLibre map |
| Top `Column` | `TopCenter` | Top drive HUD + search / profile chrome (scrollable) |
| Tools `Surface` | `BottomCenter` | Region panel when Tools is open |
| Bottom `Column` | `BottomCenter` | Bottom HUD only |
| Status chip | `BottomEnd` | Ephemeral status (settings applied, Ready, …) |
| Map / drive settings sheets | Top/Bottom center | Floating overlays above bars (`zIndex`) |

```
+------------------------------------------+
|  system status bar (AAOS)                |
+------------------------------------------+
|  [TopDriveHud]                           |  <-- TopCenter Column
|  [search_chrome / profile_menu / ...]    |      padding 10.dp, max height 520.dp
|                                          |
|              map                         |
|                                          |
|  [MapSettingsSheet / DriveSettingsSheet] |  <-- overlay (floats above bars)
|  [tools_menu]  (optional)                |  <-- BottomCenter, +88.dp lift
|  [BottomDriveHud]                        |  <-- BottomCenter Column
|                     [status toast] ----> |  <-- BottomEnd (not over attribution)
+------------------------------------------+
|  system nav / climate bar (AAOS)         |
+------------------------------------------+
```

### Move the top stack

In `MainActivity.kt`, top chrome:

```kotlin
Column(
    modifier = Modifier
        .align(Alignment.TopCenter)   // placement on the map
        .fillMaxWidth()
        .padding(10.dp)               // inset from screen edges
        .heightIn(max = 520.dp)       // cap before scrolling
        .verticalScroll(...),
)
```

| Want | Change |
|---|---|
| Closer to the top edge | Lower `.padding(10.dp)` (e.g. `4.dp`) |
| Narrower / side margins | Use `.padding(horizontal = …, vertical = …)` |
| Shorter scroll window | Lower `heightIn(max = 520.dp)` |
| Pin to a corner | `Alignment.TopStart` / `TopEnd` instead of `TopCenter` |
| Gap under top HUD before search | `TopDriveHud(…, modifier = Modifier.padding(bottom = 8.dp))` |

### Move the bottom stack

```kotlin
Column(
    modifier = Modifier
        .align(Alignment.BottomCenter)
        .fillMaxWidth()
        .padding(10.dp),
    verticalArrangement = Arrangement.spacedBy(8.dp),
)
```

| Want | Change |
|---|---|
| Clear AAOS system nav / climate bar | Increase bottom padding (e.g. `.padding(bottom = 24.dp)` in addition to `10.dp`) |
| Tighter gap between settings sheet and bar | Lower `Arrangement.spacedBy(8.dp)` |
| Status chip above the bar instead of below | Reorder children: status `Text` before `BottomDriveHud` |

The tools panel sits above the bottom HUD with an extra lift so it does not
cover zoom/settings:

```kotlin
.padding(bottom = 88.dp)   // lift tools_menu above BottomDriveHud
.heightIn(max = 360.dp)
```

Raise `88.dp` if the bottom HUD grows; lower it if you want the tools panel
closer to the bar.

## Bar size (DriveHud.kt)

Composables live in
[`app/src/main/java/no/navi/app/DriveHud.kt`](../app/src/main/java/no/navi/app/DriveHud.kt).

Bars are `Surface` + padding. They **grow with content**; there is no fixed
pixel height. To make a bar taller/shorter or denser:

### Top bar (`TopDriveHud`)

| Knob | Current | Effect |
|---|---|---|
| Corner radius | `RoundedCornerShape(10.dp)` | Softer / sharper panel |
| Elevation | `tonalElevation = 3.dp` | Shadow / separation from map |
| Inner padding | `Modifier.padding(8.dp)` | Overall bar thickness |
| Row spacing | `Arrangement.spacedBy(6.dp)` / `10.dp` | Vertical / horizontal density |
| Altitude type | `MaterialTheme.typography.titleMedium` | Readout size |
| Labels | `bodySmall` / `labelLarge` | Chip and toggle label size |
| Filter chips | Material3 `FilterChip` defaults | Compass / Travel / N-up hit targets |
| Auto-zoom −/+ | `TextButton` | Step control size |

Example: denser top bar for a smaller head unit —

```kotlin
Column(
    modifier = Modifier.padding(4.dp),
    verticalArrangement = Arrangement.spacedBy(4.dp),
) { /* … */ }
```

### Bottom bar (`BottomDriveHud`)

| Knob | Current | Effect |
|---|---|---|
| Corner radius | `RoundedCornerShape(10.dp)` | Panel shape |
| Elevation | `tonalElevation = 4.dp` | Separation from map |
| Inner padding | `padding(horizontal = 8.dp, vertical = 6.dp)` | Bar height |
| Item gap | `Arrangement.spacedBy(4.dp)` | Space between zoom, turn text, Settings |
| Turn / break type | `titleMedium` / `bodySmall` | Primary status size |
| Eco leaf | `Modifier.size(36.dp)` | Icon size |
| Zoom −/+ | `TextButton` with tags `zoom_out` / `zoom_in` | Always-visible zoom |

Example: larger zoom hit targets for driving —

```kotlin
TextButton(
    onClick = onZoomIn,
    modifier = Modifier
        .testTag("zoom_in")
        .size(56.dp),   // or padding / MinTouchTarget
) { Text("+", style = MaterialTheme.typography.headlineSmall) }
```

### Drive settings sheet (`DriveSettingsSheet`)

Opens **above** the bottom bar (same bottom `Column`).

| Knob | Current | Effect |
|---|---|---|
| Max height | `heightIn(max = 360.dp)` | Sheet scroll window |
| Outer padding | `padding(10.dp)` | Margin vs screen |
| Inner padding | `padding(12.dp)` | Field density |
| Field spacing | `spacedBy(8.dp)` | Vertical rhythm |

## Menu items (search / profile / tools)

These are **not** in `DriveHud.kt`; they are `Surface` blocks in the top
scroll column (and the tools overlay) in `MainActivity.kt`.

| Menu | `testTag` | Where |
|---|---|---|
| Search (To / Via / Place / Address / Tools) | `search_chrome` | Top column, under top HUD |
| Profile chips + eco | `profile_menu` | Top column, further down (scroll) |
| Tools / region | `tools_menu` | Bottom overlay when Tools is toggled |
| Drive settings | `drive_settings_title` | Above bottom HUD |

### Size

Each menu panel uses roughly:

```kotlin
Surface(
    shape = RoundedCornerShape(12.dp),
    tonalElevation = 4.dp,   // tools uses 6.dp
    modifier = Modifier.fillMaxWidth(),
) {
    Column(modifier = Modifier.padding(10.dp)) { /* chips, fields, buttons */ }
}
```

| Want | Change |
|---|---|
| Larger tap targets on chips | Increase chip / `FilterChip` padding, or wrap in taller `Row` |
| More space between To/Via/Place/Address | `Arrangement.spacedBy(8.dp)` on that row |
| Compact profile row | Lower `spacedBy(6.dp)` and panel `padding(10.dp)` |
| Shorter tools panel | Lower `heightIn(max = 360.dp)` on `tools_menu` |

Tools includes **Download basemap (PMTiles)** (`btn_download_pmtiles`) and
**Download terrain DEM (Mapterhorn)** (`btn_download_dem`) for the selected
Geofabrik path. See [`map-styles.md`](map-styles.md).

### Placement / order

Top column child order (top → bottom) when chrome is visible:

1. `TopDriveHud`
2. `search_chrome` (if search not hidden)
3. Profile / eco / vehicle / saved-routes blocks (`profile_menu`, etc.)

To put profile **above** search, move the `Surface(… testTag("profile_menu"))`
block above the `search_chrome` `Surface` in `MainActivity.kt`.

To hide search but keep bars (HUD screenshots / focus mode):
`NaviMapTestHooks.hideSearchChrome = true` (or the matching UI path that sets
`hideSearch`).

Full chrome off: `NaviMapTestHooks.hideUiChrome = true`.

## Typography (global size feel)

Bars and menus use `MaterialTheme.typography.*` from the default Material3
theme set in `MainActivity` (`MaterialTheme { … }`). To enlarge **all** HUD
text at once, wrap content in a custom `Typography` / `MaterialTheme` with
larger `bodySmall` / `titleMedium` / `labelLarge`, or pass explicit
`TextStyle` / `fontSize` on individual `Text` calls in `DriveHud.kt`.

## After you change layout

1. Rebuild/install: see [`android-build.md`](android-build.md).
2. Re-run `HudVerificationInstrumentedTest` so `docs/images/hud/` shots match.
3. Check on a real head unit — AAOS system bars differ from the emulator;
   bottom padding (`88.dp` tools lift, bottom `Column` padding) usually needs
   the most tuning ([`real-hardware-testing.md`](real-hardware-testing.md)).
