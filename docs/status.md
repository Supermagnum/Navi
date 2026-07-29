# Documentation status map (canonical sources)

Last updated: 2026-07-29.

Readers should not need to cross-reference half a dozen overlapping “status”
documents. Use this map. Point-in-time reports stay as historical evidence;
they are **not** the live product status.

## Where to look

| Question | Canonical doc | Notes |
|---|---|---|
| What is the current product / feature status? | [`../README.md`](../README.md) Features table + Known issues | Single live summary. Update here when status changes. |
| How do the parts fit together? | [`architecture.md`](architecture.md) | Design intent, not a rolling QA log. |
| Where do I edit code for X? | [`codebase-map.md`](codebase-map.md) | Contributor file map. |
| Host (Rust) integration evidence | [`test-results.md`](test-results.md) | Chronological evidence; supersede with newer dated sections. |
| Android / emulator instrumented evidence | [`android-test-results.md`](android-test-results.md) | Chronological evidence; later Items win over earlier contradictory rows. |
| Point-in-time closing / audit reports | e.g. [`closing-pass-report.md`](closing-pass-report.md), [`future-proofing-audit-2026-07.md`](future-proofing-audit-2026-07.md) | Keep for history; do **not** treat as live status unless the audit table is actively maintained. |
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

## Resolved example (2026-07-29)

`docs/android-test-results.md` Item 7 had toast-vs-attribution as
`confirmed-broken` while Item 8 recorded the same issue as fixed. The Item 7
row now points at Item 8 as the current status (**fixed and visually confirmed**).
