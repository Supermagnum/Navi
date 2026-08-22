# Draft 362 plates — staging + generator

The 12 plates (5–105 every-5 gaps) are **integrated** into Navi production assets and the look-forward cone. This folder keeps the compositor and historical review notes.

See `docs/road-signs.md` in Navi for shipped coverage and provenance. Upstream contribution: `Supermagnum/road-signs` (`svg/speed_limit/362_{n}.svg` + catalogue JSON), marked as generated/derived.

Official Norwegian catalogue codes stop at `362.20` through `362.110`. There is no `362.5`, `362.10`, `362.15`, and so on in NVDB / the vendored JSON. Filenames follow Navi icon naming (`no_sign_362_20.svg`) so a later integration pass can drop them in without renaming.

## Generator tool

Script: [`generate_362.py`](generate_362.py) in this folder.

It composites digits from cached Wikimedia NPRA EPS outlines (and Riksvei 2 for digit **2**), applies Trafikkalfabetet V2.3a pairwise spacing, then scales the numeral group so it clears the inner red ring the same way as official 362.50 / 362.100.

### Requirements

- Python 3
- Network once, to fill `/tmp/no-362/` with Commons SVGs (or pass `--cache`)
- Optional: `rsvg-convert` for `--preview` PNGs

### Usage

```bash
cd new-signs

# Norwegian defaults: red ring, white face behind the numbers, black ink
python3 generate_362.py --speeds 35 45 55

# Yellow face (disc behind the numbers) — for temporary / alternate faces
python3 generate_362.py --speeds 50 --face yellow --out /tmp/yellow-plates

# Explicit colours
python3 generate_362.py --speeds 80 \
  --face '#f7d117' \
  --ring '#dd1800' \
  --ink '#010101' \
  --out /tmp/custom-plates \
  --preview
```

| Option | Default | Meaning |
|---|---|---|
| `--speeds` | (required) | One or more integer km/h values |
| `--out` | this directory | Where SVGs are written |
| `--cache` | `/tmp/no-362` | Cached Commons source SVGs |
| `--face` | `white` (`#ffffff`) | Inner disc colour **behind the numbers**. Presets: `white`, `yellow` (`#f7d117`). Or any `#rrggbb`. |
| `--ring` | `#dd1800` | Outer ring colour (Norwegian red by default) |
| `--ink` | `#010101` | Numeral fill |
| `--prefix` | `no_sign_362_` | Output filename prefix |
| `--preview` | off | Also write PNGs via `rsvg-convert` |

`--face yellow` only changes the disc colour in this compositor. It does **not** produce official German, UK, or US plate artwork. Digit outlines remain Norwegian Trafikkalfabetet / NPRA shapes.

### Other countries and mph (current policy)

For **Germany**, other national speed-limit designs, and any jurisdiction that posts limits in **miles per hour**, keep using **text-only fallbacks** for now. Do not ship these Norwegian 362 composites as stand-ins for foreign signs.

- German (and similar) national plates: text fallback until dedicated national assets exist.
- mph / imperial posted **plate artwork**: text fallback until dedicated mph
  plates exist (see TODO). HUD **display units** (mph numbers in chrome) are
  already in the Android app — this folder is only about SVG plate generation.

## TODO

- **mph plate artwork** (US/UK sign faces), not only km/h compositing and
  text fallbacks. Android HUD already formats speed/distance in mph/miles when
  the user picks US or UK units — that is separate from these Norwegian 362
  SVGs.

## Approved (quality)

First batch, signed off 2026-08-21:

| File | Value | Census justification |
|---|---|---|
| `no_sign_362_5.svg` | 5 km/h | Count 84 in the Østlandet maxspeed census — parking / very-low-speed |
| `no_sign_362_10.svg` | 10 km/h | Count 137 — shared zones / gatetun and private roads |

These two are still **staging-only**. Quality is approved; they are not wired into the look-forward cone or the icon pack.

## Second batch (draft, awaiting review)

Generated after the 5/10 sign-off, same plate construction. High-outlier census values previously flagged as likely data artifacts remain skipped.

| File | Value | Notes |
|---|---|---|
| `no_sign_362_15.svg` | 15 km/h | Two-digit; **1** from 362.100, **5** from 362.50 |
| `no_sign_362_25.svg` | 25 km/h | **2** from Wikimedia `Riksvei 2.svg` (Trafikkalfabetet Pro); **5** from 362.50 |
| `no_sign_362_65.svg` | 65 km/h | **6** from 362.60, **5** from 362.50 |
| `no_sign_362_75.svg` | 75 km/h | **7** from 362.70, **5** from 362.50 |
| `no_sign_362_85.svg` | 85 km/h | **8** from 362.80, **5** from 362.50 |
| `no_sign_362_95.svg` | 95 km/h | **9** from 362.90, **5** from 362.50 |
| `no_sign_362_105.svg` | 105 km/h | Three-digit scale matching 362.100; **1** and **0** from 362.100, **5** from 362.50 scaled down |

The first 25 draft used `Trafikkalfabetet_teksttegn.svg` path `rect1057`. That path is the letter **Z** on the character sheet (it sits with W/X/V/AE/O), not digit 2. There is no NPRA 362.20 EPS (`svg: null` upstream), so the 2 is taken from the Riksvei 2 vector instead. Trafikkalfabetet’s 2 is still a geometric open 2 (flat-ish top, diagonal, bottom bar), which is the typeface, not the letter Z.

Wide pairs (25, 65, 75, 85, 95) and 105 are uniformly scaled about the plate centre so sampled outlines stay inside the inner disc with the same clearance as official 362.50 / 362.100. 15 already fitted and is unscaled. **5** and **10** were not regenerated.

## Third batch (draft, awaiting review)

Every-5 gaps between official 30/40/50/60. Same compositing and ring-clearance fit as batch 2. Østlandet way counts from `ostlandet-latest.osm.pbf` (`maxspeed=` exact integer):

| File | Value | Census | Notes |
|---|---|---|---|
| `no_sign_362_35.svg` | 35 km/h | Count 4 | **3** from 362.30, **5** from 362.50; spacing 3–5 = 6/35 H |
| `no_sign_362_45.svg` | 45 km/h | Count 11 | **4** from 362.40 (outer + white counter), **5** from 362.50; spacing 4–5 = 6/35 H |
| `no_sign_362_55.svg` | 55 km/h | Count 12 | Both **5** from 362.50; spacing 5–5 = 6/35 H |

35 is sparse (4 ways) but present and not a nonsense outlier; 45 and 55 are modest but clear real usage. Staged only — same manual approval gate as prior batches.

## Coverage after this batch

Every-5 progression from **5 through 110** is now fully present in staging and/or the shipped catalogue:

- Staging drafts: 5, 10, 15, 25, 35, 45, 55, 65, 75, 85, 95, 105
- Official / catalogue: 20, 30, 40, 50, 60, 70, 80, 90, 100, 110

No further every-5 gap remains in that range. Values outside that set (or previously flagged high-outlier artifacts) are not in scope here.

## Sources consulted

Local clone: `/mnt/2e9a1e9f-2097-408c-ab9a-a01b32f11d28/github-projects/road-signs`

| Source | Use |
|---|---|
| `reference/trafikkalfabetet.pdf` (Statens vegvesen N300 annex) | Glyph drawings (V2.1c), digit widths (V2.2), pairwise spacing (V2.3a) |
| `reference/trafikkalfabetet.en.md` / `.en.pdf` | English wording of the same rules |
| Vendored Navi 362.30–362.110 plates | Colour (`#dd1800` / `#ffffff` / `#010101`) |
| NPRA EPS vectors (Wikimedia `NO road sign 362.{30,40,50,60,70,80,90,100}.svg`) | Digit outlines. Geonorge/Navi copies of 30–110 are raster traces and were not used as numeral paths |

Pair spacing (V2.3a, as fractions of capital height H): **1–5**, **2–5**, **9–5** = 7/35 H; **3–5**, **4–5**, **5–5**, **6–5**, **7–5**, **8–5** = 6/35 H; **1–0** = 8/35 H; **0–5** = 7/35 H.

## Plate construction

- Canvas / viewBox `0 0 200 200`, same as `no_sign_362_20.svg`.
- Red ring width = **1/8 of the outer diameter** (outer radius 100, inner radius 75).
- Two-digit cap-height taken from 362.50; three-digit cap-height from 362.100.
- Single-digit **5** uses two-digit cap-height, centered.
- After pairing, the whole numeral group is scaled down if any sampled outline point would sit closer to the inner red edge than official 362.50 (two-digit) or 362.100 (three-digit).

## Not done

- No edits to `core/src/routing/road_sign.rs`, `live_hazard.rs`, catalogue JSON, `core/src/icons/road-signs/`, `app/src/main/assets/icons/road-signs/`, or app code.
- 5 and 10 are quality-approved but not integrated; say so if they should be copied into the pack and added to the cone snap table.
- Imperial / mph **plate** generation (see TODO above). HUD display units are
  shipped in the Android app.
- Dedicated national plates for Germany and other jurisdictions (text-only fallback until then).
