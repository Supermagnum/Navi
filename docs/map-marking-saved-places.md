# How to use: map long-press and saved places

Mark a point on the map with a **4-second touch-and-hold**, then set it as
**From**, **Via**, or **To**, or **save** it as a named place for later.

This is separate from **Saved routes**, which stores a full planned corridor
(start, end, vias, profile). A **saved place** is only one named coordinate.

Screenshots (SM-P613): [`images/map-long-press/`](images/map-long-press/).

---

## Mark a location on the map

1. Open the map. Collapse the route-planning panel with **Close** if you need a
   clear map (or long-press on a free map area below the panel).
2. Press and hold one finger on the map for **about 4 seconds** without sliding.
3. A **blue ring** fills around your finger while the hold is recognized.
4. When the hold completes, a **Marked location** sheet appears with the
   suggested name (nearest address/place within ~12 m when known, otherwise
   coordinates) and these actions:
   - **Set as From / Start**
   - **Set as Via** (adds another via; you can mark several)
   - **Set as To / Destination**
   - **Save this place** (see below)
   - **Cancel** (or tap outside the sheet)

### What cancels a long-press

| Gesture | Result |
|---|---|
| Hold ~4 s, finger stays put | Menu opens |
| Release before ~4 s (e.g. 1–2 s tap) | No menu; normal short touch behaviour |
| Slide / pan beyond a small drift | Hold cancelled; map pans as usual |
| Pinch-zoom / second finger | Hold cancelled |

Pan, pinch-zoom, tilt, and rotation keep working; the long hold is intentional
so it does not fight everyday map use.

---

## Save this place

1. From the mark sheet, tap **Save this place**.
2. Confirm or edit the **name**, then tap **Save**.
3. The place is stored on the device in the app database (`navi.db`, table
   `saved_places`).

You can also save later from the planning panel once a place is listed.

---

## Use Saved places (From / Via / To)

1. Open **Route** planning chrome if it is closed.
2. Scroll to **Saved places** (directly under **Saved routes**).
3. On a row, tap **From**, **Via**, or **To** — the field is filled the same way
   as choosing a search result or **Use GPS**.
4. **Rename** or **Delete** as needed.
5. Plan the route as usual with **Plan route**.

A saved place is **not** tied to one role: the same entry can be From on one
trip and To on another.

---

## Saved places vs Saved routes

| | Saved places | Saved routes |
|---|---|---|
| What is stored | One lat/lon + name | Full corridor (start, end, vias, profile) |
| Where in UI | **Saved places** panel | **Saved routes** panel |
| Typical use | “Cabin ridge”, “Depot gate”, a map mark | Reuse a planned trip summary |

---

## Tips

- Prefer the keyboard or search for known place names when you have them; use
  map mark when you are pointing at something on the basemap.
- After **Set as …**, the map pin and status toast confirm the field update.
- Multiple vias: mark and **Set as Via** repeatedly, or add vias from search.

---

## Developer pointers

| Topic | Where |
|---|---|
| Gesture + hold ring | `MapLongPress.kt`, `CorridorMapView` in `MainActivity.kt` |
| Field population | `applyHit` / `applyMarkAs` (same path as search) |
| Persistence | `saved_places` in `core/src/storage/schema.rs`, `PlaceStore`, UniFFI `list_saved_places` / `save_named_place` / … |
| File map | [`codebase-map.md`](codebase-map.md) |
| API | [`API.md`](API.md) § Saved places |
