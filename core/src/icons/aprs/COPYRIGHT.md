# APRS symbol icons — copyright

## Summary (release / legal index)

| Local file | APRS code | Meaning | License | Provenance |
|---|---|---|---|---|
| `aprs_car.png` | `/>` | Car | **CC BY-SA 2.0** | Upstream [hessu/aprs-symbols](https://github.com/hessu/aprs-symbols) symbol marked *OH7LZB* (original vector by Heikki Hannikainen). |
| `aprs_digi.png` (+ `aprs_digi.svg`) | `/#` | Digipeater | **GPL-3.0-or-later** (Navi original) | Custom Navi artwork. Replaces upstream *VEC-OH7LZB* crop whose license was **Unknown**. |
| `aprs_house.png` (+ `aprs_house.svg`) | `/-` | House | **GPL-3.0-or-later** (Navi original) | Custom Navi artwork. Replaces upstream *VEC-OH7LZB* crop whose license was **Unknown**. |
| `aprs_human.png` (+ `aprs_human.svg`) | `/[` | Human | **GPL-3.0-or-later** (Navi original) | Custom Navi artwork. Replaces upstream *VEC-OH7LZB* crop whose license was **Unknown**. |

**No bundled symbol remains with Unknown licensing.** See also the top-level APRS section in [`docs/icons.md`](../../../../docs/icons.md).

## Upstream research (2026-07-29)

Source examined: [hessu/aprs-symbols COPYRIGHT.md](https://github.com/hessu/aprs-symbols/blob/master/COPYRIGHT.md).

Upstream shorthand:

- *VEC-OH7LZB* — vectorized by OH7LZB from the classic APRS bitmap set
  (WA8LMF / G4IDE / KH4G lineage). **Licensing: Unknown** (original
  designers not attributed with a redistributable license).
- *OH7LZB* — original vector by Heikki Hannikainen. **License: CC BY-SA 2.0**.

For `/#`, `/-`, and `/[` the upstream file still lists *VEC-OH7LZB* /
Unknown. No confident redistributable license was found in upstream history
or the WA8LMF bitmap notes. Per Navi’s release policy those three crops were
**removed** and replaced with original SVG → PNG artwork (same semantic keys
the instrumented moving-icon tests use).

Attribution for the retained car symbol: please keep a pointer to
https://github.com/hessu/aprs-symbols/ as requested upstream.
