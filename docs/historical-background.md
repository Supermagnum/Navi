# Historical basis: rast and vei

**[Dokument på norsk](historisk-bakgrunn.md)**

The suggested default rest intervals for hiking (11.295 km main, 2.275 km
alternative; 40 km daily max) and for cycling (28.24 km main, 5.69 km
alternative; 100 km daily max) are inspired by the old Scandinavian units of
length *rast* and *vei*.

For cycling, the same rast/vei concept is used with distances scaled up. A
"rast" was the distance one traveled on foot before needing a rest ("rast,"
"pause," or the like); it corresponded to a *mil* and was often tied to the
length of the ell. The distance varied by region and over time. In the 900s a
rast was about 192 stone throws, divided into four quarters ("fjerdingvei"), and
corresponded to roughly 9,100.8 meters; in the 12th century it was expressed as
16,000 ells (four quarters of 8,000 feet) but remained in the same order of
magnitude.

A "dagsvei" (day's way/journey) was a traditional Scandinavian unit meaning
roughly how far you could walk in a day, commonly reckoned at about 40 km.

- A stone's throw was 120 ells (also called a "great hundred") — about 56.88 m
  (200 feet).
- There were 4 stone's throws in an arrow's flight, so about 480 ells —
  227.52 m (800 feet) around the year 900. Later in the Middle Ages, 10 arrow
  shots made up a fjerding of a mile — 2,275.2 m (8,000 feet), a quarter of a
  younger Norse mile.
- That younger Norse mile (rast / vei) was 9,100.8 m (32,000 feet). The same
  order of magnitude appears in 12th-century expressions such as 16,000 ells.
- A dagsvei (day's journey) was commonly ~40 km on foot.

Defaults follow that tradition:

| Mode | Main rest interval | Alternative rest interval | Suggested daily max |
|---|---|---|---|
| Hiking | 11.295 km | 2.275 km | 40 km |
| Cycling | 28.24 km | 5.69 km | 100 km |

Cycling uses the same rast/vei concept as hiking, with distances scaled up
(main ≈ 2.5× hiking main; alternative ≈ 2.5× hiking alternative; daily max
100 km vs 40 km).

These values are the app defaults for the **Hiking** and **Cycling** profiles
(`HIKING_*` / `CYCLING_*` constants in `core/src/config/defaults.rs`).
