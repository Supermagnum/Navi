# Documentation status map (canonical sources)

Last updated: 2026-08-28.

Readers should not need to cross-reference half a dozen overlapping “status”
documents. Use this map. Point-in-time reports stay as historical evidence;
they are **not** the live product status.

## Where to look

| Question | Canonical doc | Notes |
|---|---|---|
| What is the current product / feature status? | [`../README.md`](../README.md) Features table + Known issues | Single live summary. Update here when status changes. |
| How do I mark a map point / use Saved places? | [`map-marking-saved-places.md`](map-marking-saved-places.md) | User how-to (NO: [`kartmerking-lagrede-steder.md`](kartmerking-lagrede-steder.md)). |
| How do the parts fit together? | [`architecture.md`](architecture.md) | Design intent, not a rolling QA log. |
| Where do I edit code for X? | [`codebase-map.md`](codebase-map.md) | Contributor file map. |
| Host (Rust) integration evidence | [`test-results.md`](test-results.md) | Chronological evidence; supersede with newer dated sections. |
| Android / emulator instrumented evidence | [`android-test-results.md`](android-test-results.md) | Chronological evidence; later Items win over earlier contradictory rows. |
| Point-in-time closing / audit reports | e.g. [`closing-pass-report.md`](closing-pass-report.md), [`future-proofing-audit-2026-07.md`](future-proofing-audit-2026-07.md) | Keep for history; do **not** treat as live status unless the audit table is actively maintained. |
| Indexed map format evaluation (phased) | [`indexed-map-format-plan.md`](indexed-map-format-plan.md) | **Live** phase status for preprocess-once routing index work; update when phases complete. Includes pack-miss PBF priority / cone-skip evidence (2026-08-24). |
| Precomputed packs / town-to-town cache (direction) | [`precomputed-index-and-route-cache.md`](precomputed-index-and-route-cache.md) | **Not shipped.** Mirror (e.g. navi.app) of Navi packs; local convert as offline fallback; optional city-pair route cache. Contrast with commercial bundled DBs. |
| BRouter tiles as pack-miss speedup | [`brouter-pack-miss-investigation.md`](brouter-pack-miss-investigation.md) | **Phase 1 only (2026-08-27).** Gate not cleared — `.rd5` is not a Navi pack; no Phase 2 spec. |
| BRouter as alternate engine (car/bike miss) | [`brouter-engine-substitution-investigation.md`](brouter-engine-substitution-investigation.md) | **2026-08-27 (Gaps 1–2 closed on SM-P613).** AIDL cold ~1.3 s car / ~1.6 s bike vs ~54 s miss; warm holds/improves vs HTTP. **Car gate fail** (Friisvegen / no departure date). **Bike gate pass** → [`brouter-bike-aidl-fallback-spec.md`](brouter-bike-aidl-fallback-spec.md). |
| Plugin host vs product plugins | [`plugins.md`](plugins.md) + specs under `docs/plugins/` | Host is implemented (wasmtime **48.0.1**; deny ignores cleared; aarch64 Cranelift QEMU + Bionic on-device smoke on SM-P613). Product plugins remain spec-only. **Do not** link `plugin-host` into shipped binaries until the remaining steps in the [wasmtime upgrade gate](plugins.md#gate-upgrade-wasmtime-before-shipping-any-product-plugin) are done (source of truth in `plugins.md`). |
| Icon / APRS licensing summary | [`icons.md`](icons.md) | Asset-level `COPYRIGHT.md` remains authoritative per file; this is the release index. |
| Jurisdiction pack pattern | [`jurisdiction-rules.md`](jurisdiction-rules.md) | Pack selection detail in EC 561 / FMCSA docs. |

## Rules to avoid re-sprawl

1. **One live status surface** — README Features / Known issues. If a test or
   closing report changes product status, update README (or link from README to
   the dated evidence), do not invent a third summary doc.
2. **Evidence docs are append-only chronology** — when a later pass fixes an
   earlier “broken” row, either update that row in place with a pointer to the
   fix Item, or add an explicit supersession note. Never leave opposite statuses
   for the same claim without reconciliation.
3. **Audits stay tracked** — `docs/future-proofing-audit-*.md` action tables must
   get status + last-verified dates as work closes.
4. **Do not duplicate** architecture or codebase maps into status reports.

## Resolved (2026-08-28): Pixel UI multi-minute Espa→Atnbrufossen plans

The Pixel UI Plan-button times in the ~4–5 minute class (including 36 min / 193 s /
294 s / 314555 ms) were pack-miss PBF rebuilds: `RegionCoverage.resolvePlanPbf`
could pick `/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf` while pack
lookup used `pbf.parent()`, so the app-dir Ostlandet packs were never opened.
Pack lookup now takes an explicit app `data_dir`; the Plan button keeps the
native report (`pack_hit`, `ROUTE_PLAN_STAGES`) instead of replacing it with
`PLAN_MULTI`. Dump-test / `IndexedMapsBg` pack-hit (~13 s class) was already
correct.

**Follow-up (lower priority, not part of that fix):** `resolvePlanPbf` still
scores covering extracts by file size (`minBy`), so a smaller staged fixture
PBF can be chosen for geometry even when the app-dir copy is the one with packs.
Pack lookup is decoupled (explicit `data_dir`), so this is harmless for
correctness but confusing for which PBF path the UI plans against — consider
selection order / intent separately from pack-dir resolution.

## Resolved example (2026-07-29)

`docs/android-test-results.md` Item 7 had toast-vs-attribution as
`confirmed-broken` while Item 8 recorded the same issue as fixed. The Item 7
row now points at Item 8 as the current status (**fixed and visually confirmed**).
