# Pack coverage

Countries and subregions from the pack server catalog
[current.json](https://navigate-me.duckdns.org/current.json),
with country extract sizes from
[Geofabrik](https://download.geofabrik.de/).

- Catalog generation: `20260914T232004Z-942658-russia_northwestern_fed_district-23f468af`
- Region packs listed: **541**
- Layout: Geofabrik path (`continent/country/...`)
- **Country size:** Geofabrik `-latest.osm.pbf` (source OSM extract)
- **Subregion size:** Navi pack bytes from `current.json` (baked pack, not raw PBF)

## Why some countries have no subregions

**Subregions: none** means there are no nested **download extracts / packs**, not
that the country lacks provinces, states, or other administrative divisions.

### Example: Afghanistan

OpenStreetMap maps Afghanistan’s provinces (Taginfo lists **35** distinct
`ISO3166-2` values such as `AF-KAB`, `AF-HER`, …). Geofabrik still only
publishes one country extract
([afghanistan.html](https://download.geofabrik.de/asia/afghanistan.html)), and
the pack server therefore only lists `asia/afghanistan`. The country is **not
currently split** into admin-region extracts for download.

### Pack server vs Geofabrik vs OpenStreetMap

For every pack-catalog country with **Subregions: none** (**185** of **204**):

| Check | Result | Count |
|---|---|---|
| Pack server nested packs | none | **185** |
| Geofabrik [index-v1.json](https://download.geofabrik.de/index-v1.json) sub-extracts under that country | none | **185** (0 exceptions) |
| OpenStreetMap `ISO3166-2` admin subdivision codes ([Taginfo](https://taginfo.openstreetmap.org/keys/ISO3166-2)) | present | **163** |
| Has a Geofabrik ISO code, but Taginfo shows **no** `ISO3166-2` values | — | **5** (Antarctica / AQ, Cook Islands, New Caledonia, Niue, Faroe Islands) |
| No ISO3166-1 on the Geofabrik feature (composite / multi-country extracts) | — | **17** (e.g. Alps, DACH, Britain and Ireland, US Midwest, …) |

So Afghanistan’s pattern is the **normal** case for unsplit countries: OSM has
(or can have) administrative boundaries inside the country file, but neither
Geofabrik nor the pack host currently ships those admins as separate downloads.
Small extracts (few roads / little mapped data) are especially likely to stay
single-pack; larger ones can stay unsplit simply because nested leaves were
never published.

Where Geofabrik **does** publish subfolders (Germany, France, China, and so
on), this catalog already lists matching pack-server leaves.

Other notes:

- **Subregions appear only when nested packs exist** in `current.json` (for
  example `europe/germany/bayern/...`).
- **Missing from this doc means missing from the live catalog.** A path not
  listed here is not in this `current.json` snapshot (Navi may still download
  the Geofabrik PBF via local bake).
- OSM comparison uses Taginfo’s `ISO3166-2` key values (prefix before `-`), not
  a full Overpass polygon census. A few territories may still have
  `boundary=administrative` ways/relations without `ISO3166-2` tags.

Country-level pack **yes** means the bare country path itself is a
downloadable pack. **no (subregions only)** means you download nested
leaves, not a single whole-country pack.

Display names use Geofabrik’s local spelling where available (including
Norwegian Æ/Ø/Å and other diacritics). Pack path ids stay ASCII
(`ostlandet`, `sorlandet`, …).

**Norway / Hedmark:** the pack server still lists `europe/norway/hedmark`,
but that area is covered by **Østlandet**; this doc omits Hedmark as a
separate subregion (same rule as the app region chips).

## Table of contents

Sizes after names are Geofabrik PBFs; nested sizes are pack-server packs.
Entries that are **not countries** (multi-country or regional Geofabrik
extracts) include a short explanation on the same line.

- [Afghanistan](#asia-afghanistan) — 107 MB
- [Albania](#europe-albania) — 51.5 MB
- [Algeria](#africa-algeria) — 286 MB
- [Alps](#europe-alps) — 2.16 GB — regional extract: Alpine mountain area across several countries
- [American Oceania](#australia-oceania-american-oceania) — 5.12 MB — regional extract: US-affiliated Pacific islands
- [Andorra](#europe-andorra) — 3.31 MB
- [Angola](#africa-angola) — 81.3 MB
- [Antarctica](#antarctica) — 31.6 MB
- [Argentina](#south-america-argentina) — 410 MB
- [Armenia](#asia-armenia) — 50.7 MB
- [Australia](#australia-oceania-australia) — 918 MB
  - [Australian Capital Territory](#australia-oceania-australia-act) — 133 MB
  - [Christmas Island](#australia-oceania-australia-christmas-island) — 1.15 MB
  - [Cocos (Keeling) Islands](#australia-oceania-australia-cocos-islands) — 423 KB
  - [Coral Sea Islands](#australia-oceania-australia-coral-sea-islands) — 6.42 KB
  - [New South Wales (with ACT and JBT)](#australia-oceania-australia-new-south-wales) — 1.91 GB
  - [Norfolk Island](#australia-oceania-australia-norfolk-island) — 1.16 MB
  - [Northern Territory](#australia-oceania-australia-northern-territory) — 147 MB
  - [Queensland](#australia-oceania-australia-queensland) — 1.21 GB
  - [South Australia](#australia-oceania-australia-south-australia) — 628 MB
  - [Tasmania](#australia-oceania-australia-tasmania) — 241 MB
  - [Victoria](#australia-oceania-australia-victoria) — 2.07 GB
  - [Western Australia](#australia-oceania-australia-western-australia) — 967 MB
- [Austria](#europe-austria) — 773 MB
- [Azerbaijan](#asia-azerbaijan) — 44.0 MB
- [Azores](#europe-azores) — 16.9 MB — Portuguese Atlantic autonomous region (not a country)
- [Bahamas](#central-america-bahamas) — 13.6 MB
- [Bangladesh](#asia-bangladesh) — 338 MB
- [Belarus](#europe-belarus) — 333 MB
- [Belgium](#europe-belgium) — 662 MB
- [Belize](#central-america-belize) — 17.4 MB
- [Benin](#africa-benin) — 46.0 MB
- [Bhutan](#asia-bhutan) — 22.5 MB
- [Bolivia](#south-america-bolivia) — 165 MB
- [Bosnia-Herzegovina](#europe-bosnia-herzegovina) — 153 MB
- [Botswana](#africa-botswana) — 83.9 MB
- [Brazil](#south-america-brazil) — 1.94 GB
  - [Centro-Oeste](#south-america-brazil-centro-oeste) — 1.83 GB
  - [Nordeste](#south-america-brazil-nordeste) — 5.81 GB
  - [Norte](#south-america-brazil-norte) — 1.35 GB
  - [Sudeste](#south-america-brazil-sudeste) — 6.87 GB
  - [Sul](#south-america-brazil-sul) — 3.77 GB
- [Britain and Ireland](#europe-britain-and-ireland) — 2.43 GB — multi-country extract: Great Britain + Ireland
- [Bulgaria](#europe-bulgaria) — 166 MB
- [Burkina Faso](#africa-burkina-faso) — 80.7 MB
- [Burundi](#africa-burundi) — 44.1 MB
- [Cambodia](#asia-cambodia) — 39.0 MB
- [Cameroon](#africa-cameroon) — 213 MB
- [Canada](#north-america-canada) — 6.01 GB
  - [Alberta](#north-america-canada-alberta) — 1.53 GB
  - [British Columbia](#north-america-canada-british-columbia)
    - [Interior Administrative Region](#north-america-canada-british-columbia-interior-admreg) — 183 MB
    - [Island Administrative Region](#north-america-canada-british-columbia-island-admreg) — 361 MB
    - [Kootenay Administrative Region](#north-america-canada-british-columbia-kootenay-admreg) — 145 MB
    - [North Administrative Region](#north-america-canada-british-columbia-north-admreg) — 264 MB
    - [Okanagan Administrative Region](#north-america-canada-british-columbia-okanagan-admreg) — 202 MB
    - [South Coast Administrative Region](#north-america-canada-british-columbia-southcoast-admreg) — 612 MB
  - [Manitoba](#north-america-canada-manitoba) — 686 MB
  - [New Brunswick](#north-america-canada-new-brunswick) — 252 MB
  - [Newfoundland and Labrador](#north-america-canada-newfoundland-and-labrador) — 243 MB
  - [Northwest Territories](#north-america-canada-northwest-territories) — 112 MB
  - [Nova Scotia](#north-america-canada-nova-scotia) — 429 MB
  - [Nunavut](#north-america-canada-nunavut)
    - [Kitikmeot Region](#north-america-canada-nunavut-kitikmeot) — 51.6 MB
    - [Kivalliq Region](#north-america-canada-nunavut-kivalliq) — 63.4 MB
    - [Qikiqtaaluk Region](#north-america-canada-nunavut-qikiqtaaluk) — 98.3 MB
  - [Ontario](#north-america-canada-ontario) — 3.13 GB
  - [Prince Edward Island](#north-america-canada-prince-edward-island) — 58.8 MB
  - [Quebec](#north-america-canada-quebec) — 1.91 GB
  - [Saskatchewan](#north-america-canada-saskatchewan) — 578 MB
  - [Yukon](#north-america-canada-yukon) — 65.3 MB
- [Canary Islands](#africa-canary-islands) — 57.0 MB — Spanish autonomous community (listed under Africa on Geofabrik)
- [Cape Verde](#africa-cape-verde) — 11.1 MB
- [Central African Republic](#africa-central-african-republic) — 94.8 MB
- [Chad](#africa-chad) — 129 MB
- [Chile](#south-america-chile) — 331 MB
- [China](#asia-china) — 1.48 GB
  - [Anhui](#asia-china-anhui) — 543 MB
  - [Beijing](#asia-china-beijing) — 576 MB
  - [Chongqing](#asia-china-chongqing) — 403 MB
  - [Fujian](#asia-china-fujian) — 679 MB
  - [Gansu](#asia-china-gansu) — 789 MB
  - [Guangdong (with Hong Kong and Macau)](#asia-china-guangdong) — 1.94 GB
  - [Guangxi](#asia-china-guangxi) — 675 MB
  - [Guizhou](#asia-china-guizhou) — 397 MB
  - [Hainan](#asia-china-hainan) — 148 MB
  - [Hebei (with Beijing and Tianjin)](#asia-china-hebei) — 1.99 GB
  - [Heilongjiang](#asia-china-heilongjiang) — 484 MB
  - [Henan](#asia-china-henan) — 980 MB
  - [Hong Kong](#asia-china-hong-kong) — 186 MB
  - [Hubei](#asia-china-hubei) — 726 MB
  - [Hunan](#asia-china-hunan) — 670 MB
  - [Inner Mongolia](#asia-china-inner-mongolia) — 567 MB
  - [Jiangsu](#asia-china-jiangsu) — 1.21 GB
  - [Jiangxi](#asia-china-jiangxi) — 528 MB
  - [Jilin](#asia-china-jilin) — 445 MB
  - [Liaoning](#asia-china-liaoning) — 397 MB
  - [Macau](#asia-china-macau) — 12.5 MB
  - [Ningxia](#asia-china-ningxia) — 122 MB
  - [Qinghai](#asia-china-qinghai) — 219 MB
  - [Shaanxi](#asia-china-shaanxi) — 569 MB
  - [Shandong](#asia-china-shandong) — 1.53 GB
  - [Shanghai](#asia-china-shanghai) — 297 MB
  - [Shanxi](#asia-china-shanxi) — 498 MB
  - [Sichuan](#asia-china-sichuan) — 1.47 GB
  - [Tianjin](#asia-china-tianjin) — 283 MB
  - [Tibet](#asia-china-tibet) — 439 MB
  - [Xinjiang](#asia-china-xinjiang) — 584 MB
  - [Yunnan](#asia-china-yunnan) — 1.38 GB
  - [Zhejiang](#asia-china-zhejiang) — 1.16 GB
- [Colombia](#south-america-colombia) — 314 MB
- [Comores](#africa-comores) — 3.79 MB — Comoros islands extract
- [Congo (Democratic Republic/Kinshasa)](#africa-congo-democratic-republic) — 397 MB
- [Congo (Republic/Brazzaville)](#africa-congo-brazzaville) — 31.1 MB
- [Cook Islands](#australia-oceania-cook-islands) — 950 KB
- [Costa Rica](#central-america-costa-rica) — 37.2 MB
- [Croatia](#europe-croatia) — 190 MB
- [Cuba](#central-america-cuba) — 59.1 MB
- [Cyprus](#europe-cyprus) — 35.6 MB
- [Czech Republic](#europe-czech-republic) — 903 MB
  - [Jihočeský kraj](#europe-czech-republic-jihocesky) — 330 MB
  - [Jihomoravský kraj](#europe-czech-republic-jihomoravsky) — 372 MB
  - [Karlovarský kraj](#europe-czech-republic-karlovarsky) — 107 MB
  - [Královéhradecký kraj](#europe-czech-republic-kralovehradecky) — 198 MB
  - [Liberecký kraj](#europe-czech-republic-liberecky) — 160 MB
  - [Moravskoslezský kraj](#europe-czech-republic-moravskoslezky) — 267 MB
  - [Olomoucký kraj](#europe-czech-republic-olomoucky) — 219 MB
  - [Pardubický kraj](#europe-czech-republic-pardubicky) — 187 MB
  - [Plzeňský kraj](#europe-czech-republic-plzensky) — 254 MB
  - [Praha](#europe-czech-republic-praha) — 182 MB
  - [Středočeský kraj (with Praha)](#europe-czech-republic-stredocesky) — 875 MB
  - [Ústecký kraj](#europe-czech-republic-ustecky) — 392 MB
  - [Kraj Vysočina](#europe-czech-republic-vysocina) — 210 MB
  - [Zlínský kraj](#europe-czech-republic-zlinsky) — 189 MB
- [DACH](#europe-dach) — 5.80 GB — multi-country extract: Germany + Austria + Switzerland
- [Denmark](#europe-denmark) — 471 MB
- [Djibouti](#africa-djibouti) — 6.69 MB
- [East Timor](#asia-east-timor) — 16.9 MB
- [Ecuador](#south-america-ecuador) — 120 MB
- [Egypt](#africa-egypt) — 170 MB
- [El Salvador](#central-america-el-salvador) — 33.4 MB
- [Equatorial Guinea](#africa-equatorial-guinea) — 6.23 MB
- [Eritrea](#africa-eritrea) — 30.0 MB
- [Estonia](#europe-estonia) — 117 MB
- [Ethiopia](#africa-ethiopia) — 133 MB
- [Faroe Islands](#europe-faroe-islands) — 7.37 MB
- [Fiji](#australia-oceania-fiji) — 16.4 MB
- [Finland](#europe-finland) — 730 MB
- [France](#europe-france) — 4.73 GB
  - [Alsace](#europe-france-alsace) — 614 MB
  - [Aquitaine](#europe-france-aquitaine) — 1.56 GB
  - [Auvergne](#europe-france-auvergne) — 947 MB
  - [Basse-Normandie](#europe-france-basse-normandie) — 747 MB
  - [Bourgogne](#europe-france-bourgogne) — 949 MB
  - [Bretagne](#europe-france-bretagne) — 1.53 GB
  - [Centre](#europe-france-centre) — 1.16 GB
  - [Champagne Ardenne](#europe-france-champagne-ardenne) — 632 MB
  - [Corse](#europe-france-corse) — 162 MB
  - [Franche Comte](#europe-france-franche-comte) — 652 MB
  - [Guadeloupe](#europe-france-guadeloupe) — 95.0 MB
  - [Guyane](#europe-france-guyane) — 58.0 MB
  - [Haute-Normandie](#europe-france-haute-normandie) — 521 MB
  - [Ile-de-France](#europe-france-ile-de-france) — 1.31 GB
  - [Languedoc-Roussillon](#europe-france-languedoc-roussillon) — 1.48 GB
  - [Limousin](#europe-france-limousin) — 519 MB
  - [Lorraine](#europe-france-lorraine) — 901 MB
  - [Martinique](#europe-france-martinique) — 85.0 MB
  - [Mayotte](#europe-france-mayotte) — 31.1 MB
  - [Midi-Pyrenees](#europe-france-midi-pyrenees) — 1.86 GB
  - [Nord-Pas-de-Calais](#europe-france-nord-pas-de-calais) — 876 MB
  - [Pays de la Loire](#europe-france-pays-de-la-loire) — 1.59 GB
  - [Picardie](#europe-france-picardie) — 604 MB
  - [Poitou-Charentes](#europe-france-poitou-charentes) — 1.00 GB
  - [Provence Alpes-Cote-d'Azur](#europe-france-provence-alpes-cote-d-azur) — 1.81 GB
  - [Reunion](#europe-france-reunion) — 133 MB
  - [Rhone-Alpes](#europe-france-rhone-alpes) — 2.78 GB
- [Gabon](#africa-gabon) — 24.3 MB
- [GCC States](#asia-gcc-states) — 241 MB — multi-country extract: Gulf Cooperation Council (Bahrain, Kuwait, Oman, Qatar, UAE)
- [Georgia](#europe-georgia) — 97.0 MB
- [Germany](#europe-germany) — 4.51 GB
  - [Baden-Württemberg](#europe-germany-baden-wuerttemberg)
    - [Freiburg Regbez](#europe-germany-baden-wuerttemberg-freiburg-regbez) — 1013 MB
    - [Karlsruhe Regbez](#europe-germany-baden-wuerttemberg-karlsruhe-regbez) — 934 MB
    - [Stuttgart Regbez](#europe-germany-baden-wuerttemberg-stuttgart-regbez) — 1.27 GB
    - [Tübingen Regbez](#europe-germany-baden-wuerttemberg-tuebingen-regbez) — 826 MB
  - [Bayern](#europe-germany-bayern)
    - [Mittelfranken](#europe-germany-bayern-mittelfranken) — 555 MB
    - [Niederbayern](#europe-germany-bayern-niederbayern) — 640 MB
    - [Oberbayern](#europe-germany-bayern-oberbayern) — 1.66 GB
    - [Oberfranken](#europe-germany-bayern-oberfranken) — 508 MB
    - [Oberpfalz](#europe-germany-bayern-oberpfalz) — 668 MB
    - [Schwaben](#europe-germany-bayern-schwaben) — 763 MB
    - [Unterfranken](#europe-germany-bayern-unterfranken) — 734 MB
  - [Berlin](#europe-germany-berlin) — 478 MB
  - [Brandenburg (mit Berlin)](#europe-germany-brandenburg) — 1.57 GB
  - [Bremen](#europe-germany-bremen) — 85.2 MB
  - [Hamburg](#europe-germany-hamburg) — 230 MB
  - [Hessen](#europe-germany-hessen) — 2.19 GB
  - [Mecklenburg-Vorpommern](#europe-germany-mecklenburg-vorpommern) — 620 MB
  - [Niedersachsen (mit Bremen)](#europe-germany-niedersachsen) — 2.76 GB
  - [Nordrhein-Westfalen](#europe-germany-nordrhein-westfalen)
    - [Arnsberg Regbez](#europe-germany-nordrhein-westfalen-arnsberg-regbez) — 958 MB
    - [Detmold Regbez](#europe-germany-nordrhein-westfalen-detmold-regbez) — 762 MB
    - [Düsseldorf Regbez](#europe-germany-nordrhein-westfalen-duesseldorf-regbez) — 861 MB
    - [Köln Regbez](#europe-germany-nordrhein-westfalen-koeln-regbez) — 893 MB
    - [Münster Regbez](#europe-germany-nordrhein-westfalen-muenster-regbez) — 583 MB
  - [Rheinland-Pfalz](#europe-germany-rheinland-pfalz) — 1.68 GB
  - [Saarland](#europe-germany-saarland) — 220 MB
  - [Sachsen](#europe-germany-sachsen) — 1.57 GB
  - [Sachsen-Anhalt](#europe-germany-sachsen-anhalt) — 909 MB
  - [Schleswig-Holstein](#europe-germany-schleswig-holstein) — 803 MB
  - [Thüringen](#europe-germany-thueringen) — 1.01 GB
- [Ghana](#africa-ghana) — 110 MB
- [Great Britain](#europe-great-britain) — 2.02 GB — regional extract: England, Scotland, and Wales (not the full UK)
- [Greece](#europe-greece) — 325 MB
- [Greenland](#north-america-greenland) — 24.9 MB
- [Guatemala](#central-america-guatemala) — 125 MB
- [Guernsey and Jersey](#europe-guernsey-jersey) — 3.71 MB — Channel Islands extract (Guernsey + Jersey)
- [Guinea](#africa-guinea) — 113 MB
- [Guinea-Bissau](#africa-guinea-bissau) — 10.6 MB
- [Guyana](#south-america-guyana) — 14.8 MB
- [Haiti and Dominican Republic](#central-america-haiti-and-domrep) — 84.5 MB — multi-country extract: Hispaniola
- [Honduras](#central-america-honduras) — 70.6 MB
- [Hungary](#europe-hungary) — 309 MB
- [Iceland](#europe-iceland) — 61.7 MB
- [India](#asia-india) — 1.59 GB
  - [Central Zone](#asia-india-central-zone) — 6.58 GB
  - [Eastern Zone](#asia-india-eastern-zone) — 4.86 GB
  - [North-Eastern Zone](#asia-india-north-eastern-zone) — 1.17 GB
  - [Northern Zone](#asia-india-northern-zone) — 5.65 GB
  - [Southern Zone](#asia-india-southern-zone) — 9.58 GB
  - [Western Zone](#asia-india-western-zone) — 4.23 GB
- [Indonesia (with East Timor)](#asia-indonesia) — 1.62 GB
  - [Java](#asia-indonesia-java) — 7.60 GB
  - [Kalimantan](#asia-indonesia-kalimantan) — 1.65 GB
  - [Maluku](#asia-indonesia-maluku) — 177 MB
  - [Nusa-Tenggara](#asia-indonesia-nusa-tenggara) — 1019 MB
  - [Papua](#asia-indonesia-papua) — 247 MB
  - [Sulawesi](#asia-indonesia-sulawesi) — 1.18 GB
  - [Sumatra](#asia-indonesia-sumatra) — 3.10 GB
- [Iran](#asia-iran) — 219 MB
- [Iraq](#asia-iraq) — 86.2 MB
- [Ireland and Northern Ireland](#europe-ireland-and-northern-ireland) — 394 MB
- [Isle of Man](#europe-isle-of-man) — 5.78 MB — Crown dependency extract
- [Israel and Palestine](#asia-israel-and-palestine) — 114 MB — combined extract: Israel and Palestine
- [Italy](#europe-italy) — 2.08 GB
  - [Centro](#europe-italy-centro) — 2.60 GB
  - [Isole](#europe-italy-isole) — 1.74 GB
  - [Nord-Est](#europe-italy-nord-est) — 3.22 GB
  - [Nord-Ovest](#europe-italy-nord-ovest) — 3.57 GB
  - [Sud](#europe-italy-sud) — 2.69 GB
- [Ivory Coast](#africa-ivory-coast) — 81.4 MB
- [Jamaica](#central-america-jamaica) — 36.8 MB
- [Japan](#asia-japan) — 2.35 GB
  - [Chūbu region](#asia-japan-chubu) — 4.35 GB
  - [Chūgoku region](#asia-japan-chugoku) — 1.80 GB
  - [Hokkaidō](#asia-japan-hokkaido) — 1.24 GB
  - [Kansai region (a.k.a. Kinki region)](#asia-japan-kansai) — 3.03 GB
  - [Kantō region](#asia-japan-kanto) — 5.07 GB
  - [Kyūshū](#asia-japan-kyushu) — 3.13 GB
  - [Shikoku](#asia-japan-shikoku) — 1010 MB
  - [Tōhoku region](#asia-japan-tohoku) — 2.44 GB
- [Jordan](#asia-jordan) — 29.6 MB
- [Kazakhstan](#asia-kazakhstan) — 213 MB
- [Kenya](#africa-kenya) — 334 MB
- [Kiribati](#australia-oceania-kiribati) — 2.33 MB
- [Kosovo](#europe-kosovo) — 29.3 MB — territory extract (disputed status; own Geofabrik leaf)
- [Kyrgyzstan](#asia-kyrgyzstan) — 71.3 MB
- [Laos](#asia-laos) — 51.1 MB
- [Latvia](#europe-latvia) — 134 MB
- [Lebanon](#asia-lebanon) — 50.1 MB
- [Lesotho](#africa-lesotho) — 121 MB
- [Liberia](#africa-liberia) — 35.6 MB
- [Libya](#africa-libya) — 73.0 MB
- [Liechtenstein](#europe-liechtenstein) — 3.29 MB
- [Lithuania](#europe-lithuania) — 212 MB
- [Luxembourg](#europe-luxembourg) — 45.3 MB
- [Macedonia](#europe-macedonia) — 28.3 MB
- [Madagascar](#africa-madagascar) — 371 MB
- [Malawi](#africa-malawi) — 148 MB
- [Malaysia, Singapore, and Brunei](#asia-malaysia-singapore-brunei) — 239 MB — multi-country extract: Malaysia + Singapore + Brunei
- [Maldives](#asia-maldives) — 5.00 MB
- [Mali](#africa-mali) — 165 MB
- [Malta](#europe-malta) — 8.50 MB
- [Marshall Islands](#australia-oceania-marshall-islands) — 2.11 MB
- [Mauritania](#africa-mauritania) — 29.1 MB
- [Mauritius](#africa-mauritius) — 8.88 MB
- [Mexico](#north-america-mexico) — 616 MB
- [Micronesia](#australia-oceania-micronesia) — 1.89 MB
- [Moldova](#europe-moldova) — 96.4 MB
- [Monaco](#europe-monaco) — 675 KB
- [Mongolia](#asia-mongolia) — 59.2 MB
- [Montenegro](#europe-montenegro) — 32.8 MB
- [Morocco](#africa-morocco) — 232 MB
- [Mozambique](#africa-mozambique) — 244 MB
- [Myanmar (a.k.a. Burma)](#asia-myanmar) — 269 MB
- [Namibia](#africa-namibia) — 52.0 MB
- [Nauru](#australia-oceania-nauru) — 260 KB
- [Nepal](#asia-nepal) — 395 MB
- [Netherlands](#europe-netherlands) — 1.31 GB
  - [Drenthe](#europe-netherlands-drenthe) — 186 MB
  - [Flevoland](#europe-netherlands-flevoland) — 120 MB
  - [Friesland](#europe-netherlands-friesland) — 230 MB
  - [Gelderland](#europe-netherlands-gelderland) — 542 MB
  - [Groningen](#europe-netherlands-groningen) — 140 MB
  - [Limburg](#europe-netherlands-limburg) — 249 MB
  - [Noord-Brabant](#europe-netherlands-noord-brabant) — 523 MB
  - [Noord-Holland](#europe-netherlands-noord-holland) — 450 MB
  - [Overijssel](#europe-netherlands-overijssel) — 310 MB
  - [Utrecht](#europe-netherlands-utrecht) — 223 MB
  - [Zeeland](#europe-netherlands-zeeland) — 121 MB
  - [Zuid-Holland](#europe-netherlands-zuid-holland) — 542 MB
- [New Caledonia](#australia-oceania-new-caledonia) — 13.5 MB
- [New Zealand](#australia-oceania-new-zealand) — 384 MB
- [Nicaragua](#central-america-nicaragua) — 58.5 MB
- [Niger](#africa-niger) — 74.1 MB
- [Nigeria](#africa-nigeria) — 675 MB
- [Niue](#australia-oceania-niue) — 415 KB
- [North Korea](#asia-north-korea) — 87.7 MB
- [Norway](#europe-norway) — 1.28 GB
  - [Nord-Norge](#europe-norway-nord-norge) — 836 MB
  - [Østlandet](#europe-norway-ostlandet) — 2.43 GB
  - [Sørlandet](#europe-norway-sorlandet) — 304 MB
  - [Svalbard and Jan Mayen](#europe-norway-svalbard-janmayen) — 7.78 MB
  - [Trøndelag](#europe-norway-trondelag) — 615 MB
  - [Vestlandet](#europe-norway-vestlandet) — 1.11 GB
- [Pakistan](#asia-pakistan) — 149 MB
- [Palau](#australia-oceania-palau) — 801 KB
- [Panama](#central-america-panama) — 34.5 MB
- [Papua New Guinea](#australia-oceania-papua-new-guinea) — 51.6 MB
- [Paraguay](#south-america-paraguay) — 147 MB
- [Peru](#south-america-peru) — 244 MB
- [Philippines](#asia-philippines) — 578 MB
- [Pitcairn Islands](#australia-oceania-pitcairn-islands) — 112 KB
- [Poland](#europe-poland) — 1.95 GB
  - [Dolnośląskie](#europe-poland-dolnoslaskie) — 1.01 GB
  - [Kujawsko-pomorskie](#europe-poland-kujawsko-pomorskie) — 717 MB
  - [Łódzkie](#europe-poland-lodzkie) — 638 MB
  - [Lubelskie](#europe-poland-lubelskie) — 960 MB
  - [Lubuskie](#europe-poland-lubuskie) — 430 MB
  - [Małopolskie](#europe-poland-malopolskie) — 1.22 GB
  - [Mazowieckie](#europe-poland-mazowieckie) — 1.47 GB
  - [Opolskie](#europe-poland-opolskie) — 335 MB
  - [Podkarpackie](#europe-poland-podkarpackie) — 917 MB
  - [Podlaskie](#europe-poland-podlaskie) — 467 MB
  - [Pomorskie](#europe-poland-pomorskie) — 791 MB
  - [Śląskie](#europe-poland-slaskie) — 1.20 GB
  - [Świętokrzyskie](#europe-poland-swietokrzyskie) — 498 MB
  - [Warmińsko-mazurskie](#europe-poland-warminsko-mazurskie) — 528 MB
  - [Wielkopolskie](#europe-poland-wielkopolskie) — 902 MB
  - [Zachodniopomorskie](#europe-poland-zachodniopomorskie) — 553 MB
- [Polynésie française (French Polynesia)](#australia-oceania-polynesie-francaise) — 14.9 MB
- [Portugal](#europe-portugal) — 403 MB
- [Romania](#europe-romania) — 313 MB
- [Russian Federation](#russia) — 3.87 GB
  - [Central Federal District](#russia-central-fed-district) — 5.73 GB
  - [Crimean Federal District](#russia-crimean-fed-district) — 369 MB
  - [Far Eastern Federal District](#russia-far-eastern-fed-district) — 1.26 GB
  - [Kaliningrad](#russia-kaliningrad) — 139 MB
  - [North Caucasus Federal District](#russia-north-caucasus-fed-district) — 877 MB
  - [Northwestern Federal District](#russia-northwestern-fed-district) — 3.08 GB
  - [Siberian Federal District](#russia-siberian-fed-district) — 2.90 GB
  - [South Federal District](#russia-south-fed-district) — 1.98 GB
  - [Ural Federal District](#russia-ural-fed-district) — 1.96 GB
  - [Volga Federal District](#russia-volga-fed-district) — 4.29 GB
- [Rwanda](#africa-rwanda) — 64.1 MB
- [Saint Helena, Ascension, and Tristan da Cunha](#africa-saint-helena-ascension-and-tristan-da-cunha) — 876 KB
- [Samoa](#australia-oceania-samoa) — 3.30 MB
- [Sao Tome and Principe](#africa-sao-tome-and-principe) — 1.20 MB
- [Senegal and Gambia](#africa-senegal-and-gambia) — 100 MB — multi-country extract: Senegal + Gambia
- [Serbia](#europe-serbia) — 229 MB
- [Seychelles](#africa-seychelles) — 2.63 MB
- [Sierra Leone](#africa-sierra-leone) — 44.1 MB
- [Slovakia](#europe-slovakia) — 328 MB
- [Slovenia](#europe-slovenia) — 298 MB
- [Solomon Islands](#australia-oceania-solomon-islands) — 11.4 MB
- [Somalia](#africa-somalia) — 157 MB
- [South Africa](#africa-south-africa) — 401 MB
- [South Africa (includes Lesotho)](#africa-south-africa-and-lesotho) — 520 MB — multi-country extract: South Africa including Lesotho
- [South Korea](#asia-south-korea) — 274 MB
- [South Sudan](#africa-south-sudan) — 132 MB
- [South-East Asia](#asia-sea) — 3.41 GB — multi-country extract for South-East Asia (Geofabrik id `sea`)
- [Spain](#europe-spain) — 1.38 GB
  - [Andalucía](#europe-spain-andalucia) — 1.75 GB
  - [Aragón](#europe-spain-aragon) — 763 MB
  - [Asturias](#europe-spain-asturias) — 288 MB
  - [Cantabria](#europe-spain-cantabria) — 227 MB
  - [Castilla-La Mancha](#europe-spain-castilla-la-mancha) — 1.09 GB
  - [Castilla y León](#europe-spain-castilla-y-leon) — 1.73 GB
  - [Cataluña](#europe-spain-cataluna) — 2.21 GB
  - [Ceuta](#europe-spain-ceuta) — 4.72 MB
  - [Extremadura](#europe-spain-extremadura) — 448 MB
  - [Galicia](#europe-spain-galicia) — 1.18 GB
  - [Islas Baleares](#europe-spain-islas-baleares) — 264 MB
  - [La Rioja](#europe-spain-la-rioja) — 115 MB
  - [Madrid](#europe-spain-madrid) — 668 MB
  - [Melilla](#europe-spain-melilla) — 5.72 MB
  - [Murcia](#europe-spain-murcia) — 446 MB
  - [Navarra](#europe-spain-navarra) — 411 MB
  - [País Vasco](#europe-spain-pais-vasco) — 462 MB
  - [Valencia](#europe-spain-valencia) — 1.24 GB
- [Sri Lanka](#asia-sri-lanka) — 138 MB
- [Sudan](#africa-sudan) — 195 MB
- [Suriname](#south-america-suriname) — 20.4 MB
- [Swaziland](#africa-swaziland) — 29.2 MB
- [Sweden](#europe-sweden) — 779 MB
  - [Blekinge](#europe-sweden-blekinge) — 81.9 MB
  - [Dalarna](#europe-sweden-dalarna) — 281 MB
  - [Gävleborg](#europe-sweden-gavleborg) — 217 MB
  - [Gotland](#europe-sweden-gotland) — 51.7 MB
  - [Halland](#europe-sweden-halland) — 175 MB
  - [Jämtland](#europe-sweden-jamtland) — 242 MB
  - [Jönköping](#europe-sweden-jonkoping) — 210 MB
  - [Kalmar](#europe-sweden-kalmar) — 189 MB
  - [Kronoberg](#europe-sweden-kronoberg) — 131 MB
  - [Norrbotten](#europe-sweden-norrbotten) — 324 MB
  - [Örebro](#europe-sweden-orebro) — 218 MB
  - [Östergötland](#europe-sweden-ostergotland) — 345 MB
  - [Skåne](#europe-sweden-skane) — 455 MB
  - [Södermanland](#europe-sweden-sodermanland) — 147 MB
  - [Stockholm](#europe-sweden-stockholm) — 560 MB
  - [Uppsala](#europe-sweden-uppsala) — 184 MB
  - [Värmland](#europe-sweden-varmland) — 255 MB
  - [Västerbotten](#europe-sweden-vasterbotten) — 251 MB
  - [Västernorrland](#europe-sweden-vasternorrland) — 253 MB
  - [Västmanland](#europe-sweden-vastmanland) — 126 MB
  - [Västra Götaland](#europe-sweden-vastra-gotaland) — 927 MB
- [Switzerland](#europe-switzerland) — 521 MB
- [Syria](#asia-syria) — 77.9 MB
- [Taiwan](#asia-taiwan) — 311 MB
- [Tajikistan](#asia-tajikistan) — 46.1 MB
- [Tanzania](#africa-tanzania) — 673 MB
- [Thailand](#asia-thailand) — 312 MB
- [Togo](#africa-togo) — 59.4 MB
- [Tokelau](#australia-oceania-tokelau) — 142 KB
- [Tonga](#australia-oceania-tonga) — 3.54 MB
- [Tunisia](#africa-tunisia) — 80.2 MB
- [Turkey](#europe-turkey) — 616 MB
- [Turkmenistan](#asia-turkmenistan) — 23.7 MB
- [Tuvalu](#australia-oceania-tuvalu) — 357 KB
- [Uganda](#africa-uganda) — 354 MB
- [Ukraine (with Crimea)](#europe-ukraine) — 836 MB
- [United Kingdom](#europe-united-kingdom) — 2.10 GB
  - [Bermuda](#europe-united-kingdom-bermuda) — 12.3 MB
  - [England](#europe-united-kingdom-england)
    - [Bedfordshire](#europe-united-kingdom-england-bedfordshire) — 103 MB
    - [Berkshire](#europe-united-kingdom-england-berkshire) — 132 MB
    - [Bristol](#europe-united-kingdom-england-bristol) — 49.9 MB
    - [Buckinghamshire](#europe-united-kingdom-england-buckinghamshire) — 152 MB
    - [Cambridgeshire](#europe-united-kingdom-england-cambridgeshire) — 183 MB
    - [Cheshire](#europe-united-kingdom-england-cheshire) — 218 MB
    - [Cornwall](#europe-united-kingdom-england-cornwall) — 237 MB
    - [Cumbria](#europe-united-kingdom-england-cumbria) — 220 MB
    - [Derbyshire](#europe-united-kingdom-england-derbyshire) — 192 MB
    - [Devon](#europe-united-kingdom-england-devon) — 392 MB
    - [Dorset](#europe-united-kingdom-england-dorset) — 156 MB
    - [Durham](#europe-united-kingdom-england-durham) — 168 MB
    - [East Sussex](#europe-united-kingdom-england-east-sussex) — 103 MB
    - [East Yorkshire with Hull](#europe-united-kingdom-england-east-yorkshire-with-hull) — 94.2 MB
    - [Essex](#europe-united-kingdom-england-essex) — 294 MB
    - [Gloucestershire](#europe-united-kingdom-england-gloucestershire) — 209 MB
    - [Greater London](#europe-united-kingdom-england-greater-london) — 667 MB
    - [Greater Manchester](#europe-united-kingdom-england-greater-manchester) — 362 MB
    - [Hampshire](#europe-united-kingdom-england-hampshire) — 372 MB
    - [Herefordshire](#europe-united-kingdom-england-herefordshire) — 57.5 MB
    - [Hertfordshire](#europe-united-kingdom-england-hertfordshire) — 196 MB
    - [Isle of Wight](#europe-united-kingdom-england-isle-of-wight) — 29.9 MB
    - [Kent](#europe-united-kingdom-england-kent) — 342 MB
    - [Lancashire](#europe-united-kingdom-england-lancashire) — 262 MB
    - [Leicestershire](#europe-united-kingdom-england-leicestershire) — 150 MB
    - [Lincolnshire](#europe-united-kingdom-england-lincolnshire) — 228 MB
    - [London](#europe-united-kingdom-england-london)
      - [Enfield](#europe-united-kingdom-england-london-enfield) — 20.8 MB
    - [Merseyside](#europe-united-kingdom-england-merseyside) — 152 MB
    - [Norfolk](#europe-united-kingdom-england-norfolk) — 269 MB
    - [North Yorkshire](#europe-united-kingdom-england-north-yorkshire) — 329 MB
    - [Northamptonshire](#europe-united-kingdom-england-northamptonshire) — 151 MB
    - [Northumberland](#europe-united-kingdom-england-northumberland) — 98.2 MB
    - [Nottinghamshire](#europe-united-kingdom-england-nottinghamshire) — 206 MB
    - [Oxfordshire](#europe-united-kingdom-england-oxfordshire) — 145 MB
    - [Rutland](#europe-united-kingdom-england-rutland) — 11.0 MB
    - [Shropshire](#europe-united-kingdom-england-shropshire) — 139 MB
    - [Somerset](#europe-united-kingdom-england-somerset) — 293 MB
    - [South Yorkshire](#europe-united-kingdom-england-south-yorkshire) — 183 MB
    - [Staffordshire](#europe-united-kingdom-england-staffordshire) — 185 MB
    - [Suffolk](#europe-united-kingdom-england-suffolk) — 205 MB
    - [Surrey](#europe-united-kingdom-england-surrey) — 205 MB
    - [Tyne and Wear](#europe-united-kingdom-england-tyne-and-wear) — 143 MB
    - [Warwickshire](#europe-united-kingdom-england-warwickshire) — 124 MB
    - [West Midlands](#europe-united-kingdom-england-west-midlands) — 262 MB
    - [West Sussex](#europe-united-kingdom-england-west-sussex) — 186 MB
    - [West Yorkshire](#europe-united-kingdom-england-west-yorkshire) — 320 MB
    - [Wiltshire](#europe-united-kingdom-england-wiltshire) — 185 MB
    - [Worcestershire](#europe-united-kingdom-england-worcestershire) — 112 MB
  - [Falkland Islands](#europe-united-kingdom-falklands) — 18.2 MB
  - [Scotland](#europe-united-kingdom-scotland) — 1.80 GB
  - [Wales](#europe-united-kingdom-wales) — 858 MB
- [United States of America](#north-america-us) — 11.3 GB
  - [us/alabama](#north-america-us-alabama) — 1.78 GB
  - [us/alaska](#north-america-us-alaska) — 390 MB
  - [us/arizona](#north-america-us-arizona) — 2.68 GB
  - [us/arkansas](#north-america-us-arkansas) — 1.24 GB
  - [us/california](#north-america-us-california)
    - [Northern California](#north-america-us-california-norcal) — 4.32 GB
    - [Southern California](#north-america-us-california-socal) — 4.05 GB
  - [us/colorado](#north-america-us-colorado) — 2.68 GB
  - [us/connecticut](#north-america-us-connecticut) — 1.11 GB
  - [us/delaware](#north-america-us-delaware) — 255 MB
  - [us/district-of-columbia](#north-america-us-district-of-columbia) — 121 MB
  - [us/florida](#north-america-us-florida) — 5.37 GB
  - [Georgia](#north-america-us-georgia) — 3.09 GB
  - [us/hawaii](#north-america-us-hawaii) — 204 MB
  - [us/idaho](#north-america-us-idaho) — 1.32 GB
  - [us/illinois](#north-america-us-illinois) — 3.75 GB
  - [us/indiana](#north-america-us-indiana) — 2.49 GB
  - [us/iowa](#north-america-us-iowa) — 1.37 GB
  - [us/kansas](#north-america-us-kansas) — 1.59 GB
  - [us/kentucky](#north-america-us-kentucky) — 1.63 GB
  - [us/louisiana](#north-america-us-louisiana) — 1.37 GB
  - [us/maine](#north-america-us-maine) — 736 MB
  - [us/maryland](#north-america-us-maryland) — 1.71 GB
  - [us/massachusetts](#north-america-us-massachusetts) — 1.96 GB
  - [us/michigan](#north-america-us-michigan) — 3.88 GB
  - [us/minnesota](#north-america-us-minnesota) — 2.10 GB
  - [us/mississippi](#north-america-us-mississippi) — 984 MB
  - [us/missouri](#north-america-us-missouri) — 2.68 GB
  - [us/montana](#north-america-us-montana) — 1011 MB
  - [us/nebraska](#north-america-us-nebraska) — 972 MB
  - [us/nevada](#north-america-us-nevada) — 1.21 GB
  - [us/new-hampshire](#north-america-us-new-hampshire) — 690 MB
  - [us/new-jersey](#north-america-us-new-jersey) — 1.65 GB
  - [us/new-mexico](#north-america-us-new-mexico) — 1.18 GB
  - [us/new-york](#north-america-us-new-york) — 3.52 GB
  - [us/north-carolina](#north-america-us-north-carolina) — 4.00 GB
  - [us/north-dakota](#north-america-us-north-dakota) — 760 MB
  - [us/ohio](#north-america-us-ohio) — 4.02 GB
  - [us/oklahoma](#north-america-us-oklahoma) — 1.68 GB
  - [us/oregon](#north-america-us-oregon) — 2.05 GB
  - [us/pennsylvania](#north-america-us-pennsylvania) — 3.49 GB
  - [us/puerto-rico](#north-america-us-puerto-rico) — 419 MB
  - [us/rhode-island](#north-america-us-rhode-island) — 218 MB
  - [us/south-carolina](#north-america-us-south-carolina) — 1.71 GB
  - [us/south-dakota](#north-america-us-south-dakota) — 546 MB
  - [us/tennessee](#north-america-us-tennessee) — 2.06 GB
  - [us/texas](#north-america-us-texas) — 8.17 GB
  - [us/us-virgin-islands](#north-america-us-us-virgin-islands) — 23.8 MB
  - [us/utah](#north-america-us-utah) — 1.69 GB
  - [us/vermont](#north-america-us-vermont) — 368 MB
  - [us/virginia](#north-america-us-virginia) — 3.13 GB
  - [us/washington](#north-america-us-washington) — 2.81 GB
  - [us/west-virginia](#north-america-us-west-virginia) — 760 MB
  - [us/wisconsin](#north-america-us-wisconsin) — 2.41 GB
  - [us/wyoming](#north-america-us-wyoming) — 733 MB
- [Uruguay](#south-america-uruguay) — 53.5 MB
- [US Midwest](#north-america-us-midwest) — 2.33 GB — special US regional extract (overlaps state packs; not a country)
- [US Northeast](#north-america-us-northeast) — 1.67 GB — special US regional extract (overlaps state packs; not a country)
- [US Pacific](#north-america-us-pacific) — 164 MB — special US regional extract (overlaps state packs; not a country)
- [US South](#north-america-us-south) — 3.84 GB — special US regional extract (overlaps state packs; not a country)
- [US West](#north-america-us-west) — 3.17 GB — special US regional extract (overlaps state packs; not a country)
- [Uzbekistan](#asia-uzbekistan) — 118 MB
- [Vanuatu](#australia-oceania-vanuatu) — 7.53 MB
- [Venezuela](#south-america-venezuela) — 121 MB
- [Vietnam](#asia-vietnam) — 313 MB
- [Wallis et Futuna](#australia-oceania-wallis-et-futuna) — 604 KB
- [Yemen](#asia-yemen) — 41.2 MB
- [Zambia](#africa-zambia) — 240 MB
- [Zimbabwe](#africa-zimbabwe) — 171 MB

---

## Afghanistan

- Continent / group: Asia
- Region id: `asia/afghanistan`
- Country-level pack: yes
- Geofabrik country PBF size: **107 MB** (`112455439` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/afghanistan.html
- Pack-server country pack size: **1.12 GB** (`1207917589` bytes)
- Subregions: none (OSM has **35** `ISO3166-2` province codes; Geofabrik/pack host not split)

## Albania

- Continent / group: Europe
- Region id: `europe/albania`
- Country-level pack: yes
- Geofabrik country PBF size: **51.5 MB** (`54002234` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/albania.html
- Pack-server country pack size: **502 MB** (`526259035` bytes)
- Subregions: none

## Algeria

- Continent / group: Africa
- Region id: `africa/algeria`
- Country-level pack: yes
- Geofabrik country PBF size: **286 MB** (`299938949` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/algeria.html
- Pack-server country pack size: **2.16 GB** (`2322447961` bytes)
- Subregions: none

## Alps

- Continent / group: Europe
- Region id: `europe/alps`
- Extract type: regional Geofabrik extract for the Alpine mountain area across several countries (not a country)
- Country-level pack: yes
- Geofabrik country PBF size: **2.16 GB** (`2319806701` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/alps.html
- Pack-server country pack size: **12.8 GB** (`13693715369` bytes)
- Subregions: none

## American Oceania

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/american-oceania`
- Extract type: regional Geofabrik extract for US-affiliated Pacific islands (not a country)
- Country-level pack: yes
- Geofabrik country PBF size: **5.12 MB** (`5368012` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/american-oceania.html
- Pack-server country pack size: **46.4 MB** (`48681512` bytes)
- Subregions: none

## Andorra

- Continent / group: Europe
- Region id: `europe/andorra`
- Country-level pack: yes
- Geofabrik country PBF size: **3.31 MB** (`3466488` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/andorra.html
- Pack-server country pack size: **14.9 MB** (`15673690` bytes)
- Subregions: none

## Angola

- Continent / group: Africa
- Region id: `africa/angola`
- Country-level pack: yes
- Geofabrik country PBF size: **81.3 MB** (`85228708` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/angola.html
- Pack-server country pack size: **968 MB** (`1015084690` bytes)
- Subregions: none

## Antarctica

- Continent / group: Antarctica
- Region id: `antarctica`
- Country-level pack: yes
- Geofabrik country PBF size: **31.6 MB** (`33091090` bytes)
- Geofabrik URL: https://download.geofabrik.de/antarctica.html
- Pack-server country pack size: **6.53 MB** (`6843080` bytes)
- Subregions: none

## Argentina

- Continent / group: South America
- Region id: `south-america/argentina`
- Country-level pack: yes
- Geofabrik country PBF size: **410 MB** (`430115755` bytes)
- Geofabrik URL: https://download.geofabrik.de/south-america/argentina.html
- Pack-server country pack size: **3.62 GB** (`3889044594` bytes)
- Subregions: none

## Armenia

- Continent / group: Asia
- Region id: `asia/armenia`
- Country-level pack: yes
- Geofabrik country PBF size: **50.7 MB** (`53137041` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/armenia.html
- Pack-server country pack size: **452 MB** (`473438120` bytes)
- Subregions: none

## Australia

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/australia`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **918 MB** (`962789460` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/australia.html
- Subregions (sizes from pack server `current.json`):
  - **Australian Capital Territory** `australia-oceania/australia/act` — pack **133 MB** (`139553321` bytes)
  - **Christmas Island** `australia-oceania/australia/christmas-island` — pack **1.15 MB** (`1207971` bytes)
  - **Cocos (Keeling) Islands** `australia-oceania/australia/cocos-islands` — pack **423 KB** (`433084` bytes)
  - **Coral Sea Islands** `australia-oceania/australia/coral-sea-islands` — pack **6.42 KB** (`6578` bytes)
  - **New South Wales (with ACT and JBT)** `australia-oceania/australia/new-south-wales` — pack **1.91 GB** (`2047523526` bytes)
  - **Norfolk Island** `australia-oceania/australia/norfolk-island` — pack **1.16 MB** (`1211219` bytes)
  - **Northern Territory** `australia-oceania/australia/northern-territory` — pack **147 MB** (`153698714` bytes)
  - **Queensland** `australia-oceania/australia/queensland` — pack **1.21 GB** (`1304039181` bytes)
  - **South Australia** `australia-oceania/australia/south-australia` — pack **628 MB** (`658672562` bytes)
  - **Tasmania** `australia-oceania/australia/tasmania` — pack **241 MB** (`252440067` bytes)
  - **Victoria** `australia-oceania/australia/victoria` — pack **2.07 GB** (`2226938092` bytes)
  - **Western Australia** `australia-oceania/australia/western-australia` — pack **967 MB** (`1014492733` bytes)

## Austria

- Continent / group: Europe
- Region id: `europe/austria`
- Country-level pack: yes
- Geofabrik country PBF size: **773 MB** (`810040480` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/austria.html
- Pack-server country pack size: **5.06 GB** (`5431482098` bytes)
- Subregions: none

## Azerbaijan

- Continent / group: Asia
- Region id: `asia/azerbaijan`
- Country-level pack: yes
- Geofabrik country PBF size: **44.0 MB** (`46162988` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/azerbaijan.html
- Pack-server country pack size: **1.01 GB** (`1082734553` bytes)
- Subregions: none

## Azores

- Continent / group: Europe
- Region id: `europe/azores`
- Extract type: Portuguese Atlantic autonomous region (not a country)
- Country-level pack: yes
- Geofabrik country PBF size: **16.9 MB** (`17692392` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/azores.html
- Pack-server country pack size: **90.0 MB** (`94412723` bytes)
- Subregions: none

## Bahamas

- Continent / group: Central America
- Region id: `central-america/bahamas`
- Country-level pack: yes
- Geofabrik country PBF size: **13.6 MB** (`14272581` bytes)
- Geofabrik URL: https://download.geofabrik.de/central-america/bahamas.html
- Pack-server country pack size: **93.0 MB** (`97514643` bytes)
- Subregions: none

## Bangladesh

- Continent / group: Asia
- Region id: `asia/bangladesh`
- Country-level pack: yes
- Geofabrik country PBF size: **338 MB** (`354208454` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/bangladesh.html
- Pack-server country pack size: **1.77 GB** (`1899750466` bytes)
- Subregions: none

## Belarus

- Continent / group: Europe
- Region id: `europe/belarus`
- Country-level pack: yes
- Geofabrik country PBF size: **333 MB** (`348924252` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/belarus.html
- Pack-server country pack size: **2.14 GB** (`2301333471` bytes)
- Subregions: none

## Belgium

- Continent / group: Europe
- Region id: `europe/belgium`
- Country-level pack: yes
- Geofabrik country PBF size: **662 MB** (`694273273` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/belgium.html
- Pack-server country pack size: **2.22 GB** (`2388390665` bytes)
- Subregions: none

## Belize

- Continent / group: Central America
- Region id: `central-america/belize`
- Country-level pack: yes
- Geofabrik country PBF size: **17.4 MB** (`18250607` bytes)
- Geofabrik URL: https://download.geofabrik.de/central-america/belize.html
- Pack-server country pack size: **107 MB** (`111911005` bytes)
- Subregions: none

## Benin

- Continent / group: Africa
- Region id: `africa/benin`
- Country-level pack: yes
- Geofabrik country PBF size: **46.0 MB** (`48206317` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/benin.html
- Pack-server country pack size: **409 MB** (`428752008` bytes)
- Subregions: none

## Bhutan

- Continent / group: Asia
- Region id: `asia/bhutan`
- Country-level pack: yes
- Geofabrik country PBF size: **22.5 MB** (`23600797` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/bhutan.html
- Pack-server country pack size: **149 MB** (`156663057` bytes)
- Subregions: none

## Bolivia

- Continent / group: South America
- Region id: `south-america/bolivia`
- Country-level pack: yes
- Geofabrik country PBF size: **165 MB** (`173495088` bytes)
- Geofabrik URL: https://download.geofabrik.de/south-america/bolivia.html
- Pack-server country pack size: **1.90 GB** (`2035406167` bytes)
- Subregions: none

## Bosnia-Herzegovina

- Continent / group: Europe
- Region id: `europe/bosnia-herzegovina`
- Country-level pack: yes
- Geofabrik country PBF size: **153 MB** (`160944213` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/bosnia-herzegovina.html
- Pack-server country pack size: **872 MB** (`914286017` bytes)
- Subregions: none

## Botswana

- Continent / group: Africa
- Region id: `africa/botswana`
- Country-level pack: yes
- Geofabrik country PBF size: **83.9 MB** (`87953194` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/botswana.html
- Pack-server country pack size: **717 MB** (`752090457` bytes)
- Subregions: none

## Brazil

- Continent / group: South America
- Region id: `south-america/brazil`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **1.94 GB** (`2084317159` bytes)
- Geofabrik URL: https://download.geofabrik.de/south-america/brazil.html
- Subregions (sizes from pack server `current.json`):
  - **Centro-Oeste** `south-america/brazil/centro-oeste` — pack **1.83 GB** (`1968361162` bytes)
  - **Nordeste** `south-america/brazil/nordeste` — pack **5.81 GB** (`6239193889` bytes)
  - **Norte** `south-america/brazil/norte` — pack **1.35 GB** (`1448046207` bytes)
  - **Sudeste** `south-america/brazil/sudeste` — pack **6.87 GB** (`7373399123` bytes)
  - **Sul** `south-america/brazil/sul` — pack **3.77 GB** (`4050770681` bytes)

## Britain and Ireland

- Continent / group: Europe
- Region id: `europe/britain-and-ireland`
- Extract type: multi-country Geofabrik extract — Great Britain + Ireland
- Country-level pack: yes
- Geofabrik country PBF size: **2.43 GB** (`2607456933` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/britain-and-ireland.html
- Pack-server country pack size: **16.1 GB** (`17287727445` bytes)
- Subregions: none

## Bulgaria

- Continent / group: Europe
- Region id: `europe/bulgaria`
- Country-level pack: yes
- Geofabrik country PBF size: **166 MB** (`173688054` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/bulgaria.html
- Pack-server country pack size: **1.56 GB** (`1672032460` bytes)
- Subregions: none

## Burkina Faso

- Continent / group: Africa
- Region id: `africa/burkina-faso`
- Country-level pack: yes
- Geofabrik country PBF size: **80.7 MB** (`84581858` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/burkina-faso.html
- Pack-server country pack size: **981 MB** (`1028414035` bytes)
- Subregions: none

## Burundi

- Continent / group: Africa
- Region id: `africa/burundi`
- Country-level pack: yes
- Geofabrik country PBF size: **44.1 MB** (`46238156` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/burundi.html
- Pack-server country pack size: **447 MB** (`468938392` bytes)
- Subregions: none

## Cambodia

- Continent / group: Asia
- Region id: `asia/cambodia`
- Country-level pack: yes
- Geofabrik country PBF size: **39.0 MB** (`40843923` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/cambodia.html
- Pack-server country pack size: **683 MB** (`716027590` bytes)
- Subregions: none

## Cameroon

- Continent / group: Africa
- Region id: `africa/cameroon`
- Country-level pack: yes
- Geofabrik country PBF size: **213 MB** (`223210071` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/cameroon.html
- Pack-server country pack size: **1.02 GB** (`1100343862` bytes)
- Subregions: none

## Canada

- Continent / group: North America
- Region id: `north-america/canada`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **6.01 GB** (`6458077634` bytes)
- Geofabrik URL: https://download.geofabrik.de/north-america/canada.html
- Subregions (sizes from pack server `current.json`):
  - **Alberta** `north-america/canada/alberta` — pack **1.53 GB** (`1640957145` bytes)
  - **British Columbia** `north-america/canada/british-columbia`
    - **Interior Administrative Region** `north-america/canada/british-columbia/interior-admreg` — pack **183 MB** (`192249971` bytes)
    - **Island Administrative Region** `north-america/canada/british-columbia/island-admreg` — pack **361 MB** (`378860950` bytes)
    - **Kootenay Administrative Region** `north-america/canada/british-columbia/kootenay-admreg` — pack **145 MB** (`152035199` bytes)
    - **North Administrative Region** `north-america/canada/british-columbia/north-admreg` — pack **264 MB** (`277150286` bytes)
    - **Okanagan Administrative Region** `north-america/canada/british-columbia/okanagan-admreg` — pack **202 MB** (`212242018` bytes)
    - **South Coast Administrative Region** `north-america/canada/british-columbia/southcoast-admreg` — pack **612 MB** (`641466991` bytes)
  - **Manitoba** `north-america/canada/manitoba` — pack **686 MB** (`719035909` bytes)
  - **New Brunswick** `north-america/canada/new-brunswick` — pack **252 MB** (`264452109` bytes)
  - **Newfoundland and Labrador** `north-america/canada/newfoundland-and-labrador` — pack **243 MB** (`254792387` bytes)
  - **Northwest Territories** `north-america/canada/northwest-territories` — pack **112 MB** (`117620051` bytes)
  - **Nova Scotia** `north-america/canada/nova-scotia` — pack **429 MB** (`449981832` bytes)
  - **Nunavut** `north-america/canada/nunavut`
    - **Kitikmeot Region** `north-america/canada/nunavut/kitikmeot` — pack **51.6 MB** (`54112082` bytes)
    - **Kivalliq Region** `north-america/canada/nunavut/kivalliq` — pack **63.4 MB** (`66500313` bytes)
    - **Qikiqtaaluk Region** `north-america/canada/nunavut/qikiqtaaluk` — pack **98.3 MB** (`103082710` bytes)
  - **Ontario** `north-america/canada/ontario` — pack **3.13 GB** (`3360217703` bytes)
  - **Prince Edward Island** `north-america/canada/prince-edward-island` — pack **58.8 MB** (`61659342` bytes)
  - **Quebec** `north-america/canada/quebec` — pack **1.91 GB** (`2052607649` bytes)
  - **Saskatchewan** `north-america/canada/saskatchewan` — pack **578 MB** (`605741841` bytes)
  - **Yukon** `north-america/canada/yukon` — pack **65.3 MB** (`68474890` bytes)

## Canary Islands

- Continent / group: Africa
- Region id: `africa/canary-islands`
- Extract type: Spanish autonomous community (listed under Africa on Geofabrik; not a country)
- Country-level pack: yes
- Geofabrik country PBF size: **57.0 MB** (`59723233` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/canary-islands.html
- Pack-server country pack size: **403 MB** (`422805419` bytes)
- Subregions: none

## Cape Verde

- Continent / group: Africa
- Region id: `africa/cape-verde`
- Country-level pack: yes
- Geofabrik country PBF size: **11.1 MB** (`11683131` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/cape-verde.html
- Pack-server country pack size: **76.7 MB** (`80388287` bytes)
- Subregions: none

## Central African Republic

- Continent / group: Africa
- Region id: `africa/central-african-republic`
- Country-level pack: yes
- Geofabrik country PBF size: **94.8 MB** (`99430227` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/central-african-republic.html
- Pack-server country pack size: **372 MB** (`390262635` bytes)
- Subregions: none

## Chad

- Continent / group: Africa
- Region id: `africa/chad`
- Country-level pack: yes
- Geofabrik country PBF size: **129 MB** (`135015051` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/chad.html
- Pack-server country pack size: **579 MB** (`607550142` bytes)
- Subregions: none

## Chile

- Continent / group: South America
- Region id: `south-america/chile`
- Country-level pack: yes
- Geofabrik country PBF size: **331 MB** (`347200964` bytes)
- Geofabrik URL: https://download.geofabrik.de/south-america/chile.html
- Pack-server country pack size: **2.07 GB** (`2225063854` bytes)
- Subregions: none

## China

- Continent / group: Asia
- Region id: `asia/china`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **1.48 GB** (`1594431947` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/china.html
- Subregions (sizes from pack server `current.json`):
  - **Anhui** `asia/china/anhui` — pack **543 MB** (`569579301` bytes)
  - **Beijing** `asia/china/beijing` — pack **576 MB** (`604035795` bytes)
  - **Chongqing** `asia/china/chongqing` — pack **403 MB** (`422598242` bytes)
  - **Fujian** `asia/china/fujian` — pack **679 MB** (`712437541` bytes)
  - **Gansu** `asia/china/gansu` — pack **789 MB** (`827777567` bytes)
  - **Guangdong (with Hong Kong and Macau)** `asia/china/guangdong` — pack **1.94 GB** (`2079698569` bytes)
  - **Guangxi** `asia/china/guangxi` — pack **675 MB** (`707875035` bytes)
  - **Guizhou** `asia/china/guizhou` — pack **397 MB** (`416091807` bytes)
  - **Hainan** `asia/china/hainan` — pack **148 MB** (`155659327` bytes)
  - **Hebei (with Beijing and Tianjin)** `asia/china/hebei` — pack **1.99 GB** (`2137026720` bytes)
  - **Heilongjiang** `asia/china/heilongjiang` — pack **484 MB** (`507625382` bytes)
  - **Henan** `asia/china/henan` — pack **980 MB** (`1027595940` bytes)
  - **Hong Kong** `asia/china/hong-kong` — pack **186 MB** (`194649130` bytes)
  - **Hubei** `asia/china/hubei` — pack **726 MB** (`761319732` bytes)
  - **Hunan** `asia/china/hunan` — pack **670 MB** (`702897922` bytes)
  - **Inner Mongolia** `asia/china/inner-mongolia` — pack **567 MB** (`594735699` bytes)
  - **Jiangsu** `asia/china/jiangsu` — pack **1.21 GB** (`1298592644` bytes)
  - **Jiangxi** `asia/china/jiangxi` — pack **528 MB** (`554053103` bytes)
  - **Jilin** `asia/china/jilin` — pack **445 MB** (`466750205` bytes)
  - **Liaoning** `asia/china/liaoning` — pack **397 MB** (`416358835` bytes)
  - **Macau** `asia/china/macau` — pack **12.5 MB** (`13090799` bytes)
  - **Ningxia** `asia/china/ningxia` — pack **122 MB** (`128237222` bytes)
  - **Qinghai** `asia/china/qinghai` — pack **219 MB** (`229658263` bytes)
  - **Shaanxi** `asia/china/shaanxi` — pack **569 MB** (`596434486` bytes)
  - **Shandong** `asia/china/shandong` — pack **1.53 GB** (`1639899921` bytes)
  - **Shanghai** `asia/china/shanghai` — pack **297 MB** (`311005247` bytes)
  - **Shanxi** `asia/china/shanxi` — pack **498 MB** (`522028344` bytes)
  - **Sichuan** `asia/china/sichuan` — pack **1.47 GB** (`1576640261` bytes)
  - **Tianjin** `asia/china/tianjin` — pack **283 MB** (`297158093` bytes)
  - **Tibet** `asia/china/tibet` — pack **439 MB** (`459824021` bytes)
  - **Xinjiang** `asia/china/xinjiang` — pack **584 MB** (`612861468` bytes)
  - **Yunnan** `asia/china/yunnan` — pack **1.38 GB** (`1485151212` bytes)
  - **Zhejiang** `asia/china/zhejiang` — pack **1.16 GB** (`1240301048` bytes)

## Colombia

- Continent / group: South America
- Region id: `south-america/colombia`
- Country-level pack: yes
- Geofabrik country PBF size: **314 MB** (`329405940` bytes)
- Geofabrik URL: https://download.geofabrik.de/south-america/colombia.html
- Pack-server country pack size: **2.77 GB** (`2970452644` bytes)
- Subregions: none

## Comores

- Continent / group: Africa
- Region id: `africa/comores`
- Extract type: Geofabrik extract for the Comoros islands
- Country-level pack: yes
- Geofabrik country PBF size: **3.79 MB** (`3978038` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/comores.html
- Pack-server country pack size: **16.2 MB** (`16953244` bytes)
- Subregions: none

## Congo (Democratic Republic/Kinshasa)

- Continent / group: Africa
- Region id: `africa/congo-democratic-republic`
- Country-level pack: yes
- Geofabrik country PBF size: **397 MB** (`416121018` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/congo-democratic-republic.html
- Pack-server country pack size: **2.12 GB** (`2281060404` bytes)
- Subregions: none

## Congo (Republic/Brazzaville)

- Continent / group: Africa
- Region id: `africa/congo-brazzaville`
- Country-level pack: yes
- Geofabrik country PBF size: **31.1 MB** (`32600149` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/congo-brazzaville.html
- Pack-server country pack size: **207 MB** (`217363207` bytes)
- Subregions: none

## Cook Islands

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/cook-islands`
- Country-level pack: yes
- Geofabrik country PBF size: **950 KB** (`972429` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/cook-islands.html
- Pack-server country pack size: **4.10 MB** (`4296136` bytes)
- Subregions: none

## Costa Rica

- Continent / group: Central America
- Region id: `central-america/costa-rica`
- Country-level pack: yes
- Geofabrik country PBF size: **37.2 MB** (`38959038` bytes)
- Geofabrik URL: https://download.geofabrik.de/central-america/costa-rica.html
- Pack-server country pack size: **375 MB** (`392789020` bytes)
- Subregions: none

## Croatia

- Continent / group: Europe
- Region id: `europe/croatia`
- Country-level pack: yes
- Geofabrik country PBF size: **190 MB** (`199539642` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/croatia.html
- Pack-server country pack size: **1.25 GB** (`1343644615` bytes)
- Subregions: none

## Cuba

- Continent / group: Central America
- Region id: `central-america/cuba`
- Country-level pack: yes
- Geofabrik country PBF size: **59.1 MB** (`61959696` bytes)
- Geofabrik URL: https://download.geofabrik.de/central-america/cuba.html
- Pack-server country pack size: **677 MB** (`709547030` bytes)
- Subregions: none

## Cyprus

- Continent / group: Europe
- Region id: `europe/cyprus`
- Country-level pack: yes
- Geofabrik country PBF size: **35.6 MB** (`37335233` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/cyprus.html
- Pack-server country pack size: **506 MB** (`530826514` bytes)
- Subregions: none

## Czech Republic

- Continent / group: Europe
- Region id: `europe/czech-republic`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **903 MB** (`946569209` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/czech-republic.html
- Subregions (sizes from pack server `current.json`):
  - **Jihočeský kraj** `europe/czech-republic/jihocesky` — pack **330 MB** (`346303788` bytes)
  - **Jihomoravský kraj** `europe/czech-republic/jihomoravsky` — pack **372 MB** (`389865099` bytes)
  - **Karlovarský kraj** `europe/czech-republic/karlovarsky` — pack **107 MB** (`111780238` bytes)
  - **Královéhradecký kraj** `europe/czech-republic/kralovehradecky` — pack **198 MB** (`207696854` bytes)
  - **Liberecký kraj** `europe/czech-republic/liberecky` — pack **160 MB** (`167522166` bytes)
  - **Moravskoslezský kraj** `europe/czech-republic/moravskoslezky` — pack **267 MB** (`279621562` bytes)
  - **Olomoucký kraj** `europe/czech-republic/olomoucky` — pack **219 MB** (`229879385` bytes)
  - **Pardubický kraj** `europe/czech-republic/pardubicky` — pack **187 MB** (`196505423` bytes)
  - **Plzeňský kraj** `europe/czech-republic/plzensky` — pack **254 MB** (`265840892` bytes)
  - **Praha** `europe/czech-republic/praha` — pack **182 MB** (`190448044` bytes)
  - **Středočeský kraj (with Praha)** `europe/czech-republic/stredocesky` — pack **875 MB** (`917053065` bytes)
  - **Ústecký kraj** `europe/czech-republic/ustecky` — pack **392 MB** (`410543016` bytes)
  - **Kraj Vysočina** `europe/czech-republic/vysocina` — pack **210 MB** (`220044712` bytes)
  - **Zlínský kraj** `europe/czech-republic/zlinsky` — pack **189 MB** (`198019521` bytes)

## DACH

- Continent / group: Europe
- Region id: `europe/dach`
- Extract type: multi-country Geofabrik extract — **Germany + Austria + Switzerland** (not a single country)
- Country-level pack: yes
- Geofabrik country PBF size: **5.80 GB** (`6223361554` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/dach.html
- Pack-server country pack size: **35.0 GB** (`37580712030` bytes)
- Subregions: none

## Denmark

- Continent / group: Europe
- Region id: `europe/denmark`
- Country-level pack: yes
- Geofabrik country PBF size: **471 MB** (`494296003` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/denmark.html
- Pack-server country pack size: **2.71 GB** (`2911522059` bytes)
- Subregions: none

## Djibouti

- Continent / group: Africa
- Region id: `africa/djibouti`
- Country-level pack: yes
- Geofabrik country PBF size: **6.69 MB** (`7014242` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/djibouti.html
- Pack-server country pack size: **36.8 MB** (`38580350` bytes)
- Subregions: none

## East Timor

- Continent / group: Asia
- Region id: `asia/east-timor`
- Country-level pack: yes
- Geofabrik country PBF size: **16.9 MB** (`17764008` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/east-timor.html
- Pack-server country pack size: **81.3 MB** (`85248717` bytes)
- Subregions: none

## Ecuador

- Continent / group: South America
- Region id: `south-america/ecuador`
- Country-level pack: yes
- Geofabrik country PBF size: **120 MB** (`125400889` bytes)
- Geofabrik URL: https://download.geofabrik.de/south-america/ecuador.html
- Pack-server country pack size: **1.32 GB** (`1418630354` bytes)
- Subregions: none

## Egypt

- Continent / group: Africa
- Region id: `africa/egypt`
- Country-level pack: yes
- Geofabrik country PBF size: **170 MB** (`178264977` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/egypt.html
- Pack-server country pack size: **3.94 GB** (`4226310731` bytes)
- Subregions: none

## El Salvador

- Continent / group: Central America
- Region id: `central-america/el-salvador`
- Country-level pack: yes
- Geofabrik country PBF size: **33.4 MB** (`35035832` bytes)
- Geofabrik URL: https://download.geofabrik.de/central-america/el-salvador.html
- Pack-server country pack size: **347 MB** (`363828841` bytes)
- Subregions: none

## Equatorial Guinea

- Continent / group: Africa
- Region id: `africa/equatorial-guinea`
- Country-level pack: yes
- Geofabrik country PBF size: **6.23 MB** (`6533009` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/equatorial-guinea.html
- Pack-server country pack size: **38.6 MB** (`40512408` bytes)
- Subregions: none

## Eritrea

- Continent / group: Africa
- Region id: `africa/eritrea`
- Country-level pack: yes
- Geofabrik country PBF size: **30.0 MB** (`31433896` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/eritrea.html
- Pack-server country pack size: **174 MB** (`182132054` bytes)
- Subregions: none

## Estonia

- Continent / group: Europe
- Region id: `europe/estonia`
- Country-level pack: yes
- Geofabrik country PBF size: **117 MB** (`122974116` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/estonia.html
- Pack-server country pack size: **779 MB** (`816725889` bytes)
- Subregions: none

## Ethiopia

- Continent / group: Africa
- Region id: `africa/ethiopia`
- Country-level pack: yes
- Geofabrik country PBF size: **133 MB** (`139703549` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/ethiopia.html
- Pack-server country pack size: **1.57 GB** (`1680979851` bytes)
- Subregions: none

## Faroe Islands

- Continent / group: Europe
- Region id: `europe/faroe-islands`
- Country-level pack: yes
- Geofabrik country PBF size: **7.37 MB** (`7732459` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/faroe-islands.html
- Pack-server country pack size: **48.8 MB** (`51178689` bytes)
- Subregions: none

## Fiji

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/fiji`
- Country-level pack: yes
- Geofabrik country PBF size: **16.4 MB** (`17236331` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/fiji.html
- Pack-server country pack size: **101 MB** (`106024881` bytes)
- Subregions: none

## Finland

- Continent / group: Europe
- Region id: `europe/finland`
- Country-level pack: yes
- Geofabrik country PBF size: **730 MB** (`765965512` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/finland.html
- Pack-server country pack size: **4.91 GB** (`5269617746` bytes)
- Subregions: none

## France

- Continent / group: Europe
- Region id: `europe/france`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **4.73 GB** (`5080654925` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/france.html
- Subregions (sizes from pack server `current.json`):
  - **Alsace** `europe/france/alsace` — pack **614 MB** (`643650913` bytes)
  - **Aquitaine** `europe/france/aquitaine` — pack **1.56 GB** (`1672138238` bytes)
  - **Auvergne** `europe/france/auvergne` — pack **947 MB** (`993094883` bytes)
  - **Basse-Normandie** `europe/france/basse-normandie` — pack **747 MB** (`783652570` bytes)
  - **Bourgogne** `europe/france/bourgogne` — pack **949 MB** (`995400162` bytes)
  - **Bretagne** `europe/france/bretagne` — pack **1.53 GB** (`1639137511` bytes)
  - **Centre** `europe/france/centre` — pack **1.16 GB** (`1246871290` bytes)
  - **Champagne Ardenne** `europe/france/champagne-ardenne` — pack **632 MB** (`663120158` bytes)
  - **Corse** `europe/france/corse` — pack **162 MB** (`169661255` bytes)
  - **Franche Comte** `europe/france/franche-comte` — pack **652 MB** (`683971390` bytes)
  - **Guadeloupe** `europe/france/guadeloupe` — pack **95.0 MB** (`99585249` bytes)
  - **Guyane** `europe/france/guyane` — pack **58.0 MB** (`60863760` bytes)
  - **Haute-Normandie** `europe/france/haute-normandie` — pack **521 MB** (`546458516` bytes)
  - **Ile-de-France** `europe/france/ile-de-france` — pack **1.31 GB** (`1411292406` bytes)
  - **Languedoc-Roussillon** `europe/france/languedoc-roussillon` — pack **1.48 GB** (`1593146440` bytes)
  - **Limousin** `europe/france/limousin` — pack **519 MB** (`543782549` bytes)
  - **Lorraine** `europe/france/lorraine` — pack **901 MB** (`945090759` bytes)
  - **Martinique** `europe/france/martinique` — pack **85.0 MB** (`89108765` bytes)
  - **Mayotte** `europe/france/mayotte` — pack **31.1 MB** (`32643297` bytes)
  - **Midi-Pyrenees** `europe/france/midi-pyrenees` — pack **1.86 GB** (`1998964044` bytes)
  - **Nord-Pas-de-Calais** `europe/france/nord-pas-de-calais` — pack **876 MB** (`918433959` bytes)
  - **Pays de la Loire** `europe/france/pays-de-la-loire` — pack **1.59 GB** (`1704523241` bytes)
  - **Picardie** `europe/france/picardie` — pack **604 MB** (`633786168` bytes)
  - **Poitou-Charentes** `europe/france/poitou-charentes` — pack **1.00 GB** (`1075613534` bytes)
  - **Provence Alpes-Cote-d'Azur** `europe/france/provence-alpes-cote-d-azur` — pack **1.81 GB** (`1938955745` bytes)
  - **Reunion** `europe/france/reunion` — pack **133 MB** (`139770565` bytes)
  - **Rhone-Alpes** `europe/france/rhone-alpes` — pack **2.78 GB** (`2988154880` bytes)

## Gabon

- Continent / group: Africa
- Region id: `africa/gabon`
- Country-level pack: yes
- Geofabrik country PBF size: **24.3 MB** (`25442789` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/gabon.html
- Pack-server country pack size: **165 MB** (`172554069` bytes)
- Subregions: none

## GCC States

- Continent / group: Asia
- Region id: `asia/gcc-states`
- Extract type: multi-country Geofabrik extract — Gulf Cooperation Council (**Bahrain, Kuwait, Oman, Qatar, UAE**; not a single country)
- Country-level pack: yes
- Geofabrik country PBF size: **241 MB** (`253036510` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/gcc-states.html
- Pack-server country pack size: **4.11 GB** (`4408141425` bytes)
- Subregions: none

## Georgia

- Continent / group: Europe
- Region id: `europe/georgia`
- Country-level pack: yes
- Geofabrik country PBF size: **97.0 MB** (`101758587` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/georgia.html
- Pack-server country pack size: **903 MB** (`947237472` bytes)
- Subregions: none

## Germany

- Continent / group: Europe
- Region id: `europe/germany`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **4.51 GB** (`4838703123` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/germany.html
- Subregions (sizes from pack server `current.json`):
  - **Baden-Württemberg** `europe/germany/baden-wuerttemberg`
    - **Freiburg Regbez** `europe/germany/baden-wuerttemberg/freiburg-regbez` — pack **1013 MB** (`1062723128` bytes)
    - **Karlsruhe Regbez** `europe/germany/baden-wuerttemberg/karlsruhe-regbez` — pack **934 MB** (`979091618` bytes)
    - **Stuttgart Regbez** `europe/germany/baden-wuerttemberg/stuttgart-regbez` — pack **1.27 GB** (`1361256288` bytes)
    - **Tübingen Regbez** `europe/germany/baden-wuerttemberg/tuebingen-regbez` — pack **826 MB** (`866445412` bytes)
  - **Bayern** `europe/germany/bayern`
    - **Mittelfranken** `europe/germany/bayern/mittelfranken` — pack **555 MB** (`582313245` bytes)
    - **Niederbayern** `europe/germany/bayern/niederbayern` — pack **640 MB** (`670663064` bytes)
    - **Oberbayern** `europe/germany/bayern/oberbayern` — pack **1.66 GB** (`1787680026` bytes)
    - **Oberfranken** `europe/germany/bayern/oberfranken` — pack **508 MB** (`532846002` bytes)
    - **Oberpfalz** `europe/germany/bayern/oberpfalz` — pack **668 MB** (`700434956` bytes)
    - **Schwaben** `europe/germany/bayern/schwaben` — pack **763 MB** (`800028663` bytes)
    - **Unterfranken** `europe/germany/bayern/unterfranken` — pack **734 MB** (`769417104` bytes)
  - **Berlin** `europe/germany/berlin` — pack **478 MB** (`500899958` bytes)
  - **Brandenburg (mit Berlin)** `europe/germany/brandenburg` — pack **1.57 GB** (`1689778319` bytes)
  - **Bremen** `europe/germany/bremen` — pack **85.2 MB** (`89358890` bytes)
  - **Hamburg** `europe/germany/hamburg` — pack **230 MB** (`241309724` bytes)
  - **Hessen** `europe/germany/hessen` — pack **2.19 GB** (`2348999771` bytes)
  - **Mecklenburg-Vorpommern** `europe/germany/mecklenburg-vorpommern` — pack **620 MB** (`649676935` bytes)
  - **Niedersachsen (mit Bremen)** `europe/germany/niedersachsen` — pack **2.76 GB** (`2960864485` bytes)
  - **Nordrhein-Westfalen** `europe/germany/nordrhein-westfalen`
    - **Arnsberg Regbez** `europe/germany/nordrhein-westfalen/arnsberg-regbez` — pack **958 MB** (`1004449227` bytes)
    - **Detmold Regbez** `europe/germany/nordrhein-westfalen/detmold-regbez` — pack **762 MB** (`799029762` bytes)
    - **Düsseldorf Regbez** `europe/germany/nordrhein-westfalen/duesseldorf-regbez` — pack **861 MB** (`903035123` bytes)
    - **Köln Regbez** `europe/germany/nordrhein-westfalen/koeln-regbez` — pack **893 MB** (`935945162` bytes)
    - **Münster Regbez** `europe/germany/nordrhein-westfalen/muenster-regbez` — pack **583 MB** (`611556771` bytes)
  - **Rheinland-Pfalz** `europe/germany/rheinland-pfalz` — pack **1.68 GB** (`1806549675` bytes)
  - **Saarland** `europe/germany/saarland` — pack **220 MB** (`230946941` bytes)
  - **Sachsen** `europe/germany/sachsen` — pack **1.57 GB** (`1683222015` bytes)
  - **Sachsen-Anhalt** `europe/germany/sachsen-anhalt` — pack **909 MB** (`953388862` bytes)
  - **Schleswig-Holstein** `europe/germany/schleswig-holstein` — pack **803 MB** (`842356763` bytes)
  - **Thüringen** `europe/germany/thueringen` — pack **1.01 GB** (`1086691810` bytes)

## Ghana

- Continent / group: Africa
- Region id: `africa/ghana`
- Country-level pack: yes
- Geofabrik country PBF size: **110 MB** (`115665903` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/ghana.html
- Pack-server country pack size: **1.14 GB** (`1223449917` bytes)
- Subregions: none

## Great Britain

- Continent / group: Europe
- Region id: `europe/great-britain`
- Extract type: regional Geofabrik extract — England, Scotland, and Wales (not the full United Kingdom)
- Country-level pack: yes
- Geofabrik country PBF size: **2.02 GB** (`2172902136` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/great-britain.html
- Pack-server country pack size: **13.0 GB** (`13964022075` bytes)
- Subregions: none

## Greece

- Continent / group: Europe
- Region id: `europe/greece`
- Country-level pack: yes
- Geofabrik country PBF size: **325 MB** (`340654274` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/greece.html
- Pack-server country pack size: **4.05 GB** (`4343427946` bytes)
- Subregions: none

## Greenland

- Continent / group: North America
- Region id: `north-america/greenland`
- Country-level pack: yes
- Geofabrik country PBF size: **24.9 MB** (`26109358` bytes)
- Geofabrik URL: https://download.geofabrik.de/north-america/greenland.html
- Pack-server country pack size: **39.6 MB** (`41554237` bytes)
- Subregions: none

## Guatemala

- Continent / group: Central America
- Region id: `central-america/guatemala`
- Country-level pack: yes
- Geofabrik country PBF size: **125 MB** (`131365796` bytes)
- Geofabrik URL: https://download.geofabrik.de/central-america/guatemala.html
- Pack-server country pack size: **1004 MB** (`1052693339` bytes)
- Subregions: none

## Guernsey and Jersey

- Continent / group: Europe
- Region id: `europe/guernsey-jersey`
- Extract type: Channel Islands extract — Guernsey + Jersey
- Country-level pack: yes
- Geofabrik country PBF size: **3.71 MB** (`3891660` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/guernsey-jersey.html
- Pack-server country pack size: **35.3 MB** (`36973010` bytes)
- Subregions: none

## Guinea

- Continent / group: Africa
- Region id: `africa/guinea`
- Country-level pack: yes
- Geofabrik country PBF size: **113 MB** (`118302216` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/guinea.html
- Pack-server country pack size: **959 MB** (`1006094474` bytes)
- Subregions: none

## Guinea-Bissau

- Continent / group: Africa
- Region id: `africa/guinea-bissau`
- Country-level pack: yes
- Geofabrik country PBF size: **10.6 MB** (`11153187` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/guinea-bissau.html
- Pack-server country pack size: **94.2 MB** (`98776578` bytes)
- Subregions: none

## Guyana

- Continent / group: South America
- Region id: `south-america/guyana`
- Country-level pack: yes
- Geofabrik country PBF size: **14.8 MB** (`15534001` bytes)
- Geofabrik URL: https://download.geofabrik.de/south-america/guyana.html
- Pack-server country pack size: **69.1 MB** (`72422167` bytes)
- Subregions: none

## Haiti and Dominican Republic

- Continent / group: Central America
- Region id: `central-america/haiti-and-domrep`
- Extract type: multi-country Geofabrik extract — Haiti + Dominican Republic (Hispaniola)
- Country-level pack: yes
- Geofabrik country PBF size: **84.5 MB** (`88640558` bytes)
- Geofabrik URL: https://download.geofabrik.de/central-america/haiti-and-domrep.html
- Pack-server country pack size: **882 MB** (`924879147` bytes)
- Subregions: none

## Honduras

- Continent / group: Central America
- Region id: `central-america/honduras`
- Country-level pack: yes
- Geofabrik country PBF size: **70.6 MB** (`74073740` bytes)
- Geofabrik URL: https://download.geofabrik.de/central-america/honduras.html
- Pack-server country pack size: **642 MB** (`673194146` bytes)
- Subregions: none

## Hungary

- Continent / group: Europe
- Region id: `europe/hungary`
- Country-level pack: yes
- Geofabrik country PBF size: **309 MB** (`324433812` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/hungary.html
- Pack-server country pack size: **2.22 GB** (`2387222120` bytes)
- Subregions: none

## Iceland

- Continent / group: Europe
- Region id: `europe/iceland`
- Country-level pack: yes
- Geofabrik country PBF size: **61.7 MB** (`64747825` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/iceland.html
- Pack-server country pack size: **308 MB** (`322443224` bytes)
- Subregions: none

## India

- Continent / group: Asia
- Region id: `asia/india`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **1.59 GB** (`1706613478` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/india.html
- Subregions (sizes from pack server `current.json`):
  - **Central Zone** `asia/india/central-zone` — pack **6.58 GB** (`7060367449` bytes)
  - **Eastern Zone** `asia/india/eastern-zone` — pack **4.86 GB** (`5219680319` bytes)
  - **North-Eastern Zone** `asia/india/north-eastern-zone` — pack **1.17 GB** (`1250934320` bytes)
  - **Northern Zone** `asia/india/northern-zone` — pack **5.65 GB** (`6064809051` bytes)
  - **Southern Zone** `asia/india/southern-zone` — pack **9.58 GB** (`10286620911` bytes)
  - **Western Zone** `asia/india/western-zone` — pack **4.23 GB** (`4539035125` bytes)

## Indonesia (with East Timor)

- Continent / group: Asia
- Region id: `asia/indonesia`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **1.62 GB** (`1736135938` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/indonesia.html
- Subregions (sizes from pack server `current.json`):
  - **Java** `asia/indonesia/java` — pack **7.60 GB** (`8165203070` bytes)
  - **Kalimantan** `asia/indonesia/kalimantan` — pack **1.65 GB** (`1776698085` bytes)
  - **Maluku** `asia/indonesia/maluku` — pack **177 MB** (`185649866` bytes)
  - **Nusa-Tenggara** `asia/indonesia/nusa-tenggara` — pack **1019 MB** (`1067979212` bytes)
  - **Papua** `asia/indonesia/papua` — pack **247 MB** (`258917785` bytes)
  - **Sulawesi** `asia/indonesia/sulawesi` — pack **1.18 GB** (`1263857172` bytes)
  - **Sumatra** `asia/indonesia/sumatra` — pack **3.10 GB** (`3324657826` bytes)

## Iran

- Continent / group: Asia
- Region id: `asia/iran`
- Country-level pack: yes
- Geofabrik country PBF size: **219 MB** (`229379748` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/iran.html
- Pack-server country pack size: **5.13 GB** (`5506869543` bytes)
- Subregions: none

## Iraq

- Continent / group: Asia
- Region id: `asia/iraq`
- Country-level pack: yes
- Geofabrik country PBF size: **86.2 MB** (`90360799` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/iraq.html
- Pack-server country pack size: **1.88 GB** (`2021305210` bytes)
- Subregions: none

## Ireland and Northern Ireland

- Continent / group: Europe
- Region id: `europe/ireland-and-northern-ireland`
- Country-level pack: yes
- Geofabrik country PBF size: **394 MB** (`412715072` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/ireland-and-northern-ireland.html
- Pack-server country pack size: **2.98 GB** (`3204035852` bytes)
- Subregions: none

## Isle of Man

- Continent / group: Europe
- Region id: `europe/isle-of-man`
- Extract type: Crown dependency extract
- Country-level pack: yes
- Geofabrik country PBF size: **5.78 MB** (`6064173` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/isle-of-man.html
- Pack-server country pack size: **31.1 MB** (`32649016` bytes)
- Subregions: none

## Israel and Palestine

- Continent / group: Asia
- Region id: `asia/israel-and-palestine`
- Extract type: combined Geofabrik extract — Israel and Palestine
- Country-level pack: yes
- Geofabrik country PBF size: **114 MB** (`119528881` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/israel-and-palestine.html
- Pack-server country pack size: **1.06 GB** (`1133939956` bytes)
- Subregions: none

## Italy

- Continent / group: Europe
- Region id: `europe/italy`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **2.08 GB** (`2229846341` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/italy.html
- Subregions (sizes from pack server `current.json`):
  - **Centro** `europe/italy/centro` — pack **2.60 GB** (`2788111953` bytes)
  - **Isole** `europe/italy/isole` — pack **1.74 GB** (`1864934653` bytes)
  - **Nord-Est** `europe/italy/nord-est` — pack **3.22 GB** (`3458065502` bytes)
  - **Nord-Ovest** `europe/italy/nord-ovest` — pack **3.57 GB** (`3829159334` bytes)
  - **Sud** `europe/italy/sud` — pack **2.69 GB** (`2890128928` bytes)

## Ivory Coast

- Continent / group: Africa
- Region id: `africa/ivory-coast`
- Country-level pack: yes
- Geofabrik country PBF size: **81.4 MB** (`85393416` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/ivory-coast.html
- Pack-server country pack size: **733 MB** (`768661267` bytes)
- Subregions: none

## Jamaica

- Continent / group: Central America
- Region id: `central-america/jamaica`
- Country-level pack: yes
- Geofabrik country PBF size: **36.8 MB** (`38603362` bytes)
- Geofabrik URL: https://download.geofabrik.de/central-america/jamaica.html
- Pack-server country pack size: **191 MB** (`200234864` bytes)
- Subregions: none

## Japan

- Continent / group: Asia
- Region id: `asia/japan`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **2.35 GB** (`2521119729` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/japan.html
- Subregions (sizes from pack server `current.json`):
  - **Chūbu region** `asia/japan/chubu` — pack **4.35 GB** (`4674844060` bytes)
  - **Chūgoku region** `asia/japan/chugoku` — pack **1.80 GB** (`1928737907` bytes)
  - **Hokkaidō** `asia/japan/hokkaido` — pack **1.24 GB** (`1331050653` bytes)
  - **Kansai region (a.k.a. Kinki region)** `asia/japan/kansai` — pack **3.03 GB** (`3251896178` bytes)
  - **Kantō region** `asia/japan/kanto` — pack **5.07 GB** (`5440630351` bytes)
  - **Kyūshū** `asia/japan/kyushu` — pack **3.13 GB** (`3360326775` bytes)
  - **Shikoku** `asia/japan/shikoku` — pack **1010 MB** (`1059277391` bytes)
  - **Tōhoku region** `asia/japan/tohoku` — pack **2.44 GB** (`2615795447` bytes)

## Jordan

- Continent / group: Asia
- Region id: `asia/jordan`
- Country-level pack: yes
- Geofabrik country PBF size: **29.6 MB** (`31021515` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/jordan.html
- Pack-server country pack size: **538 MB** (`564577551` bytes)
- Subregions: none

## Kazakhstan

- Continent / group: Asia
- Region id: `asia/kazakhstan`
- Country-level pack: yes
- Geofabrik country PBF size: **213 MB** (`222842916` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/kazakhstan.html
- Pack-server country pack size: **1.88 GB** (`2014964460` bytes)
- Subregions: none

## Kenya

- Continent / group: Africa
- Region id: `africa/kenya`
- Country-level pack: yes
- Geofabrik country PBF size: **334 MB** (`350308008` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/kenya.html
- Pack-server country pack size: **2.43 GB** (`2612879218` bytes)
- Subregions: none

## Kiribati

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/kiribati`
- Country-level pack: yes
- Geofabrik country PBF size: **2.33 MB** (`2447120` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/kiribati.html
- Pack-server country pack size: **6.28 MB** (`6586042` bytes)
- Subregions: none

## Kosovo

- Continent / group: Europe
- Region id: `europe/kosovo`
- Extract type: territory extract (disputed status; own Geofabrik leaf)
- Country-level pack: yes
- Geofabrik country PBF size: **29.3 MB** (`30749699` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/kosovo.html
- Pack-server country pack size: **312 MB** (`327052577` bytes)
- Subregions: none

## Kyrgyzstan

- Continent / group: Asia
- Region id: `asia/kyrgyzstan`
- Country-level pack: yes
- Geofabrik country PBF size: **71.3 MB** (`74730938` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/kyrgyzstan.html
- Pack-server country pack size: **422 MB** (`442456544` bytes)
- Subregions: none

## Laos

- Continent / group: Asia
- Region id: `asia/laos`
- Country-level pack: yes
- Geofabrik country PBF size: **51.1 MB** (`53602656` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/laos.html
- Pack-server country pack size: **579 MB** (`606804851` bytes)
- Subregions: none

## Latvia

- Continent / group: Europe
- Region id: `europe/latvia`
- Country-level pack: yes
- Geofabrik country PBF size: **134 MB** (`140284227` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/latvia.html
- Pack-server country pack size: **1.01 GB** (`1085602180` bytes)
- Subregions: none

## Lebanon

- Continent / group: Asia
- Region id: `asia/lebanon`
- Country-level pack: yes
- Geofabrik country PBF size: **50.1 MB** (`52486815` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/lebanon.html
- Pack-server country pack size: **544 MB** (`570505785` bytes)
- Subregions: none

## Lesotho

- Continent / group: Africa
- Region id: `africa/lesotho`
- Country-level pack: yes
- Geofabrik country PBF size: **121 MB** (`126886153` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/lesotho.html
- Pack-server country pack size: **358 MB** (`375492675` bytes)
- Subregions: none

## Liberia

- Continent / group: Africa
- Region id: `africa/liberia`
- Country-level pack: yes
- Geofabrik country PBF size: **35.6 MB** (`37296801` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/liberia.html
- Pack-server country pack size: **200 MB** (`209358673` bytes)
- Subregions: none

## Libya

- Continent / group: Africa
- Region id: `africa/libya`
- Country-level pack: yes
- Geofabrik country PBF size: **73.0 MB** (`76579223` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/libya.html
- Pack-server country pack size: **919 MB** (`963764090` bytes)
- Subregions: none

## Liechtenstein

- Continent / group: Europe
- Region id: `europe/liechtenstein`
- Country-level pack: yes
- Geofabrik country PBF size: **3.29 MB** (`3454917` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/liechtenstein.html
- Pack-server country pack size: **20.5 MB** (`21528243` bytes)
- Subregions: none

## Lithuania

- Continent / group: Europe
- Region id: `europe/lithuania`
- Country-level pack: yes
- Geofabrik country PBF size: **212 MB** (`222669329` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/lithuania.html
- Pack-server country pack size: **1.24 GB** (`1332869596` bytes)
- Subregions: none

## Luxembourg

- Continent / group: Europe
- Region id: `europe/luxembourg`
- Country-level pack: yes
- Geofabrik country PBF size: **45.3 MB** (`47505705` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/luxembourg.html
- Pack-server country pack size: **189 MB** (`198471215` bytes)
- Subregions: none

## Macedonia

- Continent / group: Europe
- Region id: `europe/macedonia`
- Country-level pack: yes
- Geofabrik country PBF size: **28.3 MB** (`29686418` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/macedonia.html
- Pack-server country pack size: **264 MB** (`276662941` bytes)
- Subregions: none

## Madagascar

- Continent / group: Africa
- Region id: `africa/madagascar`
- Country-level pack: yes
- Geofabrik country PBF size: **371 MB** (`388782270` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/madagascar.html
- Pack-server country pack size: **2.81 GB** (`3014318116` bytes)
- Subregions: none

## Malawi

- Continent / group: Africa
- Region id: `africa/malawi`
- Country-level pack: yes
- Geofabrik country PBF size: **148 MB** (`154879121` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/malawi.html
- Pack-server country pack size: **1.38 GB** (`1478179631` bytes)
- Subregions: none

## Malaysia, Singapore, and Brunei

- Continent / group: Asia
- Region id: `asia/malaysia-singapore-brunei`
- Extract type: multi-country Geofabrik extract — Malaysia + Singapore + Brunei
- Country-level pack: yes
- Geofabrik country PBF size: **239 MB** (`250814575` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/malaysia-singapore-brunei.html
- Pack-server country pack size: **4.09 GB** (`4396914504` bytes)
- Subregions: none

## Maldives

- Continent / group: Asia
- Region id: `asia/maldives`
- Country-level pack: yes
- Geofabrik country PBF size: **5.00 MB** (`5246302` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/maldives.html
- Pack-server country pack size: **34.9 MB** (`36610054` bytes)
- Subregions: none

## Mali

- Continent / group: Africa
- Region id: `africa/mali`
- Country-level pack: yes
- Geofabrik country PBF size: **165 MB** (`173491550` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/mali.html
- Pack-server country pack size: **1.42 GB** (`1520089661` bytes)
- Subregions: none

## Malta

- Continent / group: Europe
- Region id: `europe/malta`
- Country-level pack: yes
- Geofabrik country PBF size: **8.50 MB** (`8914563` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/malta.html
- Pack-server country pack size: **57.4 MB** (`60148410` bytes)
- Subregions: none

## Marshall Islands

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/marshall-islands`
- Country-level pack: yes
- Geofabrik country PBF size: **2.11 MB** (`2215813` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/marshall-islands.html
- Pack-server country pack size: **5.37 MB** (`5635184` bytes)
- Subregions: none

## Mauritania

- Continent / group: Africa
- Region id: `africa/mauritania`
- Country-level pack: yes
- Geofabrik country PBF size: **29.1 MB** (`30560612` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/mauritania.html
- Pack-server country pack size: **271 MB** (`284619333` bytes)
- Subregions: none

## Mauritius

- Continent / group: Africa
- Region id: `africa/mauritius`
- Country-level pack: yes
- Geofabrik country PBF size: **8.88 MB** (`9309357` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/mauritius.html
- Pack-server country pack size: **109 MB** (`114712452` bytes)
- Subregions: none

## Mexico

- Continent / group: North America
- Region id: `north-america/mexico`
- Country-level pack: yes
- Geofabrik country PBF size: **616 MB** (`645500372` bytes)
- Geofabrik URL: https://download.geofabrik.de/north-america/mexico.html
- Pack-server country pack size: **11.2 GB** (`12044557939` bytes)
- Subregions: none

## Micronesia

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/micronesia`
- Country-level pack: yes
- Geofabrik country PBF size: **1.89 MB** (`1985237` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/micronesia.html
- Pack-server country pack size: **4.72 MB** (`4947140` bytes)
- Subregions: none

## Moldova

- Continent / group: Europe
- Region id: `europe/moldova`
- Country-level pack: yes
- Geofabrik country PBF size: **96.4 MB** (`101114173` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/moldova.html
- Pack-server country pack size: **812 MB** (`851394109` bytes)
- Subregions: none

## Monaco

- Continent / group: Europe
- Region id: `europe/monaco`
- Country-level pack: yes
- Geofabrik country PBF size: **675 KB** (`691365` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/monaco.html
- Pack-server country pack size: **3.34 MB** (`3506936` bytes)
- Subregions: none

## Mongolia

- Continent / group: Asia
- Region id: `asia/mongolia`
- Country-level pack: yes
- Geofabrik country PBF size: **59.2 MB** (`62041978` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/mongolia.html
- Pack-server country pack size: **490 MB** (`513644432` bytes)
- Subregions: none

## Montenegro

- Continent / group: Europe
- Region id: `europe/montenegro`
- Country-level pack: yes
- Geofabrik country PBF size: **32.8 MB** (`34383509` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/montenegro.html
- Pack-server country pack size: **246 MB** (`258032064` bytes)
- Subregions: none

## Morocco

- Continent / group: Africa
- Region id: `africa/morocco`
- Country-level pack: yes
- Geofabrik country PBF size: **232 MB** (`243386639` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/morocco.html
- Pack-server country pack size: **3.12 GB** (`3350150311` bytes)
- Subregions: none

## Mozambique

- Continent / group: Africa
- Region id: `africa/mozambique`
- Country-level pack: yes
- Geofabrik country PBF size: **244 MB** (`255399199` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/mozambique.html
- Pack-server country pack size: **1.83 GB** (`1966484576` bytes)
- Subregions: none

## Myanmar (a.k.a. Burma)

- Continent / group: Asia
- Region id: `asia/myanmar`
- Country-level pack: yes
- Geofabrik country PBF size: **269 MB** (`282062139` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/myanmar.html
- Pack-server country pack size: **3.21 GB** (`3448458136` bytes)
- Subregions: none

## Namibia

- Continent / group: Africa
- Region id: `africa/namibia`
- Country-level pack: yes
- Geofabrik country PBF size: **52.0 MB** (`54517641` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/namibia.html
- Pack-server country pack size: **388 MB** (`406969116` bytes)
- Subregions: none

## Nauru

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/nauru`
- Country-level pack: yes
- Geofabrik country PBF size: **260 KB** (`266514` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/nauru.html
- Pack-server country pack size: **1.10 MB** (`1157642` bytes)
- Subregions: none

## Nepal

- Continent / group: Asia
- Region id: `asia/nepal`
- Country-level pack: yes
- Geofabrik country PBF size: **395 MB** (`413858325` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/nepal.html
- Pack-server country pack size: **1.87 GB** (`2011243998` bytes)
- Subregions: none

## Netherlands

- Continent / group: Europe
- Region id: `europe/netherlands`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **1.31 GB** (`1401611763` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/netherlands.html
- Subregions (sizes from pack server `current.json`):
  - **Drenthe** `europe/netherlands/drenthe` — pack **186 MB** (`195031401` bytes)
  - **Flevoland** `europe/netherlands/flevoland` — pack **120 MB** (`125362909` bytes)
  - **Friesland** `europe/netherlands/friesland` — pack **230 MB** (`241100051` bytes)
  - **Gelderland** `europe/netherlands/gelderland` — pack **542 MB** (`568754690` bytes)
  - **Groningen** `europe/netherlands/groningen` — pack **140 MB** (`146837046` bytes)
  - **Limburg** `europe/netherlands/limburg` — pack **249 MB** (`261533222` bytes)
  - **Noord-Brabant** `europe/netherlands/noord-brabant` — pack **523 MB** (`547901538` bytes)
  - **Noord-Holland** `europe/netherlands/noord-holland` — pack **450 MB** (`471599183` bytes)
  - **Overijssel** `europe/netherlands/overijssel` — pack **310 MB** (`325312044` bytes)
  - **Utrecht** `europe/netherlands/utrecht` — pack **223 MB** (`233984857` bytes)
  - **Zeeland** `europe/netherlands/zeeland` — pack **121 MB** (`127001852` bytes)
  - **Zuid-Holland** `europe/netherlands/zuid-holland` — pack **542 MB** (`568006454` bytes)

## New Caledonia

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/new-caledonia`
- Country-level pack: yes
- Geofabrik country PBF size: **13.5 MB** (`14196225` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/new-caledonia.html
- Pack-server country pack size: **129 MB** (`134820148` bytes)
- Subregions: none

## New Zealand

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/new-zealand`
- Country-level pack: yes
- Geofabrik country PBF size: **384 MB** (`403157666` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/new-zealand.html
- Pack-server country pack size: **1.67 GB** (`1795618584` bytes)
- Subregions: none

## Nicaragua

- Continent / group: Central America
- Region id: `central-america/nicaragua`
- Country-level pack: yes
- Geofabrik country PBF size: **58.5 MB** (`61375896` bytes)
- Geofabrik URL: https://download.geofabrik.de/central-america/nicaragua.html
- Pack-server country pack size: **349 MB** (`366043485` bytes)
- Subregions: none

## Niger

- Continent / group: Africa
- Region id: `africa/niger`
- Country-level pack: yes
- Geofabrik country PBF size: **74.1 MB** (`77719644` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/niger.html
- Pack-server country pack size: **783 MB** (`820895100` bytes)
- Subregions: none

## Nigeria

- Continent / group: Africa
- Region id: `africa/nigeria`
- Country-level pack: yes
- Geofabrik country PBF size: **675 MB** (`708071377` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/nigeria.html
- Pack-server country pack size: **5.64 GB** (`6057776048` bytes)
- Subregions: none

## Niue

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/niue`
- Country-level pack: yes
- Geofabrik country PBF size: **415 KB** (`424756` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/niue.html
- Pack-server country pack size: **2.20 MB** (`2303300` bytes)
- Subregions: none

## North Korea

- Continent / group: Asia
- Region id: `asia/north-korea`
- Country-level pack: yes
- Geofabrik country PBF size: **87.7 MB** (`91940146` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/north-korea.html
- Pack-server country pack size: **750 MB** (`785977304` bytes)
- Subregions: none

## Norway

- Continent / group: Europe
- Region id: `europe/norway`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **1.28 GB** (`1374794406` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/norway.html
- Note: `europe/norway/hedmark` exists on the pack server but is omitted here; coverage is under **Østlandet**.
- Subregions (sizes from pack server `current.json`):
  - **Nord-Norge** `europe/norway/nord-norge` — pack **836 MB** (`876694995` bytes)
  - **Østlandet** `europe/norway/ostlandet` — pack **2.43 GB** (`2613668860` bytes)
  - **Sørlandet** `europe/norway/sorlandet` — pack **304 MB** (`319074305` bytes)
  - **Svalbard and Jan Mayen** `europe/norway/svalbard-janmayen` — pack **7.78 MB** (`8161385` bytes)
  - **Trøndelag** `europe/norway/trondelag` — pack **615 MB** (`645224257` bytes)
  - **Vestlandet** `europe/norway/vestlandet` — pack **1.11 GB** (`1191807281` bytes)

## Pakistan

- Continent / group: Asia
- Region id: `asia/pakistan`
- Country-level pack: yes
- Geofabrik country PBF size: **149 MB** (`156081366` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/pakistan.html
- Pack-server country pack size: **3.83 GB** (`4109963632` bytes)
- Subregions: none

## Palau

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/palau`
- Country-level pack: yes
- Geofabrik country PBF size: **801 KB** (`820588` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/palau.html
- Pack-server country pack size: **2.72 MB** (`2848313` bytes)
- Subregions: none

## Panama

- Continent / group: Central America
- Region id: `central-america/panama`
- Country-level pack: yes
- Geofabrik country PBF size: **34.5 MB** (`36189071` bytes)
- Geofabrik URL: https://download.geofabrik.de/central-america/panama.html
- Pack-server country pack size: **282 MB** (`295656580` bytes)
- Subregions: none

## Papua New Guinea

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/papua-new-guinea`
- Country-level pack: yes
- Geofabrik country PBF size: **51.6 MB** (`54136778` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/papua-new-guinea.html
- Pack-server country pack size: **363 MB** (`380493573` bytes)
- Subregions: none

## Paraguay

- Continent / group: South America
- Region id: `south-america/paraguay`
- Country-level pack: yes
- Geofabrik country PBF size: **147 MB** (`154314379` bytes)
- Geofabrik URL: https://download.geofabrik.de/south-america/paraguay.html
- Pack-server country pack size: **698 MB** (`732281906` bytes)
- Subregions: none

## Peru

- Continent / group: South America
- Region id: `south-america/peru`
- Country-level pack: yes
- Geofabrik country PBF size: **244 MB** (`255979484` bytes)
- Geofabrik URL: https://download.geofabrik.de/south-america/peru.html
- Pack-server country pack size: **2.83 GB** (`3037990728` bytes)
- Subregions: none

## Philippines

- Continent / group: Asia
- Region id: `asia/philippines`
- Country-level pack: yes
- Geofabrik country PBF size: **578 MB** (`606444025` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/philippines.html
- Pack-server country pack size: **3.81 GB** (`4093614361` bytes)
- Subregions: none

## Pitcairn Islands

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/pitcairn-islands`
- Country-level pack: yes
- Geofabrik country PBF size: **112 KB** (`115074` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/pitcairn-islands.html
- Pack-server country pack size: **272 KB** (`278055` bytes)
- Subregions: none

## Poland

- Continent / group: Europe
- Region id: `europe/poland`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **1.95 GB** (`2096077759` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/poland.html
- Subregions (sizes from pack server `current.json`):
  - **Dolnośląskie** `europe/poland/dolnoslaskie` — pack **1.01 GB** (`1080395074` bytes)
  - **Kujawsko-pomorskie** `europe/poland/kujawsko-pomorskie` — pack **717 MB** (`751618287` bytes)
  - **Łódzkie** `europe/poland/lodzkie` — pack **638 MB** (`668991635` bytes)
  - **Lubelskie** `europe/poland/lubelskie` — pack **960 MB** (`1006788756` bytes)
  - **Lubuskie** `europe/poland/lubuskie` — pack **430 MB** (`450572196` bytes)
  - **Małopolskie** `europe/poland/malopolskie` — pack **1.22 GB** (`1309734669` bytes)
  - **Mazowieckie** `europe/poland/mazowieckie` — pack **1.47 GB** (`1579135969` bytes)
  - **Opolskie** `europe/poland/opolskie` — pack **335 MB** (`351083948` bytes)
  - **Podkarpackie** `europe/poland/podkarpackie` — pack **917 MB** (`961291603` bytes)
  - **Podlaskie** `europe/poland/podlaskie` — pack **467 MB** (`489533060` bytes)
  - **Pomorskie** `europe/poland/pomorskie` — pack **791 MB** (`829315315` bytes)
  - **Śląskie** `europe/poland/slaskie` — pack **1.20 GB** (`1290263936` bytes)
  - **Świętokrzyskie** `europe/poland/swietokrzyskie` — pack **498 MB** (`522142422` bytes)
  - **Warmińsko-mazurskie** `europe/poland/warminsko-mazurskie` — pack **528 MB** (`553371105` bytes)
  - **Wielkopolskie** `europe/poland/wielkopolskie` — pack **902 MB** (`945311377` bytes)
  - **Zachodniopomorskie** `europe/poland/zachodniopomorskie` — pack **553 MB** (`579612198` bytes)

## Polynésie française (French Polynesia)

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/polynesie-francaise`
- Country-level pack: yes
- Geofabrik country PBF size: **14.9 MB** (`15647131` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/polynesie-francaise.html
- Pack-server country pack size: **76.1 MB** (`79746748` bytes)
- Subregions: none

## Portugal

- Continent / group: Europe
- Region id: `europe/portugal`
- Country-level pack: yes
- Geofabrik country PBF size: **403 MB** (`422846221` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/portugal.html
- Pack-server country pack size: **3.61 GB** (`3878348017` bytes)
- Subregions: none

## Romania

- Continent / group: Europe
- Region id: `europe/romania`
- Country-level pack: yes
- Geofabrik country PBF size: **313 MB** (`328004118` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/romania.html
- Pack-server country pack size: **2.55 GB** (`2733897337` bytes)
- Subregions: none

## Russian Federation

- Continent / group: Russia
- Region id: `russia`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **3.87 GB** (`4159357312` bytes)
- Geofabrik URL: https://download.geofabrik.de/russia.html
- Subregions (sizes from pack server `current.json`):
  - **Central Federal District** `russia/central-fed-district` — pack **5.73 GB** (`6154589322` bytes)
  - **Crimean Federal District** `russia/crimean-fed-district` — pack **369 MB** (`387386401` bytes)
  - **Far Eastern Federal District** `russia/far-eastern-fed-district` — pack **1.26 GB** (`1352891524` bytes)
  - **Kaliningrad** `russia/kaliningrad` — pack **139 MB** (`145877815` bytes)
  - **North Caucasus Federal District** `russia/north-caucasus-fed-district` — pack **877 MB** (`919197322` bytes)
  - **Northwestern Federal District** `russia/northwestern-fed-district` — pack **3.08 GB** (`3305507493` bytes)
  - **Siberian Federal District** `russia/siberian-fed-district` — pack **2.90 GB** (`3109786140` bytes)
  - **South Federal District** `russia/south-fed-district` — pack **1.98 GB** (`2121757770` bytes)
  - **Ural Federal District** `russia/ural-fed-district` — pack **1.96 GB** (`2103890128` bytes)
  - **Volga Federal District** `russia/volga-fed-district` — pack **4.29 GB** (`4611197190` bytes)

## Rwanda

- Continent / group: Africa
- Region id: `africa/rwanda`
- Country-level pack: yes
- Geofabrik country PBF size: **64.1 MB** (`67204284` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/rwanda.html
- Pack-server country pack size: **546 MB** (`572703968` bytes)
- Subregions: none

## Saint Helena, Ascension, and Tristan da Cunha

- Continent / group: Africa
- Region id: `africa/saint-helena-ascension-and-tristan-da-cunha`
- Country-level pack: yes
- Geofabrik country PBF size: **876 KB** (`896966` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/saint-helena-ascension-and-tristan-da-cunha.html
- Pack-server country pack size: **3.68 MB** (`3859962` bytes)
- Subregions: none

## Samoa

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/samoa`
- Country-level pack: yes
- Geofabrik country PBF size: **3.30 MB** (`3457578` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/samoa.html
- Pack-server country pack size: **14.9 MB** (`15573945` bytes)
- Subregions: none

## Sao Tome and Principe

- Continent / group: Africa
- Region id: `africa/sao-tome-and-principe`
- Country-level pack: yes
- Geofabrik country PBF size: **1.20 MB** (`1258312` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/sao-tome-and-principe.html
- Pack-server country pack size: **6.44 MB** (`6750835` bytes)
- Subregions: none

## Senegal and Gambia

- Continent / group: Africa
- Region id: `africa/senegal-and-gambia`
- Extract type: multi-country Geofabrik extract — Senegal + Gambia
- Country-level pack: yes
- Geofabrik country PBF size: **100 MB** (`105180489` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/senegal-and-gambia.html
- Pack-server country pack size: **1.45 GB** (`1557219648` bytes)
- Subregions: none

## Serbia

- Continent / group: Europe
- Region id: `europe/serbia`
- Country-level pack: yes
- Geofabrik country PBF size: **229 MB** (`239806025` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/serbia.html
- Pack-server country pack size: **1.60 GB** (`1715432807` bytes)
- Subregions: none

## Seychelles

- Continent / group: Africa
- Region id: `africa/seychelles`
- Country-level pack: yes
- Geofabrik country PBF size: **2.63 MB** (`2758453` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/seychelles.html
- Pack-server country pack size: **10.4 MB** (`10908742` bytes)
- Subregions: none

## Sierra Leone

- Continent / group: Africa
- Region id: `africa/sierra-leone`
- Country-level pack: yes
- Geofabrik country PBF size: **44.1 MB** (`46208822` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/sierra-leone.html
- Pack-server country pack size: **292 MB** (`305750351` bytes)
- Subregions: none

## Slovakia

- Continent / group: Europe
- Region id: `europe/slovakia`
- Country-level pack: yes
- Geofabrik country PBF size: **328 MB** (`344096172` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/slovakia.html
- Pack-server country pack size: **1.92 GB** (`2065259787` bytes)
- Subregions: none

## Slovenia

- Continent / group: Europe
- Region id: `europe/slovenia`
- Country-level pack: yes
- Geofabrik country PBF size: **298 MB** (`312812705` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/slovenia.html
- Pack-server country pack size: **1.15 GB** (`1229888388` bytes)
- Subregions: none

## Solomon Islands

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/solomon-islands`
- Country-level pack: yes
- Geofabrik country PBF size: **11.4 MB** (`11924996` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/solomon-islands.html
- Pack-server country pack size: **43.5 MB** (`45593493` bytes)
- Subregions: none

## Somalia

- Continent / group: Africa
- Region id: `africa/somalia`
- Country-level pack: yes
- Geofabrik country PBF size: **157 MB** (`164690335` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/somalia.html
- Pack-server country pack size: **1.52 GB** (`1629185256` bytes)
- Subregions: none

## South Africa

- Continent / group: Africa
- Region id: `africa/south-africa`
- Country-level pack: yes
- Geofabrik country PBF size: **401 MB** (`420141864` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/south-africa.html
- Pack-server country pack size: **5.72 GB** (`6143137093` bytes)
- Subregions: none

## South Africa (includes Lesotho)

- Continent / group: Africa
- Region id: `africa/south-africa-and-lesotho`
- Extract type: multi-country Geofabrik extract — South Africa including Lesotho
- Country-level pack: yes
- Geofabrik country PBF size: **520 MB** (`545052203` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/south-africa-and-lesotho.html
- Pack-server country pack size: **6.02 GB** (`6466072387` bytes)
- Subregions: none

## South Korea

- Continent / group: Asia
- Region id: `asia/south-korea`
- Country-level pack: yes
- Geofabrik country PBF size: **274 MB** (`287258221` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/south-korea.html
- Pack-server country pack size: **4.27 GB** (`4585970327` bytes)
- Subregions: none

## South Sudan

- Continent / group: Africa
- Region id: `africa/south-sudan`
- Country-level pack: yes
- Geofabrik country PBF size: **132 MB** (`138624189` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/south-sudan.html
- Pack-server country pack size: **614 MB** (`643855275` bytes)
- Subregions: none

## South-East Asia

- Continent / group: Asia
- Region id: `asia/sea`
- Extract type: multi-country Geofabrik extract for South-East Asia (id `sea`)
- Country-level pack: yes
- Geofabrik country PBF size: **3.41 GB** (`3660245239` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/sea.html
- Pack-server country pack size: **38.3 GB** (`41077449458` bytes)
- Subregions: none

## Spain

- Continent / group: Europe
- Region id: `europe/spain`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **1.38 GB** (`1481358977` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/spain.html
- Subregions (sizes from pack server `current.json`):
  - **Andalucía** `europe/spain/andalucia` — pack **1.75 GB** (`1874434978` bytes)
  - **Aragón** `europe/spain/aragon` — pack **763 MB** (`800076973` bytes)
  - **Asturias** `europe/spain/asturias` — pack **288 MB** (`301764068` bytes)
  - **Cantabria** `europe/spain/cantabria` — pack **227 MB** (`238483229` bytes)
  - **Castilla-La Mancha** `europe/spain/castilla-la-mancha` — pack **1.09 GB** (`1166518958` bytes)
  - **Castilla y León** `europe/spain/castilla-y-leon` — pack **1.73 GB** (`1856427978` bytes)
  - **Cataluña** `europe/spain/cataluna` — pack **2.21 GB** (`2373087133` bytes)
  - **Ceuta** `europe/spain/ceuta` — pack **4.72 MB** (`4952376` bytes)
  - **Extremadura** `europe/spain/extremadura` — pack **448 MB** (`470050862` bytes)
  - **Galicia** `europe/spain/galicia` — pack **1.18 GB** (`1268518834` bytes)
  - **Islas Baleares** `europe/spain/islas-baleares` — pack **264 MB** (`276692706` bytes)
  - **La Rioja** `europe/spain/la-rioja` — pack **115 MB** (`120482770` bytes)
  - **Madrid** `europe/spain/madrid` — pack **668 MB** (`700116966` bytes)
  - **Melilla** `europe/spain/melilla` — pack **5.72 MB** (`5997365` bytes)
  - **Murcia** `europe/spain/murcia` — pack **446 MB** (`467483286` bytes)
  - **Navarra** `europe/spain/navarra` — pack **411 MB** (`431181481` bytes)
  - **País Vasco** `europe/spain/pais-vasco` — pack **462 MB** (`484242006` bytes)
  - **Valencia** `europe/spain/valencia` — pack **1.24 GB** (`1329960382` bytes)

## Sri Lanka

- Continent / group: Asia
- Region id: `asia/sri-lanka`
- Country-level pack: yes
- Geofabrik country PBF size: **138 MB** (`144270372` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/sri-lanka.html
- Pack-server country pack size: **1016 MB** (`1065399563` bytes)
- Subregions: none

## Sudan

- Continent / group: Africa
- Region id: `africa/sudan`
- Country-level pack: yes
- Geofabrik country PBF size: **195 MB** (`204100293` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/sudan.html
- Pack-server country pack size: **1.87 GB** (`2008554986` bytes)
- Subregions: none

## Suriname

- Continent / group: South America
- Region id: `south-america/suriname`
- Country-level pack: yes
- Geofabrik country PBF size: **20.4 MB** (`21360490` bytes)
- Geofabrik URL: https://download.geofabrik.de/south-america/suriname.html
- Pack-server country pack size: **83.0 MB** (`87009577` bytes)
- Subregions: none

## Swaziland

- Continent / group: Africa
- Region id: `africa/swaziland`
- Country-level pack: yes
- Geofabrik country PBF size: **29.2 MB** (`30664206` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/swaziland.html
- Pack-server country pack size: **191 MB** (`200722146` bytes)
- Subregions: none

## Sweden

- Continent / group: Europe
- Region id: `europe/sweden`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **779 MB** (`816774310` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/sweden.html
- Subregions (sizes from pack server `current.json`):
  - **Blekinge** `europe/sweden/blekinge` — pack **81.9 MB** (`85866447` bytes)
  - **Dalarna** `europe/sweden/dalarna` — pack **281 MB** (`294519773` bytes)
  - **Gävleborg** `europe/sweden/gavleborg` — pack **217 MB** (`227255075` bytes)
  - **Gotland** `europe/sweden/gotland` — pack **51.7 MB** (`54184786` bytes)
  - **Halland** `europe/sweden/halland` — pack **175 MB** (`183868111` bytes)
  - **Jämtland** `europe/sweden/jamtland` — pack **242 MB** (`253854290` bytes)
  - **Jönköping** `europe/sweden/jonkoping` — pack **210 MB** (`220489374` bytes)
  - **Kalmar** `europe/sweden/kalmar` — pack **189 MB** (`198582483` bytes)
  - **Kronoberg** `europe/sweden/kronoberg` — pack **131 MB** (`137061002` bytes)
  - **Norrbotten** `europe/sweden/norrbotten` — pack **324 MB** (`340206090` bytes)
  - **Örebro** `europe/sweden/orebro` — pack **218 MB** (`228914640` bytes)
  - **Östergötland** `europe/sweden/ostergotland` — pack **345 MB** (`361470057` bytes)
  - **Skåne** `europe/sweden/skane` — pack **455 MB** (`476916399` bytes)
  - **Södermanland** `europe/sweden/sodermanland` — pack **147 MB** (`153957779` bytes)
  - **Stockholm** `europe/sweden/stockholm` — pack **560 MB** (`586929682` bytes)
  - **Uppsala** `europe/sweden/uppsala` — pack **184 MB** (`193144649` bytes)
  - **Värmland** `europe/sweden/varmland` — pack **255 MB** (`267705855` bytes)
  - **Västerbotten** `europe/sweden/vasterbotten` — pack **251 MB** (`262736055` bytes)
  - **Västernorrland** `europe/sweden/vasternorrland` — pack **253 MB** (`265053625` bytes)
  - **Västmanland** `europe/sweden/vastmanland` — pack **126 MB** (`132586439` bytes)
  - **Västra Götaland** `europe/sweden/vastra_gotaland` — pack **927 MB** (`971623055` bytes)

## Switzerland

- Continent / group: Europe
- Region id: `europe/switzerland`
- Country-level pack: yes
- Geofabrik country PBF size: **521 MB** (`546044355` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/switzerland.html
- Pack-server country pack size: **3.53 GB** (`3788284513` bytes)
- Subregions: none

## Syria

- Continent / group: Asia
- Region id: `asia/syria`
- Country-level pack: yes
- Geofabrik country PBF size: **77.9 MB** (`81707242` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/syria.html
- Pack-server country pack size: **1.05 GB** (`1130055168` bytes)
- Subregions: none

## Taiwan

- Continent / group: Asia
- Region id: `asia/taiwan`
- Country-level pack: yes
- Geofabrik country PBF size: **311 MB** (`326415024` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/taiwan.html
- Pack-server country pack size: **1.74 GB** (`1865948950` bytes)
- Subregions: none

## Tajikistan

- Continent / group: Asia
- Region id: `asia/tajikistan`
- Country-level pack: yes
- Geofabrik country PBF size: **46.1 MB** (`48384848` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/tajikistan.html
- Pack-server country pack size: **452 MB** (`474262315` bytes)
- Subregions: none

## Tanzania

- Continent / group: Africa
- Region id: `africa/tanzania`
- Country-level pack: yes
- Geofabrik country PBF size: **673 MB** (`705605825` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/tanzania.html
- Pack-server country pack size: **6.64 GB** (`7126688104` bytes)
- Subregions: none

## Thailand

- Continent / group: Asia
- Region id: `asia/thailand`
- Country-level pack: yes
- Geofabrik country PBF size: **312 MB** (`327319208` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/thailand.html
- Pack-server country pack size: **7.11 GB** (`7635656824` bytes)
- Subregions: none

## Togo

- Continent / group: Africa
- Region id: `africa/togo`
- Country-level pack: yes
- Geofabrik country PBF size: **59.4 MB** (`62280327` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/togo.html
- Pack-server country pack size: **395 MB** (`413829978` bytes)
- Subregions: none

## Tokelau

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/tokelau`
- Country-level pack: yes
- Geofabrik country PBF size: **142 KB** (`144948` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/tokelau.html
- Pack-server country pack size: **154 KB** (`157838` bytes)
- Subregions: none

## Tonga

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/tonga`
- Country-level pack: yes
- Geofabrik country PBF size: **3.54 MB** (`3717022` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/tonga.html
- Pack-server country pack size: **15.7 MB** (`16439422` bytes)
- Subregions: none

## Tunisia

- Continent / group: Africa
- Region id: `africa/tunisia`
- Country-level pack: yes
- Geofabrik country PBF size: **80.2 MB** (`84107462` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/tunisia.html
- Pack-server country pack size: **1.31 GB** (`1403798933` bytes)
- Subregions: none

## Turkey

- Continent / group: Europe
- Region id: `europe/turkey`
- Country-level pack: yes
- Geofabrik country PBF size: **616 MB** (`646182530` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/turkey.html
- Pack-server country pack size: **7.83 GB** (`8410065162` bytes)
- Subregions: none

## Turkmenistan

- Continent / group: Asia
- Region id: `asia/turkmenistan`
- Country-level pack: yes
- Geofabrik country PBF size: **23.7 MB** (`24865810` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/turkmenistan.html
- Pack-server country pack size: **333 MB** (`348985375` bytes)
- Subregions: none

## Tuvalu

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/tuvalu`
- Country-level pack: yes
- Geofabrik country PBF size: **357 KB** (`365181` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/tuvalu.html
- Pack-server country pack size: **874 KB** (`894553` bytes)
- Subregions: none

## Uganda

- Continent / group: Africa
- Region id: `africa/uganda`
- Country-level pack: yes
- Geofabrik country PBF size: **354 MB** (`371028466` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/uganda.html
- Pack-server country pack size: **2.07 GB** (`2219787374` bytes)
- Subregions: none

## Ukraine (with Crimea)

- Continent / group: Europe
- Region id: `europe/ukraine`
- Country-level pack: yes
- Geofabrik country PBF size: **836 MB** (`877022218` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/ukraine.html
- Pack-server country pack size: **6.47 GB** (`6946910306` bytes)
- Subregions: none

## United Kingdom

- Continent / group: Europe
- Region id: `europe/united-kingdom`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **2.10 GB** (`2255851649` bytes)
- Geofabrik URL: https://download.geofabrik.de/europe/united-kingdom.html
- Subregions (sizes from pack server `current.json`):
  - **Bermuda** `europe/united-kingdom/bermuda` — pack **12.3 MB** (`12878839` bytes)
  - **England** `europe/united-kingdom/england`
    - **Bedfordshire** `europe/united-kingdom/england/bedfordshire` — pack **103 MB** (`107609990` bytes)
    - **Berkshire** `europe/united-kingdom/england/berkshire` — pack **132 MB** (`138540315` bytes)
    - **Bristol** `europe/united-kingdom/england/bristol` — pack **49.9 MB** (`52321122` bytes)
    - **Buckinghamshire** `europe/united-kingdom/england/buckinghamshire` — pack **152 MB** (`159513797` bytes)
    - **Cambridgeshire** `europe/united-kingdom/england/cambridgeshire` — pack **183 MB** (`191616234` bytes)
    - **Cheshire** `europe/united-kingdom/england/cheshire` — pack **218 MB** (`228489512` bytes)
    - **Cornwall** `europe/united-kingdom/england/cornwall` — pack **237 MB** (`248961541` bytes)
    - **Cumbria** `europe/united-kingdom/england/cumbria` — pack **220 MB** (`230840964` bytes)
    - **Derbyshire** `europe/united-kingdom/england/derbyshire` — pack **192 MB** (`201595229` bytes)
    - **Devon** `europe/united-kingdom/england/devon` — pack **392 MB** (`411096029` bytes)
    - **Dorset** `europe/united-kingdom/england/dorset` — pack **156 MB** (`163986049` bytes)
    - **Durham** `europe/united-kingdom/england/durham` — pack **168 MB** (`176677629` bytes)
    - **East Sussex** `europe/united-kingdom/england/east-sussex` — pack **103 MB** (`107942465` bytes)
    - **East Yorkshire with Hull** `europe/united-kingdom/england/east-yorkshire-with-hull` — pack **94.2 MB** (`98804467` bytes)
    - **Essex** `europe/united-kingdom/england/essex` — pack **294 MB** (`308425612` bytes)
    - **Gloucestershire** `europe/united-kingdom/england/gloucestershire` — pack **209 MB** (`218638587` bytes)
    - **Greater London** `europe/united-kingdom/england/greater-london` — pack **667 MB** (`699226393` bytes)
    - **Greater Manchester** `europe/united-kingdom/england/greater-manchester` — pack **362 MB** (`379919156` bytes)
    - **Hampshire** `europe/united-kingdom/england/hampshire` — pack **372 MB** (`390334505` bytes)
    - **Herefordshire** `europe/united-kingdom/england/herefordshire` — pack **57.5 MB** (`60320350` bytes)
    - **Hertfordshire** `europe/united-kingdom/england/hertfordshire` — pack **196 MB** (`205380984` bytes)
    - **Isle of Wight** `europe/united-kingdom/england/isle-of-wight` — pack **29.9 MB** (`31353274` bytes)
    - **Kent** `europe/united-kingdom/england/kent` — pack **342 MB** (`358444504` bytes)
    - **Lancashire** `europe/united-kingdom/england/lancashire` — pack **262 MB** (`274896912` bytes)
    - **Leicestershire** `europe/united-kingdom/england/leicestershire` — pack **150 MB** (`157272132` bytes)
    - **Lincolnshire** `europe/united-kingdom/england/lincolnshire` — pack **228 MB** (`239234493` bytes)
    - **London** `europe/united-kingdom/england/london`
      - **Enfield** `europe/united-kingdom/england/london/enfield` — pack **20.8 MB** (`21860517` bytes)
    - **Merseyside** `europe/united-kingdom/england/merseyside` — pack **152 MB** (`159238468` bytes)
    - **Norfolk** `europe/united-kingdom/england/norfolk` — pack **269 MB** (`282436441` bytes)
    - **North Yorkshire** `europe/united-kingdom/england/north-yorkshire` — pack **329 MB** (`345471411` bytes)
    - **Northamptonshire** `europe/united-kingdom/england/northamptonshire` — pack **151 MB** (`158389769` bytes)
    - **Northumberland** `europe/united-kingdom/england/northumberland` — pack **98.2 MB** (`102925731` bytes)
    - **Nottinghamshire** `europe/united-kingdom/england/nottinghamshire` — pack **206 MB** (`215972461` bytes)
    - **Oxfordshire** `europe/united-kingdom/england/oxfordshire` — pack **145 MB** (`152139288` bytes)
    - **Rutland** `europe/united-kingdom/england/rutland` — pack **11.0 MB** (`11578913` bytes)
    - **Shropshire** `europe/united-kingdom/england/shropshire` — pack **139 MB** (`146168883` bytes)
    - **Somerset** `europe/united-kingdom/england/somerset` — pack **293 MB** (`306984770` bytes)
    - **South Yorkshire** `europe/united-kingdom/england/south-yorkshire` — pack **183 MB** (`191909065` bytes)
    - **Staffordshire** `europe/united-kingdom/england/staffordshire` — pack **185 MB** (`194463255` bytes)
    - **Suffolk** `europe/united-kingdom/england/suffolk` — pack **205 MB** (`215067187` bytes)
    - **Surrey** `europe/united-kingdom/england/surrey` — pack **205 MB** (`214983700` bytes)
    - **Tyne and Wear** `europe/united-kingdom/england/tyne-and-wear` — pack **143 MB** (`150205818` bytes)
    - **Warwickshire** `europe/united-kingdom/england/warwickshire` — pack **124 MB** (`130096045` bytes)
    - **West Midlands** `europe/united-kingdom/england/west-midlands` — pack **262 MB** (`275058793` bytes)
    - **West Sussex** `europe/united-kingdom/england/west-sussex` — pack **186 MB** (`195077014` bytes)
    - **West Yorkshire** `europe/united-kingdom/england/west-yorkshire` — pack **320 MB** (`335558295` bytes)
    - **Wiltshire** `europe/united-kingdom/england/wiltshire` — pack **185 MB** (`193572027` bytes)
    - **Worcestershire** `europe/united-kingdom/england/worcestershire` — pack **112 MB** (`117746447` bytes)
  - **Falkland Islands** `europe/united-kingdom/falklands` — pack **18.2 MB** (`19121740` bytes)
  - **Scotland** `europe/united-kingdom/scotland` — pack **1.80 GB** (`1933613195` bytes)
  - **Wales** `europe/united-kingdom/wales` — pack **858 MB** (`899634073` bytes)

## United States of America

- Continent / group: North America
- Region id: `north-america/us`
- Country-level pack: no (subregions only)
- Geofabrik country PBF size: **11.3 GB** (`12140469551` bytes)
- Geofabrik URL: https://download.geofabrik.de/north-america/us.html
- Subregions (sizes from pack server `current.json`):
  - **us/alabama** `north-america/us/alabama` — pack **1.78 GB** (`1906261846` bytes)
  - **us/alaska** `north-america/us/alaska` — pack **390 MB** (`408674828` bytes)
  - **us/arizona** `north-america/us/arizona` — pack **2.68 GB** (`2879228159` bytes)
  - **us/arkansas** `north-america/us/arkansas` — pack **1.24 GB** (`1326963661` bytes)
  - **us/california** `north-america/us/california`
    - **Northern California** `north-america/us/california/norcal` — pack **4.32 GB** (`4636561336` bytes)
    - **Southern California** `north-america/us/california/socal` — pack **4.05 GB** (`4346136125` bytes)
  - **us/colorado** `north-america/us/colorado` — pack **2.68 GB** (`2875422389` bytes)
  - **us/connecticut** `north-america/us/connecticut` — pack **1.11 GB** (`1192326122` bytes)
  - **us/delaware** `north-america/us/delaware` — pack **255 MB** (`266976974` bytes)
  - **us/district-of-columbia** `north-america/us/district-of-columbia` — pack **121 MB** (`126729520` bytes)
  - **us/florida** `north-america/us/florida` — pack **5.37 GB** (`5763135651` bytes)
  - **Georgia** `north-america/us/georgia` — pack **3.09 GB** (`3314988986` bytes)
  - **us/hawaii** `north-america/us/hawaii` — pack **204 MB** (`213883341` bytes)
  - **us/idaho** `north-america/us/idaho` — pack **1.32 GB** (`1419205064` bytes)
  - **us/illinois** `north-america/us/illinois` — pack **3.75 GB** (`4027106919` bytes)
  - **us/indiana** `north-america/us/indiana` — pack **2.49 GB** (`2677096577` bytes)
  - **us/iowa** `north-america/us/iowa` — pack **1.37 GB** (`1475601557` bytes)
  - **us/kansas** `north-america/us/kansas` — pack **1.59 GB** (`1704072337` bytes)
  - **us/kentucky** `north-america/us/kentucky` — pack **1.63 GB** (`1751101848` bytes)
  - **us/louisiana** `north-america/us/louisiana` — pack **1.37 GB** (`1469520789` bytes)
  - **us/maine** `north-america/us/maine` — pack **736 MB** (`771474329` bytes)
  - **us/maryland** `north-america/us/maryland` — pack **1.71 GB** (`1831661146` bytes)
  - **us/massachusetts** `north-america/us/massachusetts` — pack **1.96 GB** (`2105825137` bytes)
  - **us/michigan** `north-america/us/michigan` — pack **3.88 GB** (`4163820135` bytes)
  - **us/minnesota** `north-america/us/minnesota` — pack **2.10 GB** (`2253879239` bytes)
  - **us/mississippi** `north-america/us/mississippi` — pack **984 MB** (`1032112488` bytes)
  - **us/missouri** `north-america/us/missouri` — pack **2.68 GB** (`2872916833` bytes)
  - **us/montana** `north-america/us/montana` — pack **1011 MB** (`1060616984` bytes)
  - **us/nebraska** `north-america/us/nebraska` — pack **972 MB** (`1019408146` bytes)
  - **us/nevada** `north-america/us/nevada` — pack **1.21 GB** (`1303682458` bytes)
  - **us/new-hampshire** `north-america/us/new-hampshire` — pack **690 MB** (`723687737` bytes)
  - **us/new-jersey** `north-america/us/new-jersey` — pack **1.65 GB** (`1768613070` bytes)
  - **us/new-mexico** `north-america/us/new-mexico` — pack **1.18 GB** (`1270468589` bytes)
  - **us/new-york** `north-america/us/new-york` — pack **3.52 GB** (`3784855619` bytes)
  - **us/north-carolina** `north-america/us/north-carolina` — pack **4.00 GB** (`4297193712` bytes)
  - **us/north-dakota** `north-america/us/north-dakota` — pack **760 MB** (`796451370` bytes)
  - **us/ohio** `north-america/us/ohio` — pack **4.02 GB** (`4316350857` bytes)
  - **us/oklahoma** `north-america/us/oklahoma` — pack **1.68 GB** (`1801667611` bytes)
  - **us/oregon** `north-america/us/oregon` — pack **2.05 GB** (`2204944488` bytes)
  - **us/pennsylvania** `north-america/us/pennsylvania` — pack **3.49 GB** (`3750265736` bytes)
  - **us/puerto-rico** `north-america/us/puerto-rico` — pack **419 MB** (`439406334` bytes)
  - **us/rhode-island** `north-america/us/rhode-island` — pack **218 MB** (`228246479` bytes)
  - **us/south-carolina** `north-america/us/south-carolina` — pack **1.71 GB** (`1834187096` bytes)
  - **us/south-dakota** `north-america/us/south-dakota` — pack **546 MB** (`572195277` bytes)
  - **us/tennessee** `north-america/us/tennessee` — pack **2.06 GB** (`2207998668` bytes)
  - **us/texas** `north-america/us/texas` — pack **8.17 GB** (`8768293363` bytes)
  - **us/us-virgin-islands** `north-america/us/us-virgin-islands` — pack **23.8 MB** (`24977773` bytes)
  - **us/utah** `north-america/us/utah` — pack **1.69 GB** (`1809930962` bytes)
  - **us/vermont** `north-america/us/vermont` — pack **368 MB** (`385692834` bytes)
  - **us/virginia** `north-america/us/virginia` — pack **3.13 GB** (`3362902094` bytes)
  - **us/washington** `north-america/us/washington` — pack **2.81 GB** (`3017554736` bytes)
  - **us/west-virginia** `north-america/us/west-virginia` — pack **760 MB** (`797401079` bytes)
  - **us/wisconsin** `north-america/us/wisconsin` — pack **2.41 GB** (`2584914028` bytes)
  - **us/wyoming** `north-america/us/wyoming` — pack **733 MB** (`768880953` bytes)

## Uruguay

- Continent / group: South America
- Region id: `south-america/uruguay`
- Country-level pack: yes
- Geofabrik country PBF size: **53.5 MB** (`56107645` bytes)
- Geofabrik URL: https://download.geofabrik.de/south-america/uruguay.html
- Pack-server country pack size: **301 MB** (`315547312` bytes)
- Subregions: none

## US Midwest

- Continent / group: North America
- Region id: `north-america/us-midwest`
- Extract type: special US regional Geofabrik extract (Midwest); overlaps state packs, not a country
- Country-level pack: yes
- Geofabrik country PBF size: **2.33 GB** (`2500351197` bytes)
- Geofabrik URL: https://download.geofabrik.de/north-america/us-midwest.html
- Pack-server country pack size: **25.9 GB** (`27792708278` bytes)
- Subregions: none

## US Northeast

- Continent / group: North America
- Region id: `north-america/us-northeast`
- Extract type: special US regional Geofabrik extract (Northeast); overlaps state packs, not a country
- Country-level pack: yes
- Geofabrik country PBF size: **1.67 GB** (`1798185986` bytes)
- Geofabrik URL: https://download.geofabrik.de/north-america/us-northeast.html
- Pack-server country pack size: **13.8 GB** (`14807530891` bytes)
- Subregions: none

## US Pacific

- Continent / group: North America
- Region id: `north-america/us-pacific`
- Extract type: special US regional Geofabrik extract (Pacific); overlaps state packs, not a country
- Country-level pack: yes
- Geofabrik country PBF size: **164 MB** (`171546160` bytes)
- Geofabrik URL: https://download.geofabrik.de/north-america/us-pacific.html
- Pack-server country pack size: **541 MB** (`567301966` bytes)
- Subregions: none

## US South

- Continent / group: North America
- Region id: `north-america/us-south`
- Extract type: special US regional Geofabrik extract (South); overlaps state packs, not a country
- Country-level pack: yes
- Geofabrik country PBF size: **3.84 GB** (`4123288536` bytes)
- Geofabrik URL: https://download.geofabrik.de/north-america/us-south.html
- Pack-server country pack size: **36.8 GB** (`39506379549` bytes)
- Subregions: none

## US West

- Continent / group: North America
- Region id: `north-america/us-west`
- Extract type: special US regional Geofabrik extract (West); overlaps state packs, not a country
- Country-level pack: yes
- Geofabrik country PBF size: **3.17 GB** (`3400591554` bytes)
- Geofabrik URL: https://download.geofabrik.de/north-america/us-west.html
- Pack-server country pack size: **24.0 GB** (`25726803234` bytes)
- Subregions: none

## Uzbekistan

- Continent / group: Asia
- Region id: `asia/uzbekistan`
- Country-level pack: yes
- Geofabrik country PBF size: **118 MB** (`124004408` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/uzbekistan.html
- Pack-server country pack size: **1.62 GB** (`1743709068` bytes)
- Subregions: none

## Vanuatu

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/vanuatu`
- Country-level pack: yes
- Geofabrik country PBF size: **7.53 MB** (`7892716` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/vanuatu.html
- Pack-server country pack size: **25.6 MB** (`26810351` bytes)
- Subregions: none

## Venezuela

- Continent / group: South America
- Region id: `south-america/venezuela`
- Country-level pack: yes
- Geofabrik country PBF size: **121 MB** (`127064287` bytes)
- Geofabrik URL: https://download.geofabrik.de/south-america/venezuela.html
- Pack-server country pack size: **1.39 GB** (`1487687036` bytes)
- Subregions: none

## Vietnam

- Continent / group: Asia
- Region id: `asia/vietnam`
- Country-level pack: yes
- Geofabrik country PBF size: **313 MB** (`328677403` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/vietnam.html
- Pack-server country pack size: **8.03 GB** (`8623835247` bytes)
- Subregions: none

## Wallis et Futuna

- Continent / group: Australia and Oceania
- Region id: `australia-oceania/wallis-et-futuna`
- Country-level pack: yes
- Geofabrik country PBF size: **604 KB** (`618540` bytes)
- Geofabrik URL: https://download.geofabrik.de/australia-oceania/wallis-et-futuna.html
- Pack-server country pack size: **1.85 MB** (`1937655` bytes)
- Subregions: none

## Yemen

- Continent / group: Asia
- Region id: `asia/yemen`
- Country-level pack: yes
- Geofabrik country PBF size: **41.2 MB** (`43161073` bytes)
- Geofabrik URL: https://download.geofabrik.de/asia/yemen.html
- Pack-server country pack size: **507 MB** (`531918792` bytes)
- Subregions: none

## Zambia

- Continent / group: Africa
- Region id: `africa/zambia`
- Country-level pack: yes
- Geofabrik country PBF size: **240 MB** (`251778521` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/zambia.html
- Pack-server country pack size: **1.60 GB** (`1720590254` bytes)
- Subregions: none

## Zimbabwe

- Continent / group: Africa
- Region id: `africa/zimbabwe`
- Country-level pack: yes
- Geofabrik country PBF size: **171 MB** (`179411950` bytes)
- Geofabrik URL: https://download.geofabrik.de/africa/zimbabwe.html
- Pack-server country pack size: **963 MB** (`1009451236` bytes)
- Subregions: none

