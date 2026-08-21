# Horse-trekking plugin (specification)

**Status:** specification only — not implemented.  
**Path:** `docs/plugins/horse-trekking-spec.md`  
**Architecture:** planned WASM guest via `plugin-host` / `plugin-sdk` and
capability-gated `HostApi` ([`plugins.md`](../plugins.md)). Horse-specific
costing and network preference belong in a future **Horse travel profile** in
core ([`horse-profile.md`](../horse-profile.md)); this plugin covers advisory
content, support-service lookahead, and per-protected-area access guidance that
must not be hard-wired as a single global rule.
**System requirements** (all plugins): user **enable/disable** toggle; any
device link uses host-mediated **USB** / **Bluetooth**
([`plugins.md` — enable/disable](../plugins.md#enable--disable-required),
[USB/Bluetooth](../plugins.md#external-device-io--usb-and-bluetooth-required)).

Working title / id suggestion: `horse_trekking` / `horse_trek`.

---

## Interim stopgap (current product behaviour)

Until a Horse profile and this plugin exist, **Hiking is the accepted interim
stopgap** for horse-trekking planning. That is deliberate and adequate for now.

**Caveat (state plainly):** Hiking mode does **not** account for any of the
horse-specific concerns in this document — including:

- horse access tags (`horse=*`, `route=horse`, bridleway preference),
- stiles / gates / other equestrian obstacles,
- horse-appropriate pace (~7 km/h walk, not hiking’s ~16 min/km),
- horse-scale water volume or watering-place preference,
- veterinarian / farrier / stable lookahead,
- per-protected-area Norwegian *verneforskrift* horse-access rules.

Users who trek with horses on Hiking today should treat the plan as a **foot
network approximation**, not an equestrian-legal or welfare-tuned route.

---

## Disclaimer (must appear in the plugin UI)

This plugin would provide **informational guidance** for horse trekking
(routing preferences, water planning, protected-area access summaries, support
services, and plant-safety notes). It is **not legal advice**, **not a
veterinary or farriery service**, and **not a guarantee** that a trail is open
to horses, that water is safe/sufficient, or that toxic plants are absent.
Laws, park regulations, and map data change. **The rider remains responsible**
for verifying local rules, trail conditions, and animal welfare.

---

## Goals

1. Prefer equestrian-suitable ways via a **soft cost preference** (unpaved /
   bridleway / horse-tagged), with paved fallback when needed — same “prefer,
   with fallback” shape as official-network preference today.
2. Soften routes through noisy / built-up corridors via a **town-bypass style
   cost penalty**, not a hard urban ban.
3. Plan water stops at **horse-scale daily volumes**, preferring larger natural
   sources and dedicated watering places where mapped.
4. Apply **per-protected-area** horse-access rules for Norway (and eventually
   other jurisdictions), declining rather than guessing when the area’s rule is
   unknown.
5. Surface **support-service lookahead** (vet, farrier, stable) along the
   corridor, following the informational pattern in
   [`safety-resupply.md`](safety-resupply.md).
6. Show **static toxic-plant awareness** (especially Tysbast) as unverifiable
   field guidance — never as a routing filter.

## Non-goals

- Implementing the plugin or a Horse profile in this documentation pass.
- Copying BRouter’s `.brf` scripting language into Navi.
- Hard-excluding all paved roads or all urban ways.
- Detecting individual toxic plants from OSM or routing around them.
- Replacing official *verneforskrift* / management-plan text with a guessed
  national default.
- Re-parsing `.osm.pbf` inside the WASM guest.

---

## Relationship to core Horse profile

| Concern | Placement |
|---|---|
| Profile enum, pace (~7 km/h), graph access filters, `route=horse` network match | **Core** profile — see [`horse-profile.md`](../horse-profile.md) |
| Soft cost preference / official-network follow | **Core** costing (extend [`network_pref`](../future-proofing-audit-2026-07.md)-style soft preference; plugin may *request* preference weights via host, not own A*) |
| Water / rest welfare reminders, POI lookahead, plant notes, protected-area advisory | **Plugin** (this spec) |
| Jurisdiction packs for horse access | Plugin + [`jurisdiction-rules.md`](../jurisdiction-rules.md) pattern; Norway protected areas need **per-area** granularity (below) |

---

## Host capabilities (proposed)

Implemented today (reuse): `log`, `position_read`, `poi_query`.

Proposed additions (declare in manifest only after HostApi exists — see
[`plugins.md`](../plugins.md) capability sketch):

| Capability | Purpose for this plugin |
|---|---|
| `position_read` | Current lat/lon |
| `poi_query` | Water, vet, stable, horse_riding, watering_place; FTS-assisted farrier |
| `route_read` (new) | Active corridor samples for lookahead |
| `admin_region_read` (new) | Country / county for jurisdiction pack selection |
| `protected_area_query` (new) | National park / nature reserve / landscape protection polygons crossed by the corridor, with stable area id / name for pack lookup |
| `network_pref_hint` (optional) | Ask host to apply Horse soft-preference weights for the next plan (or document that Horse profile owns this in core) |
| `plugin_kv` / `storage` (new) | User dismissals, last confirmed watering sources |
| `log` | Diagnostics under `Documents/debug/horse-trekking/` |

The guest must **not** open network or invent geometry. Refreshing
*verneforskrift* text is a host/documentation concern, not a silent WASM fetch.

---

## 1. Routing preference (BRouter-informed shape)

[BRouter](https://github.com/abrensch/brouter) is an established open-source
router with a **scriptable cost-factor** profile system. It solves a directly
analogous problem for cycling/trekking. Use it as a **reference model**, not as
something to port.

### 1.1 Cost factors, not hard filters

BRouter assigns each way segment a **cost multiplier** from `highway=` /
`surface=` (and related) tags rather than a binary allow/exclude. A paved road
is not forbidden — it is made **expensive** relative to unpaved tracks/paths so
the router prefers trails when they exist, but can still use a short paved
connector when that is the only link between trail segments.

**Navi horse shape:** the same “prefer, with fallback” pattern already used for
DNT / pilgrim **official network** soft preference
(`core/src/routing/graph/network_pref.rs` and the “Follow official networks”
toggle). A horse profile / plugin should **extend that pattern**:

| Prefer (lower cost) | Penalize (higher cost) | Still allowed if needed |
|---|---|---|
| `highway=bridleway`, `track`, `path` with `horse=yes` / `horse=designated` | High-traffic / paved through-routes when alternatives exist | Short paved / higher-class connectors |
| `route=horse` relation members (via official-network soft pref) | Ways with `horse=no` / `horse=private` (harder or blocked — profile access table) | — |

Do **not** build a second BRouter-style `.brf` scripting engine from scratch.

### 1.2 Noise / traffic avoidance and town bypass

BRouter exposes an explicit “avoid noise” style option and a **town-bypass**
mechanism: an `estimated_town_class` (or equivalent) applied to highways inside
a town/city administrative area, so the router prefers routes that **skirt**
built-up areas rather than cut through them.

**Navi horse shape:**

- Soft cost penalty scaled by how built-up the area is (admin landuse / place
  class / host-supplied town class), **not** a hard ban on urban roads.
- Prefer quieter / lower-class ways when skirting towns.
- Same philosophy as soft network preference: expensive ≠ impossible.

Host may derive a coarse town class from admin boundaries already used for
jurisdiction detection; the plugin documents the desired behaviour, core
costing applies the multipliers when Horse is active.

### 1.3 Obstacles (profile concern, plugin may warn)

Stiles, horse-unfriendly gates, and similar obstacles are primarily a **graph
access / edge exclusion** problem for the Horse profile. The plugin may surface
informational warnings when known obstacle tags appear on the chosen corridor,
but must not invent obstacle geometry. Tag research for obstacles is out of
scope for this pass beyond acknowledging the gap vs Hiking.

---

## 2. Water source handling (horse-scale)

### 2.1 Daily volume (planning basis)

Approximate drinking needs for a ~**500 kg** horse (reference figures for
planning logic — refine with veterinary sources before shipping):

| Condition | Approximate intake |
|---|---|
| At rest | **25–30 L/day** |
| Heat | up to ~**55 L/day** |
| Exercise / trekking | **40–70 L/day** |

These volumes are **materially larger** than the hiking Water POI category
assumes for human bottles. Water-stop **frequency and lookahead buffers** for
horse mode must scale with these figures — do **not** reuse hiking human-scale
water planning unchanged (contrast
[`safety-resupply.md`](safety-resupply.md) foot water buffers).

### 2.2 What counts as a practical watering source

- A small `amenity=drinking_water` tap is often usable with a **collapsible
  bucket** (standard trekking kit). Low individual tap flow rate is **not**
  itself a primary routing concern.
- The real capacity concern is narrower: sources fed from a **cistern or other
  large fixed container** (finite volume, may run dry or need refill time)
  versus a **continuous supply** (running tap, spring, stream). Flag cistern /
  tank-limited sources as lower confidence when tags allow; do not treat all
  taps as insufficient merely because flow is slow.
- **Prefer larger natural sources** where practical:
  - `natural=water`, lakes, ponds,
  - `waterway=stream` / `river`,
  - and prioritize `amenity=watering_place` or POIs adjacent to
    `leisure=horse_riding` when present in the regional extract.

Scoring should follow the same **confidence / informational lookahead** spirit
as [`safety-resupply.md`](safety-resupply.md): warn on long gaps between
Medium-or-better horse-suitable sources; never hard-block the plan solely
because a tap is “only” a tap.

---

## 3. Norwegian protected-area horse access (per-area, not blanket)

### 3.1 Finding

Horse access in Norwegian protected areas is **not** governed by one blanket
national rule. It is regulated **per protected area** via that area’s
*verneforskrift* (protection regulation) and management plan, and **varies
meaningfully**.

General Miljødirektoratet-oriented guidance (verify before shipping):

- **Individual / non-organized** horse use is generally intended to be allowed
  in many landscape protection areas and national parks.
- **Organized** horse use (commercial rides, riding-centre groups) is normally
  restricted to roads and specifically approved trails named in the area’s
  management plan.

Real examples illustrating variation (illustrative; re-check current
regulations):

| Area | Horse-relevant note |
|---|---|
| **Færder NP** | Cycling and riding permitted only on approved roads/trails |
| **Ytre Hvaler NP** | Non-organized riding on roads/trails in outlying land; no areas suited for organized riding; sub-area **Ørekroken** bans riding entirely year-round |
| **Fulufjellet NP** | Organized horse use **not** permitted; non-organized riding permitted |
| **Vistehorten naturreservat** (example reserve) | Bans cycling, riding, and horse use entirely **outside existing roads** — stricter than general NP guidance |

### 3.2 Implementation shape

This is the [`jurisdiction-rules.md`](../jurisdiction-rules.md) pattern, but
granularity is **per protected area**, not only per country/region:

1. Detect whether the route corridor **crosses** a national park, nature
   reserve, or landscape protection boundary (`protected_area_query`).
2. Look up that area’s **keyed pack** (cite *verneforskrift* / management plan).
3. If the specific area’s horse-access rule **cannot be confidently determined**
   → **decline rather than guess** (same standing default as jurisdiction packs
   generally). Do **not** assume “individual riding is fine” — Vistehorten shows
   that default can be wrong for a specific area.
4. Surface an informational banner / card (organized vs non-organized, approved
   trails only, total ban outside roads, etc.). Prefer soft route preference /
   user confirmation over silent hard fails unless the pack states a clear
   prohibition on the ways in use.

Country-level horse access (Sweden, UK bridleways, …) remains as in
[`horse-profile.md` §5](../horse-profile.md#5-jurisdiction-dependent-horse-access);
this section adds the **Norway protected-area** refinement.

---

## 4. Veterinarian, farrier, and stable finding

Frame as **support-service lookahead** along the planned corridor — same family
of idea as [`safety-resupply.md`](safety-resupply.md): search ahead, score
confidence, inform the rider; do not hard-block routing solely because a
service is far away.

| Service | OSM / search notes |
|---|---|
| **Veterinarian** | `amenity=veterinary` — well-established; straightforward POI category |
| **Farrier** | No single well-standardized tag. `craft=farrier` appears in some regions but coverage is sparse. Expect **inconsistent tagging**; combine structured tags with **name/description FTS** (existing place / POI search) rather than relying on one key |
| **Stable / livery** (horse overnight, distinct from rider camping) | Investigate coverage before locking a matcher: `amenity=stable`, `building=stable`, `leisure=horse_riding` (riding centres often offer livery). Same research diligence as the original POI category work — do not assume one tag is enough |

Reuse safety-resupply patterns: corridor POI scan, confidence / staleness,
informational HUD chips, optional user confirmation cache via `plugin_kv`.

---

## 5. Toxic plant awareness (informational only)

Norway has **very few** plants that are directly deadly to horses; riders still
need to recognise the ones that matter. The most commonly cited serious example
in a Norwegian context is:

### Tysbast (*Daphne mezereum*, Daphne / Mezereon)

- **Why it matters:** genuinely toxic; classic field-safety plant for horse
  people in Norway.
- **Recognition:** the rider must identify it **by sight** in the field — not
  only by Latin name. Spec content should include a short visual description
  (habit, flowers, berries) and, if the app asset pipeline supports it, a
  reference photo/illustration in the plugin pack.
- **Plugin treatment:** **static informational content** inside the plugin UI —
  same class as the allemannsretten plugin’s fire-safety and foraging notes
  ([`right-to-roam-camping-spec.md`](right-to-roam-camping-spec.md)): guidance
  the app **cannot verify or enforce**.

**Not routing logic:** individual plant occurrences are not mapped in OSM at
usable granularity. Do **not** GPS-trigger or route-around plant warnings.
A fuller list of horse-relevant toxic plants beyond Tysbast may be added later
as more static content under this same informational note — still not a
data-driven map layer.

---

## 6. Suggested UI surfaces (when built)

- Profile: Horse (core) + optional “Horse trekking aids” plugin toggle.
- Pre-plan / post-plan cards: water gap warning, protected-area access summary,
  nearest vet / farrier / stable along corridor.
- About / field notes: Tysbast (and later plants) with visual reference.
- Always show the disclaimer above.

Debug artifacts (when a host write path exists):  
`Documents/debug/horse-trekking/…` per [`plugins.md`](../plugins.md).

---

## 7. Implementation status

| Item | Status |
|---|---|
| This plugin | **Spec only** — not implemented |
| Horse travel profile / `route=horse` preference | **Not implemented** — see [`horse-profile.md`](../horse-profile.md) |
| Hiking as horse-trekking stopgap | **Accepted interim**; does not apply horse-specific rules above |
| Per-area Norwegian *verneforskrift* packs | Candidate only; decline-if-unknown |
| Horse water / support-service lookahead | Spec only |
| Toxic-plant routing | Out of scope; informational note only |

---

## Related

- Plugin index: [`plugins.md`](../plugins.md)
- Horse profile walkthrough: [`horse-profile.md`](../horse-profile.md)
- Jurisdiction pattern: [`jurisdiction-rules.md`](../jurisdiction-rules.md)
- Safety / resupply lookahead: [`safety-resupply.md`](safety-resupply.md)
- Right-to-roam camping (informational guidance precedent):
  [`right-to-roam-camping-spec.md`](right-to-roam-camping-spec.md)
- POI categories: [`poi.md`](../poi.md)
