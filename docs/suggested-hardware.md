# Recommended hardware units

Baseline reference: the author's own **XTRONS** Android head unit — 8-core, ~2 GHz, 4 GB RAM. This device is the actual reference already used throughout this project's emulator testing (the "xtrons" Automotive AVD profile matches it), and Navi is confirmed to run smoothly on it. Anything meeting or exceeding this spec should run at least as well; the units below were chosen specifically to meet or exceed it, not as an arbitrary spec sheet.

Sweet spot: 16 GB storage and up, 4 GB RAM minimum.

---

## Car-mounted: Android head units

| Brand | Fit vs. baseline | Notes |
|---|---|---|
| **XTRONS** (higher tier) | Matches or exceeds baseline directly | Same brand/family as the reference device; double-DIN and vertical formats, octa-core, 4–8 GB RAM options |
| **Atoto** | Exceeds baseline | Well-documented, good community support |
| **Joying** | Meets/exceeds baseline | Budget-friendly, PX5/PX6 chipsets |
| **Generic OEM aftermarket** (Newegg/Amazon-sourced) | Meets/exceeds baseline | 4 GB+64 GB is now market baseline; 8 GB+128 GB commonly available for modest extra cost |

**Important caveat before buying any of these**: most aftermarket head units ship with a locked-down Android build oriented around CarPlay/Android Auto mirroring and a fixed launcher. Since Navi needs to run as the primary app (not mirrored from a phone), verify a specific model supports sideloading APKs and replacing/disabling the default launcher before committing — this varies by firmware, not by spec sheet. Check XDA-Developers or the model's own community first.

**Distribution note**: Navi is currently tested via sideloaded builds during development. Once tested on real hardware (not just emulator), the app is planned for release through the **Google Play Store and/or F-Droid** — at that point, installation on supported devices won't require manual sideloading. Until then, the sideloading-support caveat above remains the practical requirement for anyone testing on a head unit today.

---

## Portable: rugged Android tablets (car-optional, solar-compatible)

For non-car-mounted use, the requirement is **USB-C PD charging input**, compatible with any off-the-shelf USB-C PD solar panel (e.g. Anker, BigBlue, EcoFlow foldable panels). Note: genuine built-in-solar-panel tablets are rare and not a realistic retail option — "solar charging support" in practice means USB-C PD compatibility with an external panel.

| Model | Fit vs. baseline | Notes |
|---|---|---|
| **Samsung Galaxy Tab Active 5 Pro** | Exceeds baseline | Snapdragon 7s Gen 3, up to 8 GB RAM, IP68, USB-C. Best build quality/known-quantity Android build of this list — least sideloading friction |
| **OUKITEL RT7 Titan** | Exceeds baseline | Octa-core, 12 GB+256 GB, 32,000 mAh battery, IP68/IP69K, USB-C. Large battery well-suited to multi-day hiking-profile use |
| **HOTWAV R7 / R8** | Meets/exceeds baseline | Octa-core, 4–6 GB RAM, IP68/IP69K, budget tier |
| **8849 Tank Pad** | Meets/exceeds baseline | Rugged, large battery, IP68, USB-C |

---

## Linux "pad" format — honest assessment

**No purchasable, off-the-shelf Linux tablet currently matches or exceeds the XTRONS baseline.** This is a real market gap, not a compromise to work around:

- **PineTab2** (Pine64) — the closest genuine, ready-to-use Linux tablet (Manjaro ARM/Debian pre-installed, no assembly required). Quad-core, lower clock than the baseline — a step *down* from the reference device, not comparable.
- No other retail Linux tablet in this class clears the baseline either.

**Conclusion**: for anyone wanting hardware that matches or improves on the current reference device, **Android is the only side of this comparison that currently delivers that**. Linux-tablet support remains a documented target platform (see the Linux build docs and gpsd/IMU integration work) for anyone who wants to run Navi there regardless of performance tier, but it should not be shopped for expecting an upgrade over the XTRONS baseline today.

---

## Summary

| Need | Recommendation |
|---|---|
| Car-mounted, matches/exceeds current unit | XTRONS (higher tier), Atoto, Joying, or generic octa-core aftermarket, 4–8 GB RAM |
| Portable, solar-chargeable, matches/exceeds current unit | Galaxy Tab Active 5 Pro (best build quality) or OUKITEL RT7 Titan (best battery) |
| Genuine Linux, ready to use, no assembly | PineTab2 — accept it will underperform the baseline |
| Genuine Linux, matching or exceeding the baseline | Not currently available as a retail product |
