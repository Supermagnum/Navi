# Jurisdiction-dependent rules (reusable pattern)

Navi already has two working precedents where behaviour depends on **where
you are** (GPS fix, overnight candidate, or route corridor), not only on the
selected travel profile. This document extracts a **general, reusable pattern**
from those examples so future contributors can add another jurisdiction-dependent
rule set without re-deriving the approach each time.

**This page is documentation only.** It does not implement new rule packs, a
Horse profile, or new admin-boundary APIs. Cite the source docs for the live
details of each example. For the Horse profile walkthrough, see
[`horse-profile.md`](horse-profile.md).

Related:

- Truck driving hours: [`ec-561-truck-rest.md`](ec-561-truck-rest.md)
  (MobileHome separation:
  [`#mobilehome-private-motorhome--deliberate-separation`](ec-561-truck-rest.md#mobilehome-private-motorhome--deliberate-separation))
- Right-to-roam camping (plugin spec): [`plugins/right-to-roam-camping-spec.md`](plugins/right-to-roam-camping-spec.md)
- Horse profile worked example (doc only): [`horse-profile.md`](horse-profile.md)
- Plugin host / capabilities: [`plugins.md`](plugins.md)
- POI overnight categories: [`poi.md`](poi.md) (**RestArea**, **Lodging**)
- Profile / rest overview: [`../README.md`](../README.md)

---

## 1. Two grounding examples

### 1.1 EC 561/2006 truck driving hours (core)

[`ec-561-truck-rest.md`](ec-561-truck-rest.md) documents Navi’s **Truck** /
**TruckElectric** duty and rest rules taken from EU Regulation EC 561/2006
(break after 4.5 h, daily / weekly / fortnightly caps, daily and weekly rest,
exceptional extension, multi-day segmentation).

**Jurisdiction scope (as currently implemented):** the parameter set is the
**EU/EEA-aligned** commercial HGV rule family. Research during that work treated
EC 561 as applying (or closely mirrored) for EU member states plus Switzerland,
Iceland, Norway, and Liechtenstein; the **AETR** agreement covers a parallel but
similar rule family for additional non-EU signatories (e.g. Turkey, Ukraine).
Navi’s shipped truck rest params today are a **single EC 561-shaped pack**, not
yet a keyed multi-country table — but the *domain* is inherently
jurisdiction-bound: applying those numbers outside the EC 561 / AETR family
would be the wrong law.

**Placement:** **core**. Caps and rest placement affect route planning and
persisted `TruckDrivingHistory` directly. MobileHome deliberately does **not**
share this pack (vehicle clearance ≠ commercial HGV legal tracking).

**Sources:** EU official driving-time summary and regulation articles cited in
the EC 561 doc — parameters are not invented without a citation trail.

### 1.2 Allemannsretten / right-to-roam camping (plugin)

[`plugins/right-to-roam-camping-spec.md`](plugins/right-to-roam-camping-spec.md)
specifies a **country-aware** wild-camping suggestion guest: Norway
(*allemannsretten*) as the detailed primary pack, with a comparison table for
Sweden, Finland, Scotland, Iceland, Estonia, and the explicitly narrower
England/Wales and Denmark cases.

**Jurisdiction scope:** country packs with **sub-national** notes where needed
(e.g. Northern Norway cloudberry guidance only for Nordland / Troms / Finnmark).
Unknown country → **decline** wild-camp suggestion rather than silently reuse
Norway’s 150 m / two-night logic.

**Placement:** **plugin** (spec; not shipped yet). Suggestions are advisory /
informational; they must not silently invent “legal camping” in jurisdictions
where Nordic wild-camp logic does not apply. Host would expose
`admin_region_read` (country / county for a lat/lon) among other caps — see
[`plugins.md`](plugins.md).

**Sources:** Outdoor Recreation Act / country outdoor-access codes cited in the
camping table; re-verify before production use.

### 1.3 What they share and where they differ

| Aspect | EC 561 truck rest | Right-to-roam camping |
|---|---|---|
| **Depends on location** | Yes (which driving-hours law applies) | Yes (which access/camping pack applies) |
| **Keyed rule pack** | Single EU/EEA-shaped pack today; multi-pack (EC 561 vs AETR vs other) is the natural extension | Explicit country table + sub-region gates |
| **Unknown jurisdiction** | Must not pretend local HGV law is EC 561 | **Decline** suggestion; never guess Norway |
| **Hard vs soft** | Hard planning constraints + history | Mostly hard *filters* for suggestions + lots of informational guidance |
| **Lives in** | **Core** (planning) | **Plugin** (advisory guest) |
| **Traceable sources** | Regulation articles | National outdoor-access law / codes |
| **Granularity** | Country / treaty family (EU/EEA/AETR) | Country **and** region (e.g. N. Norway) |

The general pattern below is this comparison turned into process: detect
jurisdiction → select a keyed pack → decline when unknown → place core vs
plugin by whether planning must hard-constrain → cite sources.

---

## 2. General pattern

### 2.1 Jurisdiction detection

A **candidate point** (current GPS position, a sample along a planned corridor,
an overnight / rest-stop / camping candidate) must resolve to a jurisdiction
using **administrative boundary data already available to the host/core**
(OSM admin boundaries / region extracts used elsewhere in Navi — the same class
of data the right-to-roam spec assumes via `admin_region_read`).

Do **not** introduce a new online geocoding dependency for this. Offline
admin-boundary lookup (country ISO code, and county / ISO region when needed)
is the intended path.

**Granularity is a first-class decision**, not an afterthought:

| Level | When to use | Example |
|---|---|---|
| **Country / treaty family** | Uniform national or multi-state legal regime | EC 561 vs AETR vs “not this law” |
| **Sub-national (county / region)** | Rules or guidance that apply only inside part of a country | Cloudberry note only in Nordland / Troms / Finnmark |

The pattern must support both. A country-only key is fine when the law is
uniform; do not assume country-level always suffices, and do not force every
rule to invent fake sub-national splits.

### 2.2 Rule-set structure

Model packs as a **keyed table** (conceptually: `ISO_country` or
`ISO_country + region` → parameter / behaviour struct), following the shape of
the right-to-roam country table:

| Key | Fields (examples) | Behaviour notes |
|---|---|---|
| `NO` | distance_m, duration_nights, … | Primary detailed pack |
| `SE` | … | Same shape, different values |
| `GB-SCT` or country=`GB` + region=`Scotland` | … | Sub-national or nation-within-UK as required |
| *(unlisted)* | — | **Decline / do not apply** (see fallback) |

Truck-style packs use the same idea even if today there is only one row
(“EC 561 / EEA-aligned”). Extending to AETR or US FMCSA Hours of Service means
**adding rows** (or parallel pack enums), not forking unrelated one-off code
paths.

### 2.3 Fallback / unknown jurisdiction (standing default)

For anything **safety- or legal-adjacent**:

> **Decline rather than guess.**

Do not apply Norway’s camping geometry to an unrecognized country. Do not apply
EC 561 numbers as if they were universal HGV law. Prefer an explicit
“cannot determine local rules / use designated facilities / this pack does not
apply here” outcome over a silent default pack.

This is the **standing default for the general pattern**, not a one-off for the
camping plugin. Only deviate when there is a documented, non-legal reason
(e.g. a pure UX preference with no safety claim) — and still document the
deviation.

### 2.4 Where rule sets live (core vs plugin)

| Question | Prefer |
|---|---|
| Must the rule **hard-constrain** routing, day budgets, break placement, or persisted duty history? | **Core** (like EC 561 truck rest) |
| Is the rule **advisory / suggestion / guidance text**, with the user free to ignore it? | **Plugin** (like right-to-roam camping) |
| Does it need host-only sensors or heavy optional data? | Usually **plugin**, capability-scoped |
| Shared numeric safety already used by core overnight filters? | Keep the constant in **core** (`SafetyConfig`); plugin reads it — do not fork |

Decision point for every new pack: **core if planning must obey it; plugin if
it informs.** Do not default everything to core “because GPS is involved,” and
do not put hard legal caps only in a plugin the planner never consults.

### 2.5 Sourcing requirement

Every jurisdiction pack that claims legal or safety-adjacent parameters must
cite **traceable sources** (regulation text, official summary, national outdoor
access code) the same way EC 561 and the allemannsretten table do. Navi must
not silently invent those numbers. Mark illustrative / welfare defaults
explicitly when they are **not** regulation-derived (see
[`horse-profile.md`](horse-profile.md) rest notes).

---

## 3. Adding a new jurisdiction-dependent rule set (checklist)

1. **Granularity** — Country/treaty only, or country + region? List the keys.
2. **Core vs plugin** — Hard planning constraint → core; advisory → plugin
   ([§2.4](#24-where-rule-sets-live-core-vs-plugin)).
3. **Rule-set structure** — Table/struct keyed by those jurisdictions; one
   shared shape, different values ([§2.2](#22-rule-set-structure)).
4. **Fallback** — Unknown / unsupported → **decline by default** unless a
   documented exception exists ([§2.3](#23-fallback--unknown-jurisdiction-standing-default)).
5. **Detection wiring** — Ensure admin-boundary resolution is available at the
   point of use (core planner query, or plugin `admin_region_read`). Add wiring
   only if missing — reuse existing region/PBF admin data where possible.
6. **Sources** — Cite official regulation / guidance for each pack row; note
   re-verification duty.
7. **Docs** — Link the new pack from README / this page / the relevant plugin
   or EC-style coverage doc; state enforced vs informational honestly.

---

## 4. Candidate future rule sets (list only)

Illustrative; **not implemented** in this pass:

- **Non-EU / non-AETR driving-hour regimes** — e.g. US FMCSA Hours of Service
  (structure differs from EC 561; would be a separate pack keyed by jurisdiction,
  not a silent rename of truck defaults).
- **National `maxspeed` fallback tables** — pre-departure ETA already uses
  highway-class fallbacks when OSM `maxspeed` is missing; per-country defaults
  (e.g. `NO:rural`) may need explicit tuning packs.
- **Additional right-to-roam-adjacent jurisdictions** — extend the camping table
  only with sources; until verified, treat like “uncertain → decline.”
- **Horse-specific right-of-access packs** — see
  [`horse-profile.md` §5](horse-profile.md#5-jurisdiction-dependent-horse-access);
  do **not** assume horse access equals foot access under allemannsretten /
  allemansrätten / CRoW without checking each jurisdiction.

---

## 5. Summary

1. Ground new location-dependent behaviour in the **EC 561** and
   **right-to-roam** precedents.
2. Resolve jurisdiction from **offline admin boundaries**; support country and
   sub-national keys.
3. Use a **keyed pack table**; **decline** when unknown for legal/safety claims.
4. Put **hard planning** rules in **core**, **advisory** rules in **plugins**.
5. Always **cite sources**; mark illustrative defaults honestly.
6. Use the **Horse** walkthrough ([`horse-profile.md`](horse-profile.md)) as a
   template when a new profile brings both routing reuse and a fresh
   jurisdiction pack candidate.
