# PR #161 emulator evidence — place-index compact (#155)

Captured: 2026-10-12 14:21:36Z  
Device: `emulator-5554` only (Navi_8c_4G_128G / sdk_gphone64_x86_64). Tablet SM-P613 not used.

Instrumented test: `PlaceIndexCompactEvidenceInstrumentedTest.emulator_compact_before_after_bak_and_defer`

## 1. BEFORE / AFTER (migrated-style freelist fixture)

| | bytes | page_count | freelist_count | freelist_ratio |
|--|------:|----------:|---------------:|---------------:|
| BEFORE | 950272 | 232 | 182 | 0.7845 |
| AFTER | 200704 | 49 | 0 | 0.0000 |

`delta_bytes=749568 delta_pages=183 delta_freelist=182`

Raw: `BEFORE bytes=950272 page_count=232 freelist_count=182 freelist_ratio=0.7845`  
`AFTER bytes=200704 page_count=49 freelist_count=0 freelist_ratio=0.0000`

## 2. Backup kept/dropped

live_counts={europe/norway/ostlandet=400, europe/norway/vestlandet=200}  
bak_counts={europe/norway/ostlandet=400, europe/norway/vestlandet=200}  
bak_decision_preview delete=true reason=counts_match  
bak_deleted=true reason=counts_match live_vs_bak_rows_equal=true  
compact_outcome=Done(compacted=true, bakDeleted=true, bakMismatches=[])

Decision: **bak deleted** (`counts_match`) — overlapping region row counts equal.

## 3. Defer log (SIMULATED low free space)

`freeBytes` stubbed to `10` so the free-space gate fires before `VACUUM INTO`.

Production-shaped log line:

```
deferred: insufficient_space free=10 need=67350528 live=241664
```

Full emit: `DEFER_LOG_LINE[PlaceIndexCompact] deferred: insufficient_space free=10 need=67350528 live=241664`

## Files

- [results.txt](results.txt)
- [logcat-place-index-compact.txt](logcat-place-index-compact.txt)
- [device.txt](device.txt)
