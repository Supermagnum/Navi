# Online place search

Navi can send a place-name query to a Nominatim-compatible geocoder. This is
**off by default**. Offline search from installed regions is always available
and is not affected by the setting.

## Service

- Default base URL: `https://nominatim.openstreetmap.org`
- Usage policy: [Nominatim usage policy](https://operations.osmfoundation.org/policies/nominatim/)
- Attribution shown with online results: `Data © OpenStreetMap contributors`

The base URL is a Map settings field. It can be changed without a new build.
An empty value means no online calls.

The switch is in Map settings: tap the "Map" bar at the top, then scroll to
"Online place search". From the search screen, the "No offline match" line is a
link that opens Map settings scrolled to the switch.

## How the app complies

- The setting starts **off**. Nothing is sent until the user turns it on and
  explicitly submits a search (Search button or the keyboard Search action).
- Typing, auto-complete, periodic jobs, background work, and GPS / nearby
  lookups do not call the service.
- Two numeric parts that look like coordinates but are out of range are not
  sent (they are not a place name).
- Selecting a search result moves the map and offers From / Via / To. It does
  not change the Tools download region and does not set a via by itself.
- App-wide limiter: at most one request per second; a new submit while one is
  pending replaces it; identical queries in the same session are answered from
  a small cache. Errors, HTTP 429, and empty bodies show the offline list and
  are not retried automatically.
- Every request uses a User-Agent that names the app and version and gives the
  project page (`https://github.com/Supermagnum/Navi`). No stock library
  User-Agent.

## Shared limit

The public Nominatim instance applies its policy to **all users of this app
together**, not per device. A larger user base needs a self-hosted Nominatim
or another third-party instance. Put that URL in the setting.
