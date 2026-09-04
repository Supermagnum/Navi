# NPRA DATEX II v3.1 client (`datex_npra`)

**Status:** user-facing specification for a planned host-owned DATEX pull client —
**not implemented** yet. Complements the broader traffic-source research in
[`traffic-information.md`](traffic-information.md) and the `road_info` idea in
[`plugins.md`](../plugins.md).

**Path:** `docs/plugins/datex-npra-client.md`  
**Node:** `https://datex-server-get-v3-1.atlas.vegvesen.no/`  
**Standard:** DATEX II **version 3.1** (CEN), NPRA publications as HTTPS pull
snapshots (XML).

This document describes how a Navi-side plugin/client **should** obtain,
authenticate to, and poll Statens vegvesen (Norwegian Public Roads
Administration, NPRA) DATEX publications. Where request/response details are
not fully verified against a live node or the NPRA DATEX II 3.1 PDF, they are
marked **[verify]**.

---

## Overview

The DATEX NPRA client pulls **road-related open data** from the NPRA DATEX II
v3.1 node and returns (or caches) DATEX II **XML** for use by higher layers
(e.g. a future `road_info` guest that turns situations into map banners /
reroute prompts).

| Publication (service name) | Typical content | Official HTTP GET snapshot |
|---|---|---|
| `GetSituation` | Roadworks, closures, accidents, weather-related hazards, other situations | `…/datexapi/GetSituation/pullsnapshotdata` |
| `GetTravelTimeData` | Measured travel times (seconds) on instrumented segments | `…/datexapi/GetTravelTimeData/pullsnapshotdata` |
| `GetMeasuredWeatherData` | Road-weather station measurements | `…/datexapi/GetMeasuredWeatherData/pullsnapshotdata` |
| `GetCCTVSiteTable` | Roadside CCTV site table (camera metadata / image references) | `…/datexapi/GetCCTVSiteTable/pullsnapshotdata` |

Base host for all of the above:

```text
https://datex-server-get-v3-1.atlas.vegvesen.no
```

**What it is for in Navi:** optional, user-enabled live traffic/weather/camera
context for Norway. It does **not** replace OSM basemap indexing, and it is
**not** a global traffic source (see [`traffic-information.md`](traffic-information.md)).

NPRA also publishes companion tables (e.g. weather site locations,
predefined travel-time locations). Those are out of scope for this client’s
default endpoint list unless explicitly enabled later.

---

## Requirements

| Item | Requirement |
|---|---|
| Network | Opt-in HTTPS to `datex-server-get-v3-1.atlas.vegvesen.no` |
| Credentials | NPRA-issued DATEX **username + password** (see [Obtaining Access](#obtaining-access)) |
| Platforms (intended) | Host-native path first (desktop / Android host), same pattern as other network plugins: WASM guests never hold secrets |
| Runtime deps (planned) | TLS HTTP client already used by the host; no SOAP stack required for the documented **HTTP GET** pull URLs |
| XML parsing (planned) | **`roxmltree`** (already in the Navi Rust workspace) for read-only DOM walks; alternatives such as `quick-xml` only if streaming becomes necessary **[verify when implementing]** |
| Storage | Local cache of last successful XML + `Last-Modified` (or equivalent) for conditional GET |

System requirements shared with all Navi plugins: user **enable/disable**
toggle; no silent background fetch without consent
([`plugins.md`](../plugins.md)).

---

## Obtaining Access

Access is **not self-service API-key signup**.

1. Open NPRA’s DATEX access request form:  
   [Get access to DATEX (Statens vegvesen)](https://www.vegvesen.no/en/fag/technology/open-data/a-selection-of-open-data/what-is-datex/get-access/)
2. Submit organisation / contact / purpose (and any IP/DNS fields the form
   requires).
3. NPRA registers you as a DATEX user and issues a **username and password**
   for the DATEX node (typically by email after review).

**Separate from ID-porten / “Your Page”.** The DATEX node credentials are for
machine pull of publications. They are **not** the same login as private
citizen services on vegvesen.no.

**Licence and cost.** Use is governed by the
[Norwegian Licence for Open Government Data (NLOD)](https://data.norge.no/nlod/en/2.0).
NPRA states that use of the DATEX service is **free of charge**. You must still
follow NPRA’s DATEX conditions (e.g. do not distort information; attribute NPRA
as source when redistributing; treat username/password as confidential — not for
end-user disclosure). See the conditions on the access form page.

---

## Configuring Login Credentials

### Where the client expects username / password

**Intended design (Navi host):**

| Priority | Mechanism | Notes |
|---|---|---|
| 1 (preferred) | Environment variables `NAV_DATEX_USERNAME` and `NAV_DATEX_PASSWORD` | CI, desktop, and operator-run tools |
| 2 | Host secrets file under app private storage (e.g. Android `filesDir` / desktop config dir), **mode 0600**, never in the git tree | Path example: `$NAVI_DATA_DIR/secrets/datex.env` or equivalent keystore entry |
| 3 | Interactive prompt once per session if neither is set and stdin is a TTY | Password not echoed; not written unless user opts into “save” |

**Not used as primary:** CLI flags such as `--password=…` (argv leakage in
`ps`, shell history, crash logs).

Example `datex.env` (local only — never commit):

```bash
NAV_DATEX_USERNAME=<DATEX_USERNAME>
NAV_DATEX_PASSWORD=<DATEX_PASSWORD>
```

### Auth on each pull

Credentials are sent as **HTTP Basic Authentication** on **every** HTTPS GET to
a protected publication URL:

```http
GET /datexapi/GetSituation/pullsnapshotdata HTTP/1.1
Host: datex-server-get-v3-1.atlas.vegvesen.no
Authorization: Basic <base64(username:password)>
Accept: application/xml
```

There is **no OAuth / ID-porten token** and **no refresh flow** for this node:
the password is static until NPRA reissues it.

**[verify]** Whether unauthenticated GETs ever return a useful body, a redirect,
or only 401; treat Basic Auth as required in all client code paths.

### Local storage and persistence

| Context | Persistence |
|---|---|
| Env vars only | Process lifetime (or shell/session profile) |
| Saved host secret | Persists across app restarts until the user clears it or rotates values |
| Interactive-only | Not persisted |

Android: store in app-private encrypted prefs / secrets store when “save” is
chosen — never in shared storage or logcat.

### Auth failure behaviour

| Outcome | Expected behaviour |
|---|---|
| HTTP **401** Unauthorized | Fail the pull; user-visible message: `DATEX authentication failed (401). Check NAV_DATEX_USERNAME / NAV_DATEX_PASSWORD.` |
| HTTP **403** Forbidden | Fail the pull; message: `DATEX access forbidden (403). Credentials may be revoked or IP-restricted — contact NPRA DATEX support.` |
| Missing credentials | Do not call the node; message: `DATEX credentials not configured.` |

**[verify]** Exact status codes and WWW-Authenticate headers against the live
node with deliberately wrong credentials.

### Updating / rotating credentials

1. Obtain new username/password from NPRA (re-request or support email such as
   `datex@vegvesen.no` as published on NPRA pages).
2. Replace `NAV_DATEX_USERNAME` / `NAV_DATEX_PASSWORD` (or overwrite the host
   secret file / prefs).
3. Restart the client / disable+enable the plugin so in-memory Basic Auth
   material is rebuilt.
4. Confirm with a single manual pull (see [Usage](#usage)).

NPRA conditions state that usernames and passwords are confidential and must
**not** be disclosed to end users of a redistributed product — store them in
operator/host config, not in end-user UI copy.

---

## Configuration

Example host config (TOML sketch — **[verify]** final schema when implemented):

```toml
[datex_npra]
enabled = false
base_url = "https://datex-server-get-v3-1.atlas.vegvesen.no"
# Prefer env for secrets; leave empty to use NAV_DATEX_* only.
username_env = "NAV_DATEX_USERNAME"
password_env = "NAV_DATEX_PASSWORD"

# Publications to poll (subset of NPRA v3.1 pull snapshots).
endpoints = [
  "GetSituation",
  "GetTravelTimeData",
  "GetMeasuredWeatherData",
  "GetCCTVSiteTable",
]

# Default poll interval when the plugin is enabled and network is allowed.
# NPRA update cadences differ (e.g. weather ~10 min, travel times ~5 min);
# do not poll faster than the publication’s own refresh without need.
poll_interval_secs = 300

# Conditional GET: send If-Modified-Since from last successful response’s
# Last-Modified (or Date) when available.
use_if_modified_since = true

# Optional User-Agent identifying the app + contact (good practice for open-data nodes).
user_agent = "Navi/<version> (DATEX client; <contact-url-or-email>)"
```

Resolved GET URL for each endpoint name `E`:

```text
{base_url}/datexapi/{E}/pullsnapshotdata
```

Examples:

```text
https://datex-server-get-v3-1.atlas.vegvesen.no/datexapi/GetSituation/pullsnapshotdata
https://datex-server-get-v3-1.atlas.vegvesen.no/datexapi/GetTravelTimeData/pullsnapshotdata
https://datex-server-get-v3-1.atlas.vegvesen.no/datexapi/GetMeasuredWeatherData/pullsnapshotdata
https://datex-server-get-v3-1.atlas.vegvesen.no/datexapi/GetCCTVSiteTable/pullsnapshotdata
```

### If-Modified-Since

NPRA documents that clients may send an `If-Modified-Since` header so unchanged
publications return **HTTP 304** instead of a full XML body. Recommended
client behaviour:

1. On **200**, store body + remember `Last-Modified` (or other validator NPRA
   returns — **[verify]** header name/format).
2. On next poll, send `If-Modified-Since: <stored value>`.
3. On **304**, keep the previous cached XML; do not treat as an error.

Add jitter to poll timers (same idea as MET Norway weather guidance) so many
clients do not align on exact clock boundaries.

---

## Usage

### Manual pull (curl)

```bash
export NAV_DATEX_USERNAME='<DATEX_USERNAME>'
export NAV_DATEX_PASSWORD='<DATEX_PASSWORD>'

curl -sS -u "${NAV_DATEX_USERNAME}:${NAV_DATEX_PASSWORD}" \
  -H 'Accept: application/xml' \
  -D - \
  'https://datex-server-get-v3-1.atlas.vegvesen.no/datexapi/GetSituation/pullsnapshotdata' \
  -o situation.xml
```

Conditional GET after a prior `Last-Modified`:

```bash
curl -sS -u "${NAV_DATEX_USERNAME}:${NAV_DATEX_PASSWORD}" \
  -H 'Accept: application/xml' \
  -H 'If-Modified-Since: <LAST_MODIFIED_FROM_PREVIOUS_RESPONSE>' \
  -D - \
  'https://datex-server-get-v3-1.atlas.vegvesen.no/datexapi/GetSituation/pullsnapshotdata' \
  -o situation.xml
```

### Planned host CLI / plugin call (sketch)

```bash
# Once implemented — names are illustrative.
navi datex pull --endpoint GetSituation --out /tmp/situation.xml
navi datex pull --endpoint GetTravelTimeData --out /tmp/travel-time.xml
```

### Sample XML shape (illustrative only)

DATEX II publications wrap model-specific payloads. Exact element names and
namespaces follow the NPRA 3.1 schemas / WSDL. The fragment below is **not** a
verbatim node dump — use NPRA sample data from
[git.vegvesen.no DATEX2 specifications](https://git.vegvesen.no/projects/DATEX2/repos/datex2-spesifications/browse)
for golden fixtures.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!-- [verify] namespaces, root element, and payload type per publication -->
<d2LogicalModel xmlns="http://datex2.eu/schema/3/d2Payload"
                modelBaseVersion="3"
                xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <exchange>
    <!-- supplier / exchange metadata -->
  </exchange>
  <payloadPublication xsi:type="SituationPublication" lang="en">
    <!-- situations, validity, location, … -->
  </payloadPublication>
</d2LogicalModel>
```

**[verify]** NPRA may use profile-specific namespaces or envelope wrappers;
always validate against the WSDL/XSD for the publication you call
(`…/datexapi/GetSituation?wsdl`, etc.).

---

## Troubleshooting

| Symptom | Likely cause | What to do |
|---|---|---|
| **401** | Wrong username/password, or credentials not sent | Re-check env/secret; confirm `Authorization: Basic …` on the wire (local debug only) |
| **403** | Account revoked, IP allow-list mismatch, or policy block | Re-read access email; update form IP/DNS if required; contact NPRA DATEX |
| **304** with empty body | Normal when `If-Modified-Since` matches | Keep previous XML; not a failure |
| **5xx** / timeouts | Node downtime or maintenance | Back off exponentially; NPRA corrects DATEX node issues on their ICT schedule |
| Huge XML / slow parse | Full national snapshot | Keep `roxmltree` (or stream) off the UI thread; filter after parse; do not poll faster than needed |
| TLS / DNS errors | Network policy, captive portal, offline mode | Fail closed; show clear offline/DATEX-unavailable status |
| “Works in curl, fails in app” | Missing User-Agent, wrong base URL, or credentials only in shell | Compare request headers; ensure app reads the same env/secret path |

Filters on situation publications (topic-limited pulls) are described in NPRA
DATEX documentation — **[verify]** query-parameter or SOAP-filter syntax before
documenting product filters in the UI.

---

## Security notes

- **Never commit** `<DATEX_USERNAME>` / `<DATEX_PASSWORD>` (or a filled
  `datex.env`) to version control. Add secret paths to `.gitignore`.
- Prefer **environment variables** or a **secrets manager** / OS keystore over
  plain files when available.
- Auth is **static HTTP Basic** — there is **no token expiry or refresh**. If
  credentials leak, **rotate manually** with NPRA and purge old secrets from
  devices, CI, and backups.
- Do not log full `Authorization` headers, password values, or complete XML
  dumps that may contain operationally sensitive detail at debug level in
  production builds.
- Per NPRA DATEX conditions: do not disclose node credentials to end users of a
  redistributed service; attribute NPRA when redistributing DATEX-derived
  information.

---

## Relationship to other docs

| Doc | Role |
|---|---|
| [`traffic-information.md`](traffic-information.md) | Why DATEX is national/curated, not a free global feed; RTL-SDR alternative research |
| [`plugins.md`](../plugins.md) § Road info | Product plugin sketch (`road_info`) that may consume DATEX-normalized incidents |
| NPRA DATEX landing | [What is DATEX](https://www.vegvesen.no/en/fag/technology/open-data/a-selection-of-open-data/what-is-datex/) |
| Access form | [Get access](https://www.vegvesen.no/en/fag/technology/open-data/a-selection-of-open-data/what-is-datex/get-access/) |
| Specs / samples | [DATEX2 specifications (git.vegvesen.no)](https://git.vegvesen.no/projects/DATEX2/repos/datex2-spesifications/browse) |
| NLOD | [data.norge.no NLOD](https://data.norge.no/nlod/en/2.0) |

---

## Open verification checklist

Before treating this client as production-ready, confirm against a live
credentialed node and the NPRA DATEX II 3.1 specification PDF:

1. Basic Auth required on all four snapshot URLs (status for anonymous GET).
2. `Last-Modified` / `If-Modified-Since` behaviour and 304 body emptiness.
3. Exact XML root, namespaces, and publication `xsi:type` per endpoint.
4. Whether SOAP POST is ever required for filtered pulls (HTTP GET snapshots
   are the default documented here).
5. Recommended minimum poll intervals per publication vs NPRA update cadence.
