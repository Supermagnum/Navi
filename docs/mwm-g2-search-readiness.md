# G2 follow-up: search readiness, postcodes, latency

Branch: `fix/search-parity-g2` (PR #222). Device: Samsung `R52TB0JQEDE`, releasetest.

## Production readiness

`ComapsSearchBridge.enabled()` only means the MWM flag and native core are present.
The CoMaps search Engine is created lazily on the first successful `comapsSearchStart`.

**Fix:** `search` / `searchDetailed` wait until `comapsSessionIsOpen()` and a start returns a
non-zero request id (up to 90 s). They never treat a cold Engine as “no results”.
The UI already shows `search_in_progress` while `searchBusy` is true; partial CoMaps
batches update the hit list before the final merge.

Warm-up “Oslo” queries were removed from both suites. New test:
`coldStartImmediateLillehammer` (headless + in-app).

## Postcodes (`postcode_2609` / `postcode_2815`)

| Path | MWM root |
|------|----------|
| In-app | Production `{filesDir}/mwm` (`releases/*/leaves`) |
| Headless | Same production root when present (`used_production_leaves=true`) |

**Cause of earlier in-app miss:** Platform Context was set in the headless harness
(`OrganicMaps.nativeSetContext`) but not yet on the production search path before
drape init. Without Context, `m_needAddress` is false and `GetAddress()` stays empty,
so `expect_address_any: Lillehammer/Gjøvik` failed even though the same street
Feature hits were returned.

**Fix:** `ComapsSessionBootstrap` calls `nativeSetContext` before `comapsSessionOpen`
when the CoMaps SDK is packaged. In-app suite also calls `ComapsHeadlessPlatform.requireContext`.

After the fix both suites pass these rows. Bare postcode queries still often return
street **Feature** hits (not `Postcode` kind); locality in the address depends on
CoMaps. Fixture `non_gating_reason` documents that; they remain medium / non-gating
only as a data-shape note (currently both pass with Context).

## Latency (in-app releasetest, high queries)

`first_ms` = time to first non-empty CoMaps batch; `total_ms` = final batch.

| Query | first_ms | total_ms |
|-------|----------|----------|
| town_lillehammer | ~420–1400 | ~420–2400 |
| street_strandgata_hamar | ~550 | ~3100 |
| street_karl_johan_oslo | ~1900 | ~5500 |

**Karl Johan first batch > 1 s:** CoMaps Engine work across World + many leaves, not
Navi holding the UI for the final batch. Navi-side mitigation: stream partial batches
to the hit list. No further Navi-side cause found for the first-batch delay.

Artifacts: `test-artifacts/g2-followup/` (gitignored).
