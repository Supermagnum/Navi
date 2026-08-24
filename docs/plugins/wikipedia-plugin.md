# Wikipedia plugin (deferred)
**Status: documentation only — not implemented.**
Same treatment as other deferred plugin specs: design first, code later.
This plugin lets a mapped place that has a known Wikipedia article surface
that article to the user with a single tap, using the device's browser
rather than an in-app renderer.
Related: [`plugins.md`](plugins.md) (plugin architecture overview).
---
## Purpose
| In scope | Out of scope |
|---|---|
| Detecting a Wikipedia link/reference for a place | Hosting or caching article content in-app |
| Opening the relevant article in the device's browser | Custom in-app article reader/renderer |
| Graceful behavior when no link exists | Editing or contributing to Wikipedia |
---
## User-facing behavior
1. A place icon on the map may carry an associated Wikipedia reference
   (e.g. a `wikipedia=*` / `wikidata=*`-style tag, sourced from the place's
   underlying map data).
2. When the user taps the place icon, if a Wikipedia reference is present,
   the plugin shows an option to open the article.
3. Selecting that option opens the relevant Wikipedia page in the device's
   default browser (not an embedded webview), consistent with Navi's
   external-link handling elsewhere.
4. If no Wikipedia reference exists for the place, no such option is shown —
   no error state, just absence of the action.
---
## Data source
- Place data is expected to carry a Wikipedia article reference (language +
  title, e.g. `en:Eiffel_Tower`) and/or a Wikidata QID, following the same
  convention OSM uses for `wikipedia=*` / `wikidata=*` tags.
- The plugin should prefer a Wikidata QID when present (language-independent,
  more stable) and fall back to a direct `wikipedia=*` language:title
  reference otherwise.
- No network call is required to *detect* whether a reference exists — that
  comes from already-loaded place data. The network call happens only when
  the user actively taps to open the article, consistent with Navi's rule
  that every network call is user-visible / consented.
---
## Open questions (future)
- URL construction: direct `https://<lang>.wikipedia.org/wiki/<Title>` vs.
  resolving a Wikidata QID to the user's preferred language edition at
  tap-time (requires a lookup call).
- Fallback language if the place's tagged language edition doesn't match the
  device/app locale.
- Whether disambiguation pages should be handled specially or just opened
  as-is.
---
## Implementation checklist (future)
1. Parse/store Wikipedia and Wikidata references from place data.
2. Place-icon tap UI: show "Wikipedia" action only when a reference exists.
3. URL construction logic (direct reference vs. Wikidata resolution).
4. Open via system browser intent (not embedded webview).
5. Locale fallback behavior.
