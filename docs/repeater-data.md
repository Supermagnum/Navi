# Repeater data folder (`repeater-data`)

Offline import of amateur repeater lists when no API is available (OpenRepeater /
RepeaterBook). Navi does **not** download these files from the network.

## Where the folder is

Navi creates `repeater-data` automatically on first CAT bootstrap, with user write
access for USB/MTP copies:

| Storage | Path |
|---------|------|
| **SD card present** | `Android/data/no.navi.app/files/repeater-data` on that volume |
| **No SD** | Internal app files: `…/files/repeater-data` |

Desktop / host default: `{app_data}/repeater-data`.

A short `README.txt` is written into the folder. The folder is removed when the
app is uninstalled.

## How to import OpenRepeater (KML)

1. In OpenRepeater, export the region or network as **KML** (KMZ is also accepted).
2. Copy the export into `repeater-data`. **Any file name is fine** — keep distinct
   names so you do not overwrite earlier exports (e.g. `Norway.kml`, `Sweden.kml`).
3. In Navi: enable the **CAT** plugin → open CAT tools → **Update repeaters**.

Networked systems often need **several** KML files, not one file per country.

## Policy notes

- This path is for **user file import only**. Server/OSM/RadioID updates use other
  channels (see [`CAT.md`](CAT.md)).
- You are responsible for complying with OpenRepeater / RepeaterBook terms.
- Local example KML trees at the repo root (`/repeater-data/`) are **gitignored**
  and must never be committed.
