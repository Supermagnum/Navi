# Worked example: adding a Horse profile

This walkthrough shows how a new travel profile intersects the jurisdiction
pattern in [`jurisdiction-rules.md`](jurisdiction-rules.md). **Horse is not
implemented** — OSM already knows `route=horse`, and Navi’s network-preference
tests explicitly show that tag is **not** matched by Hiking or Cycling packs
today (`OfficialNetworkKind` only lists hiking/foot and bicycle/mtb).

Related:

- Jurisdiction pack pattern: [`jurisdiction-rules.md`](jurisdiction-rules.md)
- Icons: [`icons.md`](icons.md)
- Profile / rest overview: [`../README.md`](../README.md)

---

## 1. Profile definition

- Add `Horse` beside existing profiles (Hiking, Cycling, Car, Truck, and their
  electric variants / MobileHome as already established).
- **Cost model:** start from **Hiking** (human/animal-powered, non-motor), not
  from Car/Truck. Adjust pace (below), not a greenfield cost formula.
- **Eco-mode:** locked **on** by default, same idea as Hiking / Cycling — there
  is no meaningful “eco off” for a horse profile.

## 2. Road / path preference

- **Official networks:** reuse the existing soft network-preference mechanism
  (“Follow official networks”). Matching is already generic
  (`type=route` + `route=*` + `network=*`). Adding Horse means recognizing
  `route=horse` (and an appropriate `OfficialNetworkKind::Horse` or equivalent)
  — **not** inventing a new matcher.
- **Ways:** prefer `highway=bridleway`, `highway=track`, `highway=path` where
  `horse=yes` / `horse=designated`; start from Hiking’s forbidden high-class
  road filter (avoid motorway / trunk / primary as a baseline), then adjust for
  horse-specific access (`horse=no` / `horse=private` exclusions that do not
  apply the same way to foot).

## 3. Pace / duration estimation

Pre-departure ETA already uses a small pace table (motor: OSM `maxspeed` +
highway fallback; hiking ~16 min/km; cycling ~4 min/km). Document Horse as a
**new illustrative row** in that same table:

| Mode | Illustrative default | Notes |
|---|---|---|
| Hiking | ~16 min/km | Existing |
| Cycling | ~4 min/km | Existing |
| **Horse (walk)** | ~**8.5 min/km** (~**7 km/h**) | Walking pace for distance planning; trot/canter are not sustainable all-day defaults |

Treat ~7 km/h as a **starting welfare/practical estimate to refine**, not as a
regulation-grade figure like EC 561’s hours.

## 4. Rest / break parameters

Horse has **no** EC 561-style regulatory source in Navi’s truck work. Rest /
water stops for a horse are a **welfare and practical judgment**, not a hard
legal duty pack.

Per [`jurisdiction-rules.md` §2.4](jurisdiction-rules.md#24-where-rule-sets-live-core-vs-plugin):

- Soft interval suggestions (water / rest reminders) → **plugin-appropriate,
  advisory-only** (or soft car/hiking-style reminders clearly labelled as
  non-legal).
- Do **not** build Horse rest as a hard-enforced core constraint in the EC 561
  sense unless a cited regulation for the active jurisdiction requires it.

## 5. Jurisdiction-dependent horse access

Horse **access rights** vary by country in ways that parallel the right-to-roam
table: e.g. Sweden’s *allemansrätten* and UK access regimes have horse-specific
provisions that are **not** identical to foot or bicycle rights. Therefore:

- Adding Horse surfaces a **new jurisdiction-dependent rule-set candidate**
  (horse right-of-access / bridle access per country) — listed in
  [`jurisdiction-rules.md` §4](jurisdiction-rules.md#4-candidate-future-rule-sets-list-only).
- Do **not** automatically inherit Hiking’s allemannsretten / overnight camping
  treatment for horses without verifying horse access in that jurisdiction.
- Follow the same detection → keyed pack → decline-if-unknown → cite-sources
  process as [`jurisdiction-rules.md`](jurisdiction-rules.md).

## 6. Icon and UI

- Check the bundled Navit-derived set under `core/src/icons` (see
  [`icons.md`](icons.md)) for an equestrian / horse / bridleway-appropriate
  asset **before** commissioning a custom icon. As of this writing there is no
  dedicated `horse*` icon in that tree; if still absent, add an SVG via the
  normal custom-icon path and document provenance.
- **Menu placement:** treat Horse like other secondary profiles (similar to how
  electric truck variants are not the primary four-mode focus) unless product
  later promotes it — i.e. available in the profile enum / extended picker, not
  necessarily one of the primary HUD shortcuts on first ship.

## 7. Implementation status

| Item | Status |
|---|---|
| Horse profile / routing / ETA | **Not implemented** (doc only) |
| `route=horse` network preference | Tag exists in OSM; Navi matcher does not select it yet |
| Horse jurisdiction access packs | Candidate only ([`jurisdiction-rules.md` §4](jurisdiction-rules.md#4-candidate-future-rule-sets-list-only)) |
| Horse rest regulation pack | None; advisory welfare if added |
