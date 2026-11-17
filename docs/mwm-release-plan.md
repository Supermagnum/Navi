# MWM client — release plan (G3+)

This document tracks the MWM default cutover. Rollback for emergency builds
only: `-Pnavi.mwmClient=false` / `NAVI_MWM_CLIENT_ENABLED=0` (developer /
operator path — **not** a user-facing setting). There is **no** classic-maps
fallback toggle for end users; `.rkyv` is being retired.

## Gates

| Gate | Meaning | Status |
|------|---------|--------|
| G2 | Search parity (Everywhere context, per-query 21/21) + migration pause/resume | Met (PR #222) |
| G3 | First mainline release with MWM as default (`0.5.0-beta` / versionCode 35). Note: CAT tags `v0.4.0-beta` / `v0.4.1-beta` are separate CAT-line builds; an earlier G3 draft reused the `0.4.0-beta` / 34 name by mistake and was renumbered. | **Prepared** 2026-11-17 — pending tag / publish go-ahead |
| G4 | Client `.rkyv` removal (PR #221), keeping Phase 7 migration that reads the old on-disk `.rkyv` region list | **Partial merged** (#221): flag forced ON, flag-OFF CI removed. MapLibre/pack loaders deletion still open (next release / follow-up). Keep migration + thin `.rkyv` detection + FTS5. |
| G5 | Server `.rkyv` retirement after adoption near zero | Still required — users on `0.3.30-beta` keep hitting frozen packs until they update. Measure with `measure-rkyv-adoption.py` / weekly timer. |

## 1. Client release steps (G3)

1. **Version** — `versionName` `0.5.0-beta`, `versionCode` 35 in
   `app/build.gradle.kts` (and mirrored docs / Fastlane changelog).
   Do not reuse CAT tags `v0.4.0-beta` / `v0.4.1-beta`.
2. **Changelog** — User-facing notes: CoMaps-based maps + Navi layers; smaller
   downloads; contours + 3D; new search; explicit migration from existing
   `.rkyv` (Wi-Fi default; old maps removed only after confirmation); GPX
   import. **No** classic-maps fallback line.
3. **F-Droid metadata** — Update `fdroiddata` recipe / version / versionCode
   only when publishing (see `docs/RELEASING.md`). Network-use text must
   mention CoMaps mirrors + `navigate-me.duckdns.org`.
4. **User migration notes** — In-app Migrate flow (Wi-Fi default for large
   plans, pause/resume, confirm before deleting `.rkyv`).
5. **Known limitations** — Vehicle dimensional limits not applied on the new
   maps; boardwalk detection partial; long regional routes can take ~10 s;
   **no** user-facing classic fallback.

## 2. Server `.rkyv` retirement criteria (G5 / Phase 8 server)

**Freeze ≠ retirement.** Keep serving `published/packs/` + classic
`current.json` (and related `.rkyv` trees) until request volume is near zero
for several weeks after users actually run the MWM client.

### Measure

On the Apache host whose DocumentRoot serves the packs (live:
`https://navigate-me.duckdns.org`):

```bash
# Prefer the Phase 8 script (navi-server):
./scripts/phase8/measure-rkyv-adoption.py \
  --log-glob '/var/log/apache2/access.log*' \
  --out "$HOME/navi-rkyv-adoption"

# Or a rough daily count:
awk '
  $7 ~ /\.rkyv($|\?)/ || $7 ~ /\/packs\// || ($7 ~ /current\.json/ && $7 !~ /\/mwm\//) {
    d = substr($4, 2, 11)
    gsub(/\//, "-", d)
    c[d]++
  }
  END { for (d in c) print d, c[d] }
' /var/log/apache2/access.log* | sort
```

Install the weekly timer on **media** with
`/tmp/navi-rkyv-adoption-install.sh` (sudo; prepared by the G3 agent). Read the
weekly Markdown under the configured `NAVI_ADOPTION_OUT` directory.

### Then remove (only after the criterion holds)

1. Announce freeze window closed; confirm MWM catalog is the only supported path.
2. Stop publishing new `.rkyv` packs; leave existing objects readable during a
   short grace if needed.
3. Delete DocumentRoot trees for classic packs / region manifests when logs stay
   near zero.
4. Client: Phase 7 migration detection can shrink further once G5 is met and
   old installs are gone.

## 3. References

- `docs/mwm-client-migration.md` — phases 1–8 overview
- `docs/mwm-phase7-migration.md` — migration E2E
- `docs/mwm-phase5-search.md` / `docs/mwm-phase6b-drape.md` — search + drape
- `docs/mwm-vehicle-limits-design.md` — limits not built
- Draft PR #221 — client `.rkyv` retirement prep
