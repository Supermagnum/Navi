# MWM client — later release plan (no action yet)

This document is the **future** release / cutover checklist for shipping the MWM
client that lands on `feat/mwm-client` (PR toward `dev`). It does **not** authorize
a version bump, tag, F-Droid metadata change, or server cutover today.

Rollback for one release after MWM ships: build or run with
`-Pnavi.mwmClient=false` / `NAVI_MWM_CLIENT_ENABLED=0` so the `.rkyv` path stays
intact.

## 1. Client release steps (when intentionally releasing)

1. **Version** — Bump `versionName` / `versionCode` in the Android app Gradle
   config (and any mirrored Crate / package metadata the release process uses).
2. **Changelog** — User-facing notes: MWM maps + offline search; one-tap
   migration from existing `.rkyv` downloads; flag-OFF rollback for one cycle.
3. **F-Droid metadata** — Update `fdroiddata` recipe / version / versionCode only
   when publishing that release (see existing F-Droid from-source docs). Do not
   change metadata on draft / merge-to-dev work.
4. **User migration notes** — Point at in-app Migrate flow (Wi-Fi default for
   large plans, pause/resume, confirm before deleting `.rkyv`). Map storage
   cleanup if the user declined delete.
5. **Known limitations to mention** — Vehicle dimensional limits not applied
   (design note only); boardwalk recall ~25%; long-route Joints latency ~12 s
   accepted on device.

## 2. Server `.rkyv` retirement criteria (Phase 8)

**Freeze ≠ retirement.** Keep serving `published/packs/` + `current.json` (and
related `.rkyv` trees) until request volume is near zero for several weeks after
the MWM client is what users actually run.

### Measure

On the Apache host whose DocumentRoot serves the packs (live:
`https://navigate-me.duckdns.org`):

```bash
# Rough daily count of .rkyv / classic pack hits (adjust log path / vhost).
# Prefer Combined or custom logs that include the request URI.
awk '
  $7 ~ /\.rkyv($|\?)/ || $7 ~ /\/packs\// || $7 ~ /current\.json/ {
    # optional: skip health probes / known scrapers
    d = substr($4, 2, 11)  # [17/Oct/2026:…
    gsub(/\//, "-", d)
    c[d]++
  }
  END { for (d in c) print d, c[d] }
' /var/log/apache2/access.log* | sort
```

Or a small script that tails / rotates daily into a CSV of
`date,rkyv_reqs,packs_reqs,current_json_reqs`. Track for **several consecutive
weeks** with totals near zero (owner threshold; suggest < a handful/day and no
upward trend).

### Then remove (only after the criterion holds)

1. Announce freeze window closed; confirm MWM catalog is the only supported path.
2. Stop publishing new `.rkyv` packs; leave existing objects readable during a
   short grace if needed.
3. Delete DocumentRoot trees for classic packs / region manifests when logs stay
   near zero.
4. Client: remove `.rkyv` UI / download path in a later release (not the first
   MWM ship).

## 3. References

- `docs/mwm-client-migration.md` — phases 1–8 overview
- `docs/mwm-phase7-migration.md` — migration E2E
- `docs/mwm-phase5-search.md` / `docs/mwm-phase6b-drape.md` — search + drape
- `docs/mwm-vehicle-limits-design.md` — limits not built
