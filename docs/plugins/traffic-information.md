# Traffic information sourcing (research)

**Status:** specification / research only — not implemented.  
**Path:** `docs/plugins/traffic-information.md`  
**Architecture:** informs the planned `road_info` plugin idea in
[`plugins.md`](../plugins.md); no guest WASM or HostApi work exists yet.
**System requirements** (if shipped as a plugin): user **enable/disable**
toggle; any RTL-SDR / tuner link uses host-mediated **USB**
([`plugins.md` — enable/disable](../plugins.md#enable--disable-required),
[USB/Bluetooth](../plugins.md#external-device-io--usb-and-bluetooth-required)).

Working title / id suggestion: none — this file records **source research** for
the existing `road_info` idea, not a separate product plugin id.

---

## Problem statement

No traffic data source is known today that is simultaneously:

| Requirement | Why it matters for Navi |
|---|---|
| **Free** (no per-request API bill at driving scale) | Offline-first product; network is opt-in |
| **Open** (inspectable terms, no opaque ToS trap) | Contributor-friendly, reproducible |
| **Global** (single integration covers any region a user might drive) | Geofabrik-style regions worldwide |
| **~1-minute cadence** (fresh enough for reroute prompts) | Incidents stale quickly on motorways |

Something always gives. The sections below map what each class of source
actually offers.

---

## Network / feed options (surveyed)

### DATEX II (CEN European standard)

Referenced in the [`road_info`](../plugins.md#3-road-info-road_info) plugin
idea as “DATEX-II style feeds.”

| Aspect | Finding |
|---|---|
| Coverage | **Europe-oriented** — national / regional road-authority feeds, not one global endpoint |
| Freshness | Typically **1–10 minutes** per country/feed (varies by operator) |
| Licensing | **Per feed** — terms differ; not a single open licence |
| Integration | Fragmented: each member state or agency exposes its own API or file drop |

**Conclusion:** DATEX II can work for **curated national** deployments but does
not solve “free + global + uniform” in one integration.

### Commercial global APIs (TomTom, HERE, …)

| Aspect | Finding |
|---|---|
| Coverage | **Global** |
| Freshness | Often **~1 minute** on paid tiers |
| Cost | **Paid** beyond small free / trial quotas — not sustainable as the default Navi path |

### Free / open crowdsourced wrappers (e.g. Waze-derived)

| Aspect | Finding |
|---|---|
| Coverage | Patchy; depends on third-party scraper |
| Freshness | Can be good where crowdsourcing is dense |
| Sustainability | Usually **rate-limited trials** or fragile ToS — not a dependable free source |

---

## Alternative under consideration — RTL-SDR decoding

Broadcast traffic services avoid recurring API fees but require **hardware**
(an RTL-SDR dongle or RDS-capable tuner) and only cover regions where over-the-
air traffic data is transmitted (mainly **Europe** for TMC/TPEG).

Same **host/guest split** as [`aprs_sdr`](../plugins.md#1-aprs-aprs--aprs_sdr):
the trusted host owns USB, IQ or audio capture, and decoding; a future guest (or
host-native service before the wasmtime gate) receives **normalized incident
records** only. Shares the RTL-SDR IQ pipeline documented in
[`APRS-SDR.md`](../APRS-SDR.md) and the [`rtl-sdr-rs`](https://crates.io/crates/rtl-sdr-rs)
crate dependency class.

### FM RDS-TMC (Traffic Message Channel)

| Aspect | Detail |
|---|---|
| Signal | Traffic events carried in **FM RDS** subcarriers on standard broadcast radio |
| Hardware | Cheap **RTL-SDR** dongle or dedicated RDS-capable tuner |
| Cost | **No service fee** — receive-only RF |
| Coverage | **Per broadcaster / per country** — not global |
| Freshness | **Real-time** broadcast (as fast as the station airs updates) |
| Open decoders (reference) | [RDS Surveyor](https://github.com/ChristopheJacquet/RdsSurveyor) — TMC decode with event-code interpretation; [RDSExpert-Plugin](https://github.com/wagnandr/RDSExpert-Plugin) — TMC decode plus 1500+ event code database |

### DAB / DAB+ TPEG

| Aspect | Detail |
|---|---|
| Signal | Traffic often carried as **TPEG** applications inside DAB/DAB+ multiplexes |
| Groundwork | [welle.io](https://www.welle.io/) / [welle-cli](https://github.com/snrupfel/welle-cli) — open-source DAB/DAB+ SDR decoder (RTL-SDR / Airspy) |
| Gap | **TPEG traffic extraction is not off-the-shelf** in welle today — would need a Navi-side or plugin-side layer on top of the multiplex decode |
| Coverage | Same regional limits as DAB rollout (strongest in Europe) |

### RTL-SDR path — trade-offs

| Pro | Con |
|---|---|
| Genuinely free at runtime (no API meter) | Extra **USB hardware** per vehicle / head unit |
| Real-time where broadcast exists | **Not global** — useless where TMC/TPEG is not on air |
| Reuses SDR host patterns from APRS research | Separate demux/decode stack from AFSK/AX.25 |
| Fits offline-first if decode is local | Antenna placement, multipath, and tuner lock are UX factors |

---

## Proposed capabilities (not in ABI yet)

Align with the existing `road_info` sketch in [`plugins.md`](../plugins.md):

| Proposed capability | Purpose |
|---|---|
| `incident_query` | Read normalized incidents near lat/lon / along corridor |
| `incident_write` | Upsert host-decoded or user-confirmed incidents (expiry + bbox) |
| `position_read` | Snap incidents to current fix |
| `log` | Diagnostics |
| `accessory_*` (USB) | Host opens RTL-SDR or tuner; guest never touches raw USB |

For an RTL-SDR TMC/TPEG plugin shaped like `aprs_sdr`, the host would also own
IQ capture (see [`APRS-SDR.md`](../APRS-SDR.md)) and emit incidents after
decode — the guest only filters, deduplicates, or formats HUD copy.

Core effect (unchanged from `road_info` idea): soft or hard edge penalties /
avoid flags during A* (future graph hook); map banners + route recalc prompt.

---

## Relationship to existing docs

| Doc | Link |
|---|---|
| `road_info` plugin idea | [`plugins.md` §3](../plugins.md#3-road-info-road_info) — this file explains why DATEX-II-style network feeds are hard to source **free and globally**, and records RTL-SDR as the leading **free** alternative |
| APRS SDR pipeline | [`APRS-SDR.md`](../APRS-SDR.md) — shared RTL-SDR IQ capture, host-owned USB, `rtl-sdr-rs` |
| APRS plugin split | [`plugins.md` §1](../plugins.md#1-aprs-aprs--aprs_sdr) — same host decodes / guest consumes pattern |

---

## Notes

| | |
|---|---|
| **Status** | Spec / research only — **not implemented** |
| **Product decision** | **None yet** on whether to pursue RTL-SDR TMC/TPEG decoding as a `road_info` backend or a sibling plugin |
| **Non-goals** | Implementing decoders, shipping rtl-sdr-rs in the APK, or changing `road_info` numbering / capabilities in `plugins.md` beyond cross-links |
