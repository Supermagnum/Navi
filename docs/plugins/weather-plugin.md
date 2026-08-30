# Weather plugin (`weather`)

**Status:** specification + **vendored icon assets** — guest WASM, host fetch
cache, and HUD integration are **not implemented**.

**Path:** `docs/plugins/weather-plugin.md`  
**Assets:** `plugins/weather/` (Meteocons SVG sets, MIT)  
**Architecture:** planned WASM guest on `plugin-host` with host-owned HTTPS
fetch and SQLite cache ([`plugins.md`](../plugins.md)). Icon lookup stays in the
host; guests map API codes to semantic slugs.

**System requirements** (all plugins): user **enable/disable** toggle; any device
link uses host-mediated **USB** / **Bluetooth**
([`plugins.md`](../plugins.md#enable--disable-required)).

Related: [`plugins.md`](../plugins.md) (weather idea #2),
[`weather-icons-reference.md`](weather-icons-reference.md) (human-readable icon catalog, fill style),
[`icons.md`](../icons.md) (Meteocons licensing index),
[`plugins/animated-icons-spec.md`](animated-icons-spec.md) (SMIL playback vs
Synfig frame packs),
[`plugins/safety-resupply.md`](safety-resupply.md) (optional `weather_read` for
WBGT / heat model).

---

## Current product state

| Layer | Status |
|---|---|
| Meteocons static SVGs | **Vendored** — `plugins/weather/icons/` |
| Meteocons animated SVGs (SMIL) | **Vendored** — `plugins/weather/animated-icons/` |
| `manifest.json` + MIT `LICENSE` | **Present** in both asset trees |
| `plugin.json` / guest `.wasm` | **Not present** |
| Host weather fetch / cache | **Not implemented** |
| HUD chips / map overlay | **Not implemented** |
| `weather_read` HostApi capability | **Not in ABI** |

APRS WX beacons (`b` / `t` / `h` keys) remain a separate radio-side path
([`APRS.md`](../APRS.md)). This plugin is the internet weather overlay.

---

## Purpose

| In scope | Out of scope |
|---|---|
| Current conditions and alerts along route (wind, precip, temp, pressure) | Replacing national met authority apps |
| HUD chips / small map overlays near position or corridor | Full-screen weather radar or satellite loops |
| Offline last-known cache after opt-in fetch | Silent background refresh without user consent |
| Icon display keyed by provider condition codes | Lottie / JSON animation packs (`@meteocons/lottie` is **not** vendored) |

**Providers:** see [Weather data providers](#weather-data-providers) (primary,
fallback, failover, and rate throttling). Weather Underground–class feeds only
where Terms of Use and API keys allow; keys stay in host secrets, never in WASM.

---

## Weather data providers

### Primary: MET Norway / Yr.no (Locationforecast 2.0)

| Aspect | Detail |
|---|---|
| Cost / access | **Free**, no API key or registration — funded by the Norwegian government; no billing tiers, no published daily request cap |
| License | **CC BY 4.0** — attribution + link to license + indicate changes required |
| Branding | The **“Yr”** brand name must **not** appear in Navi's UI or docs per their Terms of Service — refer to it generically as the weather provider |
| User-Agent | **Mandatory** on every request — must identify the app plus a contact email or URL; missing or generic User-Agents risk silent throttling or a permanent ban |

**Update frequency:**

| Product | Horizon / resolution | Refresh |
|---|---|---|
| Nordic/Arctic short-range (MEPS) | 0–60 h, **2.5 km** | **Once per hour** — strongest coverage; relevant for Norwegian Scenic Routes / off-road use |
| Medium-range (ECMWF 51-member ensemble) | 2–10 days, **~9 km** | **Twice per day** |
| Global (outside Nordic/Arctic) | Coarser | Less frequently refreshed than Nordic products |

**Traffic rules** (stricter than a simple daily cap — governs throttle design):

- Honor `Expires` and `If-Modified-Since` caching headers.
- Never poll on exact clock boundaries — **jitter required**.
- Truncate lat/lon to **4 decimals**.
- Mobile apps must **not** poll while not in active use.
- Push-style polling (e.g. alerts) capped at **once per 10 minutes**.
- Sustained traffic over **20 req/s** per application requires a special agreement with MET Norway.

### Fallback: Open-Meteo

| Aspect | Detail |
|---|---|
| When used | Yr.no unreachable, error response, or **HTTP 429** |
| Cost / access | **Free**, no API key; non-commercial use up to **10,000 requests/day** |
| License | **CC BY 4.0** — same attribution requirement as Yr.no |
| Self-host | Open-source server (**AGPLv3**), self-hostable — noted as a future option for fully offline/self-hosted deployment; **not** required for the initial plugin |

**Update frequency:**

| Model class | Refresh |
|---|---|
| Global models | Every **6 h** |
| High-resolution regional (ICON-D2, HRRR, AROME) | Every **1–3 h** |
| `best_match` | Auto-selects best model per location — cadence **varies by region** |

### Failover logic

| Condition | Provider |
|---|---|
| Fix inside Nordic/Arctic bounds | Attempt **Yr.no first** |
| Fix elsewhere | **Open-Meteo** as primary |
| Yr.no failure / timeout / **429** | Fall back to **Open-Meteo** regardless of location |

Label served data with **which provider** it came from in diagnostic / log output
(not required in the HUD itself).

### Rate throttling (conservative cellular data use)

The host-owned fetch layer (see [Proposed guest / host split](#proposed-guest--host-split))
must enforce a minimum interval between fetches, **independent** of either
provider's own rules, to keep cellular data usage low:

| Rule | Detail |
|---|---|
| Default interval | No more than once per **30–60 minutes** while the app is in **active use**, aligned with each provider's update cadence (no value polling Yr.no's Nordic model faster than its hourly refresh, or Open-Meteo regional models faster than 1–3 h) |
| Background | **No fetch** while the app is backgrounded / not in active use — consistent with [Offline behaviour](#offline-behaviour) |
| Jitter | Small random jitter on scheduled fetch times (per Yr.no traffic guidance; apply to **both** providers) |
| Manual refresh | User **“refresh now”** bypasses the interval but remains rate-limited (e.g. no more than once per **2–5 minutes**) |
| Cache-first | Serve **last-known cache** (labelled stale per Offline behaviour) when the throttle window has not elapsed, rather than blocking on a fresh fetch |

---

## Vendored Meteocons assets

Source: [Meteocons](https://github.com/basmilius/meteocons) by Bas Milius,
**MIT**. NPM packages used only as a download source — **not** project
dependencies:

| Package | Vendored into | Animation |
|---|---|---|
| `@meteocons/svg-static@0.1.0` | `plugins/weather/icons/` | None (plain SVG) |
| `@meteocons/svg@0.1.0` | `plugins/weather/animated-icons/` | SMIL (`<animate>`) |

Refresh: `scripts/vendor-meteocons.sh [svg-static-version] [svg-version]`
(default `0.1.0` / `0.1.0`).

### Directory layout

```text
plugins/weather/
  icons/
    manifest.json
    LICENSE
    fill/*.svg
    flat/*.svg
    line/*.svg
    monochrome/*.svg
  animated-icons/
    manifest.json
    LICENSE
    fill/*.svg
    flat/*.svg
    line/*.svg
    monochrome/*.svg
```

Each style folder is a complete parallel set. Pick one style at runtime (user
setting or host default — e.g. `fill` for HUD, `monochrome` for map pins).

**On-disk size (2026-08):** ~28 MiB total (~14 MiB static, ~15 MiB animated;
~19 MiB raw SVG bytes).

### Inventory (v0.1.0)

| Style | Static (`icons/`) | Animated (`animated-icons/`) |
|---|---|---|
| `fill/` | 519 | 475 |
| `flat/` | 519 | 475 |
| `line/` | 519 | 475 |
| `monochrome/` | 519 | 475 |
| **Total SVG files** | **2,076** | **1,900** |

The animated package ships fewer slugs than the static set. Host code must fall
back to the static icon (or `unknown`) when an animated slug is missing.

### `manifest.json`

Each asset tree includes an identical-schema manifest beside the style folders
(self-contained copy per tree; static and animated manifests differ in category
counts and `animated` flags).

Top-level fields:

- `styles` — `fill`, `flat`, `line`, `monochrome`
- `categories[]` — grouped icon metadata (`slug`, `name`, `animated`)

**Static categories (16):** `standard`, `mostly-clear`, `partly-cloudy`,
`overcast`, `extreme`, `thunderstorms`, `extreme-thunderstorms`, `alarms`,
`astronomical`, `miscellaneous`, `solar-and-power`, `thermometer`, `time`,
`pollen`, `uv`, `wind`.

**Animated categories (14):** same set minus `pollen` and `solar-and-power`.

Conceptual groups used by downstream mappers (not always top-level category
slugs):

| Group | Manifest location |
|---|---|
| Standard conditions | `standard`, `mostly-clear`, `partly-cloudy`, `overcast`, … |
| Thermometer | `thermometer` |
| Barometer | `miscellaneous` (`barometer`, `barometer-low`, …) |
| Wind | `wind` |
| Moon phases | `astronomical` (`moon-new`, `moon-full`, …) |
| UV index | `uv` (`uv-index`, `uv-index-1`, …) |
| Alerts | `alarms` |

Icon file path for slug `clear-day` and style `fill`:

```text
plugins/weather/icons/fill/clear-day.svg
plugins/weather/animated-icons/fill/clear-day.svg   # when animated slug exists
```

Entries may be `null` placeholders in the manifest array — skip them when
building lookup tables.

**Human-readable catalog:** [`weather-icons-reference.md`](weather-icons-reference.md)
lists every fill-style slug with a plain-language purpose. Regenerate after
manifest changes: `python3 scripts/generate-weather-icons-reference.py`.

### Runtime files only

Vendored trees contain **SVG**, `manifest.json`, and `LICENSE` only. No
`.ts`/`.d.ts`, `package.json`, Lottie JSON, or build tooling.

### Animated SVG (SMIL)

Animated Meteocons use inline SMIL inside a single `.svg` per slug. That is
**not** the Synfig frame-sequence model in
[`animated-icons-spec.md`](animated-icons-spec.md). A future weather host may:

1. Rasterize one frame from SMIL via a host SVG player, or
2. Show the static counterpart from `icons/` when reduce-motion is on or SMIL
   playback is unavailable.

Do not add a Synfig runtime to the routing core for weather motion.

---

## Proposed guest / host split

| Responsibility | Owner |
|---|---|
| HTTPS fetch (opt-in network), provider failover, **rate throttle**, SQLite cache | **Host** |
| Parse provider JSON → semantic slug + units | **Guest** (or host helper) |
| Select stations / grid cells near route or position | **Guest** |
| Resolve icon path from slug + style | **Host** |
| Render HUD chips / optional map overlay | **Host UI** |
| SMIL playback / reduce-motion | **Host** |

### Proposed capabilities

| Capability | Purpose |
|---|---|
| `position_read` | Current fix for nearby samples |
| `weather_read` (new) | Read cached samples near lat/lon / along corridor (throttled host cache; see [Rate throttling](#rate-throttling-conservative-cellular-data-use)) |
| `log` | Diagnostics |

Until `weather_read` exists, other specs (e.g.
[`safety-resupply.md`](safety-resupply.md)) may use manual WBGT / temperature
from settings.

---

## Offline behaviour

- Routing and core HUD must work with the weather plugin **disabled**.
- With the plugin enabled but offline: show **last-known cache** only; label
  stale data in UI.
- No silent background refresh — network fetch requires user opt-in consistent
  with Navi's offline-first rules ([`plugins.md`](../plugins.md)).

---

## Non-goals

- Vendoring or shipping `@meteocons/lottie`.
- Writing weather samples into OSM extracts or graph caches.
- Replacing APRS WX decode ([`APRS.md`](../APRS.md)).
- Embedding provider API keys in guest WASM.
- Fetch intervals **faster than the throttle floor** in [Rate throttling](#rate-throttling-conservative-cellular-data-use) (30–60 minutes active-use default), even if a future feature request asks for “real-time” weather.

---

## Implementation checklist (future)

1. Add `plugin.json` + guest scaffold under `plugins/weather/`.
2. Implement host fetch + SQLite cache + **provider failover and rate throttle**
   ([Weather data providers](#weather-data-providers)) + `weather_read` capability.
3. Map provider condition codes → Meteocons slugs via manifest (Yr.no primary,
   Open-Meteo fallback).
4. Host icon resolver: `{style}/{slug}.svg` with static fallback.
5. HUD chips + optional corridor overlay; stale-data labelling.
6. SMIL playback path + reduce-motion → static icon.
7. Wire optional `weather_read` into safety-resupply heat model.
8. Ship icons in Android assets or load from plugin tree when product plugin
   ships (after [wasmtime upgrade gate](../plugins.md#gate-upgrade-wasmtime-before-shipping-any-product-plugin)).

---

## See also

| Doc | Topic |
|---|---|
| [`plugins.md`](../plugins.md) | Plugin host, weather roadmap entry |
| [`weather-icons-reference.md`](weather-icons-reference.md) | Icon slug purposes (fill style) |
| [`icons.md`](../icons.md) | Meteocons MIT licensing index |
| [`animated-icons-spec.md`](animated-icons-spec.md) | Generic animated-icon plugin (Synfig frames) |
| [`safety-resupply.md`](safety-resupply.md) | Optional WBGT from weather feed |
| [`custom-alert-sounds-spec.md`](custom-alert-sounds-spec.md) | `weather_hazard` alert category (placeholder) |
