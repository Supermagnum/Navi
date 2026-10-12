# Reduced LA5MR SPLAT talk coverage (CI fixture)

## Source

- Upstream: https://github.com/Supermagnum/splat `test-results/LA5MR.geojson.bz2`
- Local extract SHA-256 (`LA5MR.geojson.bz2`):
  `7d8c44eaa8cfb5dde5cb05b182c74a90b74adc0baf9bcd956f694dcdbb9a3607`
- Upstream git commit: use the splat repo tip that published that bz2 (oracle only)
- Licence: follow Supermagnum/splat project licence (coverage oracle for tests only;
  not a product input)

## Reduction method

1. Keep only features with `properties.level == "talk"`.
2. Keep a polygon if its outer-ring bbox (expanded by 25 km) intersects either
   Navi car-router corridor:
   - `testdata/cat/routes/espa_dombas_e6.polyline`
   - `testdata/cat/routes/espa_litjfossen_rv3_skansen.polyline`
3. Simplify rings with Douglas–Peucker (tolerance 0.005 deg, ~500 m).
4. Drop hear/noise levels and polygons far from both corridors.

Committed file: `LA5MR_talk_corridor_reduced.geojson` (~2.9 MB).

## Score parity

CI test `coverage_j_reduced_matches_full_within_tolerance` (package
`navi-plugin-cats`) compares this file vs `CAT_SPLAT_GEOJSON` (full extract)
when that env var is set.

Tolerance: ±2.0 percentage points and identical switch count when both runs see
≥50 any-talk samples.

Observed (2026-10-12, Navi polylines):

| Route | Reduced | Full (`CAT_SPLAT_GEOJSON`) |
|---|---|---|
| Espa→Dombås E6 | 100.0 %, 2 switches | 100.0 %, 2 switches |
| Espa→Litjfossen Rv 3 | 93.3 %, 3 switches | 92.9 %, 3 switches |
