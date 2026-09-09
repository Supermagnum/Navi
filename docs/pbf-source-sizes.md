# Geofabrik PBF source sizes vs Navi baked packs

Generated: **2026-09-09**.

## Methodology

This document compares two related but different measurements for each Geofabrik country extract:

1. **Geofabrik raw PBF** — the published `{path}-latest.osm.pbf` size shown on [download.geofabrik.de](https://download.geofabrik.de/) continent index pages (fetched 2026-09-09). Sizes are Geofabrik’s displayed figures (KB/MB/GB, often rounded). As of this date Geofabrik publishes a country-level `.osm.pbf` for every extract in the main continent tables, including US, China, France, Spain, Germany, Poland, Australia, Czech Republic, Canada, Netherlands, Russia, Japan, Indonesia, India, Norway, Brazil, Italy, and United Kingdom; those published country-file sizes are used (not sums of administrative subregion files, which slightly over-count shared border geometries). Special / overlapping composites listed under “Special Sub Regions” (Alps, DACH, Britain and Ireland, Great Britain, US Midwest/Northeast/South/West/Pacific) are excluded from country totals. Russia is counted once (path `russia`), filed under Europe. Antarctica is included in the global total and shown in its own short table.

2. **Navi baked packs** — byte sizes from the published pack catalog at `https://navigate-me.duckdns.org/current.json` (generation `20260909T014316Z-4181137-e437434e`). For countries published as multiple leaf packs (for example `europe/norway/*`, `asia/china/*`, `north-america/us/*`), leaf `bytes` values are summed. An exact country-level pack is used only when no child packs exist. Overlapping composite packs in the catalog (Alps, DACH, Britain and Ireland, Great Britain, US census-style regions, `africa/south-africa-and-lesotho`) are **not** folded into country totals.

Raw OSM extracts and Navi packs measure different artifacts: packs contain converted/indexed navigation data (graphs, place indexes, and related bake artifacts), so the ratio is an observed storage multiplier, not a compression ratio of the PBF itself.

## Summary

| Metric | Size |
| --- | ---: |
| Total Geofabrik country PBF (all continents + Antarctica, no composites) | 76.76 GB (82,421,850,830 bytes) |
| Total Navi packs matched to those countries | 549.00 GB (589,483,276,199 bytes) |
| Navi / Geofabrik ratio (country-matched) | **7.15×** |
| Sum of all `bytes` in `current.json` (includes composites + orphans) | 745.70 GB (800,687,272,113 bytes) |

Navi catalog: 540 region entries; Geofabrik country extracts accounted: 194.

## Europe

Continent subtotal — Geofabrik PBF: 33.75 GB; Navi packs: 186.98 GB.

| Country | Geofabrik PBF | Navi pack | ISO | Region path |
| --- | ---: | ---: | --- | --- |
| France | 4.70 GB | 21.73 GB | FR | `europe/france` |
| Germany | 4.50 GB | 24.04 GB | DE | `europe/germany` |
| Russian Federation | 3.90 GB | 21.29 GB | RU | `russia` |
| Italy | 2.10 GB | 12.35 GB | IT | `europe/italy` |
| United Kingdom | 2.10 GB | 9.91 GB | GB | `europe/united-kingdom` |
| Poland | 2.00 GB | 10.90 GB | PL | `europe/poland` |
| Spain | 1.40 GB | 11.86 GB | ES | `europe/spain` |
| Netherlands | 1.30 GB | 3.12 GB | NL | `europe/netherlands` |
| Norway | 1.30 GB | 4.82 GB | NO | `europe/norway` |
| Czech Republic | 902.0 MB | 3.38 GB | CZ | `europe/czech-republic` |
| Ukraine (with Crimea) | 835.0 MB | 5.73 GB | UA | `europe/ukraine` |
| Sweden | 778.0 MB | 4.93 GB | SE | `europe/sweden` |
| Austria | 771.0 MB | 4.51 GB | AT | `europe/austria` |
| Finland | 730.0 MB | 4.39 GB | FI | `europe/finland` |
| Belgium | 661.0 MB | 1.97 GB | BE | `europe/belgium` |
| Turkey | 615.0 MB | 7.00 GB | TR | `europe/turkey` |
| Switzerland | 520.0 MB | 3.13 GB | CH | `europe/switzerland` |
| Denmark | 471.0 MB | 2.36 GB | DK | `europe/denmark` |
| Portugal | 402.0 MB | 3.23 GB | PT | `europe/portugal` |
| Ireland and Northern Ireland | 393.0 MB | 2.63 GB | IE,GB | `europe/ireland-and-northern-ireland` |
| Belarus | 332.0 MB | 1.88 GB | BY | `europe/belarus` |
| Slovakia | 327.0 MB | 1.70 GB | SK | `europe/slovakia` |
| Greece | 325.0 MB | 3.67 GB | GR | `europe/greece` |
| Romania | 312.0 MB | 2.29 GB | RO | `europe/romania` |
| Hungary | 309.0 MB | 1.96 GB | HU | `europe/hungary` |
| Slovenia | 298.0 MB | 1.03 GB | SI | `europe/slovenia` |
| Serbia | 228.0 MB | 1.45 GB | RS | `europe/serbia` |
| Lithuania | 212.0 MB | 1.10 GB | LT | `europe/lithuania` |
| Croatia | 190.0 MB | 1.12 GB | HR | `europe/croatia` |
| Bulgaria | 165.0 MB | 1.38 GB | BG | `europe/bulgaria` |
| Bosnia-Herzegovina | 153.0 MB | 795.7 MB | BA | `europe/bosnia-herzegovina` |
| Latvia | 133.0 MB | 910.9 MB | LV | `europe/latvia` |
| Estonia | 117.0 MB | 686.3 MB | EE | `europe/estonia` |
| Georgia | 96.0 MB | 807.6 MB | GE | `europe/georgia` |
| Moldova | 96.0 MB | 721.0 MB | MD | `europe/moldova` |
| Iceland | 61.0 MB | 278.4 MB | IS | `europe/iceland` |
| Albania | 51.0 MB | 459.1 MB | AL | `europe/albania` |
| Luxembourg | 45.3 MB | 167.8 MB | LU | `europe/luxembourg` |
| Cyprus | 35.5 MB | 448.5 MB | CY | `europe/cyprus` |
| Montenegro | 32.8 MB | 225.9 MB | ME | `europe/montenegro` |
| Kosovo | 29.3 MB | 279.9 MB | XK | `europe/kosovo` |
| Macedonia | 28.3 MB | 239.1 MB | MK | `europe/macedonia` |
| Azores | 16.9 MB | 81.6 MB | PT | `europe/azores` |
| Malta | 8.5 MB | 50.2 MB | MT | `europe/malta` |
| Faroe Islands | 7.4 MB | 43.4 MB | FO | `europe/faroe-islands` |
| Isle of Man | 5.8 MB | 27.4 MB | IM | `europe/isle-of-man` |
| Guernsey and Jersey | 3.7 MB | 30.9 MB | GG,JE | `europe/guernsey-jersey` |
| Andorra | 3.3 MB | 13.6 MB | AD | `europe/andorra` |
| Liechtenstein | 3.3 MB | 18.1 MB | LI | `europe/liechtenstein` |
| Monaco | 675.0 KB | 2.9 MB | MC | `europe/monaco` |

## Africa

Continent subtotal — Geofabrik PBF: 7.31 GB; Navi packs: 59.43 GB.

| Country | Geofabrik PBF | Navi pack | ISO | Region path |
| --- | ---: | ---: | --- | --- |
| Nigeria | 675.0 MB | 5.06 GB | NG | `africa/nigeria` |
| Tanzania | 672.0 MB | 6.04 GB | TZ | `africa/tanzania` |
| South Africa | 400.0 MB | 5.10 GB | ZA | `africa/south-africa` |
| Congo (Democratic Republic/Kinshasa) | 397.0 MB | 1.93 GB | CD | `africa/congo-democratic-republic` |
| Madagascar | 370.0 MB | 2.56 GB | MG | `africa/madagascar` |
| Uganda | 353.0 MB | 1.88 GB | UG | `africa/uganda` |
| Kenya | 333.0 MB | 2.22 GB | KE | `africa/kenya` |
| Algeria | 285.0 MB | 1.92 GB | DZ | `africa/algeria` |
| Mozambique | 243.0 MB | 1.66 GB | MZ | `africa/mozambique` |
| Zambia | 240.0 MB | 1.47 GB | ZM | `africa/zambia` |
| Morocco | 232.0 MB | 2.84 GB | MA | `africa/morocco` |
| Cameroon | 212.0 MB | 952.3 MB | CM | `africa/cameroon` |
| Sudan | 194.0 MB | 1.66 GB | SD | `africa/sudan` |
| Zimbabwe | 171.0 MB | 889.5 MB | ZW | `africa/zimbabwe` |
| Egypt | 169.0 MB | 3.45 GB | EG | `africa/egypt` |
| Mali | 165.0 MB | 1.28 GB | ML | `africa/mali` |
| Somalia | 157.0 MB | 1.40 GB | SO | `africa/somalia` |
| Malawi | 147.0 MB | 1.26 GB | MW | `africa/malawi` |
| Ethiopia | 133.0 MB | 1.40 GB | ET | `africa/ethiopia` |
| South Sudan | 131.0 MB | 559.3 MB | SS | `africa/south-sudan` |
| Chad | 128.0 MB | 529.3 MB | TD | `africa/chad` |
| Lesotho | 120.0 MB | 325.7 MB | LS | `africa/lesotho` |
| Guinea | 112.0 MB | 877.7 MB | GN | `africa/guinea` |
| Ghana | 110.0 MB | 1.02 GB | GH | `africa/ghana` |
| Senegal and Gambia | 100.0 MB | 1.28 GB | SN,GM | `africa/senegal-and-gambia` |
| Central African Republic | 94.0 MB | 344.7 MB | CF | `africa/central-african-republic` |
| Botswana | 83.0 MB | 648.7 MB | BW | `africa/botswana` |
| Angola | 81.0 MB | 874.2 MB | AO | `africa/angola` |
| Ivory Coast | 81.0 MB | 659.9 MB | CI | `africa/ivory-coast` |
| Burkina Faso | 80.0 MB | 878.1 MB | BF | `africa/burkina-faso` |
| Tunisia | 80.0 MB | 1.18 GB | TN | `africa/tunisia` |
| Libya | 73.0 MB | 816.3 MB | LY | `africa/libya` |
| Niger | 73.0 MB | 693.7 MB | NE | `africa/niger` |
| Rwanda | 63.0 MB | 500.5 MB | RW | `africa/rwanda` |
| Togo | 59.0 MB | 357.2 MB | TG | `africa/togo` |
| Canary Islands | 56.0 MB | 360.0 MB | ES | `africa/canary-islands` |
| Namibia | 51.0 MB | 357.5 MB | NA | `africa/namibia` |
| Benin | 46.0 MB | 367.9 MB | BJ | `africa/benin` |
| Burundi | 44.1 MB | 407.9 MB | BI | `africa/burundi` |
| Sierra Leone | 44.0 MB | 266.7 MB | SL | `africa/sierra-leone` |
| Liberia | 35.6 MB | 183.4 MB | LR | `africa/liberia` |
| Congo (Republic/Brazzaville) | 31.1 MB | 186.9 MB | CG | `africa/congo-brazzaville` |
| Eritrea | 29.9 MB | 158.0 MB | ER | `africa/eritrea` |
| Swaziland | 29.2 MB | 172.4 MB | SZ | `africa/swaziland` |
| Mauritania | 29.1 MB | 243.3 MB | MR | `africa/mauritania` |
| Gabon | 24.3 MB | 152.4 MB | GA | `africa/gabon` |
| Cape Verde | 11.1 MB | 68.6 MB | CV | `africa/cape-verde` |
| Guinea-Bissau | 10.6 MB | 86.5 MB | GW | `africa/guinea-bissau` |
| Mauritius | 8.9 MB | 95.6 MB | MU | `africa/mauritius` |
| Djibouti | 6.7 MB | 33.3 MB | DJ | `africa/djibouti` |
| Equatorial Guinea | 6.2 MB | 35.0 MB | GQ | `africa/equatorial-guinea` |
| Comores | 3.8 MB | 14.7 MB | KM | `africa/comores` |
| Seychelles | 2.6 MB | 9.4 MB | SC | `africa/seychelles` |
| Sao Tome and Principe | 1.2 MB | 5.8 MB | ST | `africa/sao-tome-and-principe` |
| Saint Helena, Ascension, and Tristan da Cunha | 875.0 KB | 3.3 MB | SH | `africa/saint-helena-ascension-and-tristan-da-cunha` |

## Asia

Continent subtotal — Geofabrik PBF: 11.99 GB; Navi packs: 139.71 GB.

| Country | Geofabrik PBF | Navi pack | ISO | Region path |
| --- | ---: | ---: | --- | --- |
| Japan | 2.30 GB | 19.41 GB | JP | `asia/japan` |
| India | 1.60 GB | 28.50 GB | IN | `asia/india` |
| Indonesia (with East Timor) | 1.60 GB | 13.38 GB | ID,TL | `asia/indonesia` |
| China | 1.50 GB | 20.17 GB | CN | `asia/china` |
| Philippines | 577.0 MB | 3.42 GB | PH | `asia/philippines` |
| Nepal | 394.0 MB | 1.74 GB | NP | `asia/nepal` |
| Bangladesh | 337.0 MB | 1.60 GB | BD | `asia/bangladesh` |
| Vietnam | 312.0 MB | 7.13 GB | VN | `asia/vietnam` |
| Taiwan | 311.0 MB | 1.53 GB | TW | `asia/taiwan` |
| Thailand | 311.0 MB | 6.28 GB | TH | `asia/thailand` |
| South Korea | 273.0 MB | 3.77 GB | KR | `asia/south-korea` |
| Myanmar (a.k.a. Burma) | 268.0 MB | 2.90 GB | MM | `asia/myanmar` |
| GCC States | 240.0 MB | 3.60 GB | BH,KW,OM,QA,SA,AE | `asia/gcc-states` |
| Malaysia, Singapore, and Brunei | 238.0 MB | 3.62 GB | MY,SG,BN | `asia/malaysia-singapore-brunei` |
| Iran | 218.0 MB | 4.51 GB | IR | `asia/iran` |
| Kazakhstan | 212.0 MB | 1.66 GB | KZ | `asia/kazakhstan` |
| Pakistan | 148.0 MB | 3.47 GB | PK | `asia/pakistan` |
| Sri Lanka | 137.0 MB | 911.9 MB | LK | `asia/sri-lanka` |
| Uzbekistan | 118.0 MB | 1.42 GB | UZ | `asia/uzbekistan` |
| Israel and Palestine | 113.0 MB | 955.5 MB | IL,PS | `asia/israel-and-palestine` |
| Afghanistan | 107.0 MB | 1.03 GB | AF | `asia/afghanistan` |
| North Korea | 87.0 MB | 679.2 MB | KP | `asia/north-korea` |
| Iraq | 86.0 MB | 1.65 GB | IQ | `asia/iraq` |
| Syria | 77.0 MB | 951.8 MB | SY | `asia/syria` |
| Kyrgyzstan | 69.0 MB | 379.0 MB | KG | `asia/kyrgyzstan` |
| Mongolia | 59.0 MB | 446.8 MB | MN | `asia/mongolia` |
| Laos | 51.0 MB | 529.4 MB | LA | `asia/laos` |
| Armenia | 50.0 MB | 403.1 MB | AM | `asia/armenia` |
| Lebanon | 50.0 MB | 492.6 MB | LB | `asia/lebanon` |
| Tajikistan | 46.1 MB | 406.7 MB | TJ | `asia/tajikistan` |
| Azerbaijan | 43.9 MB | 910.3 MB | AZ | `asia/azerbaijan` |
| Yemen | 41.1 MB | 459.1 MB | YE | `asia/yemen` |
| Cambodia | 38.9 MB | 603.8 MB | KH | `asia/cambodia` |
| Jordan | 29.6 MB | 480.9 MB | JO | `asia/jordan` |
| Turkmenistan | 23.7 MB | 292.1 MB | TM | `asia/turkmenistan` |
| Bhutan | 22.5 MB | 140.2 MB | BT | `asia/bhutan` |
| East Timor | 16.9 MB | 75.8 MB | TL | `asia/east-timor` |
| Maldives | 5.0 MB | 30.3 MB | MV | `asia/maldives` |

## South America

Continent subtotal — Geofabrik PBF: 3.79 GB; Navi packs: 36.65 GB.

| Country | Geofabrik PBF | Navi pack | ISO | Region path |
| --- | ---: | ---: | --- | --- |
| Brazil | 1.90 GB | 19.63 GB | BR | `south-america/brazil` |
| Argentina | 409.0 MB | 3.62 GB | AR | `south-america/argentina` |
| Chile | 330.0 MB | 2.07 GB | CL | `south-america/chile` |
| Colombia | 313.0 MB | 2.77 GB | CO | `south-america/colombia` |
| Peru | 244.0 MB | 2.83 GB | PE | `south-america/peru` |
| Bolivia | 165.0 MB | 1.90 GB | BO | `south-america/bolivia` |
| Paraguay | 147.0 MB | 698.4 MB | PY | `south-america/paraguay` |
| Venezuela | 121.0 MB | 1.39 GB | VE | `south-america/venezuela` |
| Ecuador | 119.0 MB | 1.32 GB | EC | `south-america/ecuador` |
| Uruguay | 53.0 MB | 300.9 MB | UY | `south-america/uruguay` |
| Suriname | 20.4 MB | 83.0 MB | SR | `south-america/suriname` |
| Guyana | 14.8 MB | 69.1 MB | GY | `south-america/guyana` |

## Central America

Continent subtotal — Geofabrik PBF: 567.8 MB; Navi packs: 4.33 GB.

| Country | Geofabrik PBF | Navi pack | ISO | Region path |
| --- | ---: | ---: | --- | --- |
| Guatemala | 125.0 MB | 905.9 MB | GT | `central-america/guatemala` |
| Haiti and Dominican Republic | 84.0 MB | 784.6 MB | HT,DO | `central-america/haiti-and-domrep` |
| Honduras | 70.0 MB | 579.3 MB | HN | `central-america/honduras` |
| Cuba | 58.0 MB | 594.8 MB | CU | `central-america/cuba` |
| Nicaragua | 58.0 MB | 318.2 MB | NI | `central-america/nicaragua` |
| Costa Rica | 37.1 MB | 336.1 MB | CR | `central-america/costa-rica` |
| Jamaica | 36.8 MB | 173.1 MB | JM | `central-america/jamaica` |
| Panama | 34.5 MB | 252.2 MB | PA | `central-america/panama` |
| El Salvador | 33.4 MB | 310.2 MB | SV | `central-america/el-salvador` |
| Belize | 17.4 MB | 95.3 MB | BZ | `central-america/belize` |
| Bahamas | 13.6 MB | 81.7 MB | BS | `central-america/bahamas` |

## North America

Continent subtotal — Geofabrik PBF: 17.92 GB; Navi packs: 113.18 GB.

| Country | Geofabrik PBF | Navi pack | ISO | Region path |
| --- | ---: | ---: | --- | --- |
| United States of America | 11.30 GB | 93.49 GB | US | `north-america/us` |
| Canada | 6.00 GB | 9.70 GB | CA | `north-america/canada` |
| Mexico | 615.0 MB | 9.94 GB | MX | `north-america/mexico` |
| Greenland | 24.8 MB | 37.4 MB | GL | `north-america/greenland` |

## Australia/Oceania

Continent subtotal — Geofabrik PBF: 1.40 GB; Navi packs: 8.72 GB.

| Country | Geofabrik PBF | Navi pack | ISO | Region path |
| --- | ---: | ---: | --- | --- |
| Australia | 917.0 MB | 6.46 GB | AU | `australia-oceania/australia` |
| New Zealand | 384.0 MB | 1.50 GB | NZ | `australia-oceania/new-zealand` |
| Papua New Guinea | 51.0 MB | 337.2 MB | PG | `australia-oceania/papua-new-guinea` |
| Fiji | 16.4 MB | 93.3 MB | FJ | `australia-oceania/fiji` |
| Polynésie française (French Polynesia) | 14.9 MB | 68.2 MB | VU | `australia-oceania/polynesie-francaise` |
| New Caledonia | 13.5 MB | 116.2 MB | NC | `australia-oceania/new-caledonia` |
| Solomon Islands | 11.4 MB | 40.1 MB | SB | `australia-oceania/solomon-islands` |
| Vanuatu | 7.5 MB | 23.5 MB | VU | `australia-oceania/vanuatu` |
| American Oceania | 5.1 MB | 40.9 MB | VU | `australia-oceania/american-oceania` |
| Tonga | 3.5 MB | 13.8 MB | TO | `australia-oceania/tonga` |
| Samoa | 3.3 MB | 13.4 MB | WS | `australia-oceania/samoa` |
| Kiribati | 2.3 MB | 5.7 MB | KI | `australia-oceania/kiribati` |
| Marshall Islands | 2.1 MB | 4.7 MB | MH | `australia-oceania/marshall-islands` |
| Micronesia | 1.9 MB | 4.3 MB | FM | `australia-oceania/micronesia` |
| Cook Islands | 947.0 KB | 3.7 MB | CK | `australia-oceania/cook-islands` |
| Palau | 801.0 KB | 2.5 MB | PW | `australia-oceania/palau` |
| Wallis et Futuna | 603.0 KB | 1.7 MB | VU | `australia-oceania/wallis-et-futuna` |
| Niue | 414.0 KB | 2.0 MB | NU | `australia-oceania/niue` |
| Tuvalu | 356.0 KB | 781.6 KB | TV | `australia-oceania/tuvalu` |
| Nauru | 259.0 KB | 1006.5 KB | NR | `australia-oceania/nauru` |
| Tokelau | 141.0 KB | 137.1 KB | VU | `australia-oceania/tokelau` |
| Pitcairn Islands | 112.0 KB | 251.0 KB | MH | `australia-oceania/pitcairn-islands` |
| Île de Clipperton | 41.7 KB | — | FR | `australia-oceania/ile-de-clipperton` |

## Antarctica

| Country | Geofabrik PBF | Navi pack | ISO | Region path |
| --- | ---: | ---: | --- | --- |
| Antarctica | 31.6 MB | 6.3 MB | AQ | `antarctica` |

## Coverage gaps and mismatches

### Geofabrik extracts with no matching Navi pack

| Country | Geofabrik PBF | Region path |
| --- | ---: | --- |
| Île de Clipperton | 41.7 KB | `australia-oceania/ile-de-clipperton` |

### Navi catalog entries not attributed to a Geofabrik country total

Composite / overlapping packs (intentionally excluded from country sums because they duplicate geography already covered by country or leaf packs):

| Region id | Bytes |
| --- | ---: |
| `asia/sea` | 34.09 GB |
| `north-america/us-south` | 32.48 GB |
| `europe/dach` | 30.79 GB |
| `north-america/us-midwest` | 22.68 GB |
| `north-america/us-west` | 21.32 GB |
| `europe/britain-and-ireland` | 14.12 GB |
| `north-america/us-northeast` | 12.16 GB |
| `europe/alps` | 11.42 GB |
| `europe/great-britain` | 11.39 GB |
| `africa/south-africa-and-lesotho` | 5.38 GB |
| `north-america/us-pacific` | 486.3 MB |

Other unattributed Navi entries:

| Region id | Bytes | Notes |
| --- | ---: | --- |
| `hedmark` | 416.9 MB | Orphan top-level id (Hedmark is a Norway county; expected under `europe/norway/...`) |

Sweden is published in Navi as leaf packs under `europe/sweden/*` (county extracts); it is included in the Europe table above.

## Estimated Geofabrik PBF growth rate

Sample comparison of Geofabrik displayed `.osm.pbf` sizes between a Wayback Machine snapshot of the continent index pages on **2024-09-13** (`https://web.archive.org/web/20240913121843/https://download.geofabrik.de/{continent}.html`) and the live pages on **2026-09-09** (about 2.00 years). Annualized rate uses `(size_2026 / size_2024)^(1/2) - 1`.

Sample (15 extracts, mix of large/small and several continents):

| Country | Path | Sep 2024 | Sep 2026 | Total growth | Annualized |
| --- | --- | ---: | ---: | ---: | ---: |
| Germany | `europe/germany` | 4.10 GB | 4.50 GB | 9.8% | 4.8%/yr |
| France | `europe/france` | 4.30 GB | 4.70 GB | 9.3% | 4.5%/yr |
| Norway | `europe/norway` | 1.20 GB | 1.30 GB | 8.3% | 4.1%/yr |
| Iceland | `europe/iceland` | 58.0 MB | 61.0 MB | 5.2% | 2.6%/yr |
| Malta | `europe/malta` | 5.9 MB | 8.5 MB | 44.1% | 20.0%/yr |
| Japan | `asia/japan` | 1.90 GB | 2.30 GB | 21.1% | 10.0%/yr |
| India | `asia/india` | 1.40 GB | 1.60 GB | 14.3% | 6.9%/yr |
| Nepal | `asia/nepal` | 377.0 MB | 394.0 MB | 4.5% | 2.2%/yr |
| United States | `north-america/us` | 9.90 GB | 11.30 GB | 14.1% | 6.8%/yr |
| Canada | `north-america/canada` | 4.10 GB | 6.00 GB | 46.3% | 21.0%/yr |
| Mexico | `north-america/mexico` | 554.0 MB | 615.0 MB | 11.0% | 5.4%/yr |
| Nigeria | `africa/nigeria` | 590.0 MB | 675.0 MB | 14.4% | 7.0%/yr |
| Egypt | `africa/egypt` | 161.0 MB | 169.0 MB | 5.0% | 2.5%/yr |
| Brazil | `south-america/brazil` | 1.70 GB | 1.90 GB | 11.8% | 5.7%/yr |
| New Zealand | `australia-oceania/new-zealand` | 335.0 MB | 384.0 MB | 14.6% | 7.1%/yr |

- Mean annualized growth (all 15): **7.4% per year**
- Median annualized growth (all 15): **5.7% per year**
- Mean / median excluding sub-100 MB extracts (reduces display-rounding noise; n=13): **6.8% / 5.7% per year**
- Sample range: 2.2% … 21.0% per year

### Decade projection (illustrative)

Compounding the sample rates over **10 years** (`(1 + r)^10`):

| Assumed annualized rate | Multiplier over 10 years | 2026 country-PBF total (76.76 GB) → ~2036 |
| --- | ---: | ---: |
| 5.7%/yr (sample median) | **1.74×** | ~134 GB |
| 6.8%/yr (mean, sub-100 MB excluded) | **1.93×** | ~148 GB |
| 7.4%/yr (sample mean) | **2.04×** | ~156 GB |

If Navi country-matched packs stayed near today’s **7.15×** vs Geofabrik PBF, the same growth band would imply on the order of **~0.96–1.1 TB** of country-matched packs by ~2036 (from today’s 549 GB) — only if the pack/PBF ratio does not change. Treat both the PBF and pack decade figures as planning envelopes, not commitments.

**Caveat:** This is an estimate from a small convenience sample of displayed (rounded) Geofabrik sizes, not a precise global figure. Rounding especially exaggerates percentage change on small extracts (for example Malta). Canada’s large jump may mix real OSM growth with how Geofabrik rounds GB display values. Do not treat the mean or median as a forecast for every region. Growth need not stay constant for a decade.

---

*Sources: [Geofabrik download server](https://download.geofabrik.de/), [Navi `current.json`](https://navigate-me.duckdns.org/current.json) (generation 20260909T014316Z-4181137-e437434e), Wayback Machine snapshot 20240913121843.*
