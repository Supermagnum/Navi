**[English README](README.md)**

# Testere ønskes

**Testere ønskes** for testing på **ekte maskinvare** (Android Automotive /
skjermenheter). Utviklingen så langt er kun på emulator — ekte enheter oppfører
seg annerledes for GPS, MapLibre, Vulkan/GLES og ytelse. Sjekkliste:
[`docs/real-hardware-testing.md`](docs/real-hardware-testing.md).

# AI-bistand

Dette prosjektet er utviklet med AI-bistand (Claude). Forfatteren har en
nevrologisk tilstand knyttet til dyskalkuli som påvirker programmering på en
måte som tilsvarer hvordan dyskalkuli påvirker matematiske evner — AI-bistand
ble brukt for å omsette designintensjon til fungerende kode og dokumentasjon.
Designvalg, krav og testing er styrt og gjennomgått av forfatteren underveis.

## Innhold

- [Navi](#navi)
  - [Funksjoner](#funksjoner)
  - [Hvor data kommer fra](#hvor-data-kommer-fra)
  - [Slik fungerer funksjonene](#slik-fungerer-funksjonene)
  - [Innstillinger](#innstillinger)
- [Fungerende app (emulatorskjermbilder)](#fungerende-app-emulatorskjermbilder)
- [Dokumenter](#dokumenter)
- [Ikoner (Navit)](#ikoner-navit)
- [Bygge Android-pakker](#bygge-android-pakker)
- [Ytelseskrav](#ytelseskrav-minimum-8-kjerner--2-ghz-4-gb-ram)
- [Arbeidsområdets struktur](#arbeidsområdets-struktur)
- [Vertstester](#vertstester)
- [Kjente problemer](#kjente-problemer)

Mer å lese i depotet: kassekobling og SQLite-oppsett i
[`architecture.md`](architecture.md); plugin-idéer i
[`docs/plugins.md`](docs/plugins.md); Android-byggesteg i
[`docs/android-build.md`](docs/android-build.md); Linux-kjernebygg i
[`docs/build-linux.md`](docs/build-linux.md); feilsøking i
[`docs/debugging.md`](docs/debugging.md); HUD-layout i
[`docs/hud-layout.md`](docs/hud-layout.md); kartstiler / PMTiles / 3D i
[`docs/map-styles.md`](docs/map-styles.md); IMU-monteringskalibrering (utsatt) i
[`docs/imu-calibration.md`](docs/imu-calibration.md).

# Navi

Frakoblet navigasjonskjerne (Rust) og Android Automotive-vert (Kotlin/Compose)
for ruteplanlegging med terrengbevisst (øko) kostnadsberegning, POI-støtte,
hvile/overnatting og profilbasert ruting. Karttegning bruker MapLibre
(Vulkan SDK) over OpenFreeMap liberty-grunnkart. Kjernen forblir frakoblet når
et regionsuttrekk og DEM-fliser ligger på disk; nettverk er valgfritt for
nedlastinger og oppdateringer.

Lisens for dette depotet: se `LICENSE` (GPL-3.0-or-later med mindre annet er
angitt). Ikonressurser under `core/src/icons` er Navit-avledet (**GPL v2**); se
[`docs/icons.md`](docs/icons.md).

Navigasjonsappen har valgfri bevissthet om terrenghelning: med økomodus på
forsøker den å finne ruten som bruker minst energi (personbil-baseline;
elektriske profiler får regen-kreditt via `EcoConfig::for_profile`). Når
økomodus er på, vises et lite bladikon nederst til høyre. Den kan foreslå
pausestopp langs planlagt rute, anvende kjøretøygrenser og unngå
hovedvei/bom/ferge ved planlegging, og valgfritt **følge offisielle
tur-/sykkelnettverk** (myk preferanse, av som standard). Du kan sette
bil-pauseintervaller. Den har et minnebasert bevegelig-ikon-lager
(`TrackStore`) og en sandkasse WASM-pluginvert for fremtidige plugins;
produktplugins er ikke levert ennå ([`docs/plugins.md`](docs/plugins.md)).

## Funksjoner

| Funksjon | Hva du får | Status |
|---|---|---|
| **Profiler** | Bil, motorsykkel, sykling, fottur, lastebil, bobil (elektriske varianter i enum; primære UI-brikker er de ikke-elektriske) | Ferdig |
| **Kjøretøygrenser** | Aksel / boggi / høyde / bredde / lengde lagres i SQLite og brukes av `plan_car_route` (hard ekskludering ved OSM-begrensninger) | Ferdig |
| **Unngåelser** | Unngå hovedvei / bom / ferge endrer faktisk planlagt rute | Ferdig |
| **Følg offisielle nettverk** | Myk preferanse for tur-/sykkelrutenett (av som standard); hull faller tilbake til vanlige stier; vanskelighets-tagger som metadata; navngitte ruter i FTS | Ferdig |
| **Økoruting** | DEM-høyde + luftmotstand/masse/rulle; grafbuffer; A*; EV-regen via `EcoConfig::for_profile` i FFI-planleggeren; formler i [`docs/mathematical-formulas.md`](docs/mathematical-formulas.md) | Ferdig |
| **Korridor- / regionsruting** | OSM `.pbf` → graf → øko-omvekt → bufret graf → A* → polylinje + pause-POI på MapLibre | Ferdig |
| **POI-søk** | FTS-stedsindeks (inkl. navngitte ruter); Til / Via / Fra ([`docs/poi.md`](docs/poi.md)). Hytteradius: [`docs/poi-search-defaults.md`](docs/poi-search-defaults.md). Kategori Fishing (`leisure=fishing`) | Ferdig |
| **Hvile og pauser** | Profilstandarder ([rast/vei for fottur/sykkel](docs/historisk-bakgrunn.md)); bil-HUD minutter til pause; pause-POI; overnatting bygg/bre-filter på hiking-FFI | Ferdig |
| **Kjøre-HUD** | Kollapset topp (høyde; trykk → kartinnstillinger) + bunn (zoom −/+, pause/ETA, øko; trykk → kjøreinnstillinger) | Ferdig |
| **Kartrotasjon** | Kompass, kjøreretning eller nord opp | Ferdig |
| **Bevegelige ikoner** | `TrackStore` (upsert, tidsavbrudd, 50–150 km); Compose kan tegne stasjoner | **Delvis** — lager + kartsti finnes; ingen live APRS; app via testhooks |
| **OSM-oppdateringer** | Valgfri Geofabrik-sjekk / `.osc.gz` (trenger `osmium`) eller full nedlasting ([`docs/osm-updates.md`](docs/osm-updates.md)) | Ferdig |
| **Plugins** | Sandkasse WASM-vert + HostApi + isolasjonstester; eksempel `log-hello` / `busy-loop` | **Vert ferdig; innholdsplugins utsatt** — bevisst (se [`docs/plugins.md`](docs/plugins.md)); lastes ikke av Android-appen ennå |

**Ekte maskinvare:** Utvikling og automatiske sjekker så langt bruker bare
Android Automotive-**emulatoren**. Appen **må testes på ekte maskinvare** før
noe leveringskrav — GPS/IMU, MapLibre-lag, Vulkan/GLES, sensorer og ytelse
skiller seg fra AVD. Følg
[`docs/real-hardware-testing.md`](docs/real-hardware-testing.md).

## Hvor data kommer fra

Navi er **frakoblet først**: ruting, søk og økokostnader kjører fra filer på
disk. Nettverk brukes bare når du velger det (klargjøring, oppdatering, eller
live grunnkartfliser mens du er online).

| Data | Kilde | Bruk |
|---|---|---|
| **Vei- / POI-uttrekk** | [OpenStreetMap](https://www.openstreetmap.org/) via [Geofabrik](https://download.geofabrik.de/) regional `.osm.pbf` (eller egen korridor) | Graf for ruting; FTS sted/adresse; POI-kategorier |
| **OSM-oppdateringer** | Geofabrik `state.txt` + `.osc.gz` eller full `*-latest.osm.pbf` | Valgfri sjekk/anvendelse — aldri stille ([`docs/osm-updates.md`](docs/osm-updates.md)) |
| **Høyde (DEM)** | Copernicus DSM / SRTM / Viewfinder-lignende fliser | Økorute-energikostnader og terrenglogikk |
| **Grunnkart (visuelt)** | Online: [OpenFreeMap](https://openfreemap.org/) Liberty (MapLibre). Frakoblet: regional **Protomaps PMTiles** + medfølgende Protomaps light-stil ([`docs/map-styles.md`](docs/map-styles.md)). Valgfri **3D**: Mapterhorn DEM hillshade | Kart på skjermen; ikke rutingsgrafen |
| **Posisjon / kurs** | Enhets-GPS (Android) eller **gpsd** + IMU på Linux | Live posisjon, høyde-HUD, kompass / kjøreretning |
| **Ikoner** | Medfølgende Navit-avledet SVG under `core/src/icons` | Manøver / POI / øko-blad |

Når regionsuttrekk og DEM-fliser ligger på enheten, trenger kjernenettet ikke
nettverk. Det visuelle grunnkartet bruker live OpenFreeMap Liberty til en
regional PMTiles-fil er lastet ned (Verktøy → Last ned grunnkart); deretter
lastes Protomaps frakoblet. Valgfri terreng-DEM er samme sti med
**Last ned terreng-DEM (Mapterhorn)** (`{region}_dem.pmtiles`).
([`docs/map-styles.md`](docs/map-styles.md)).

## Slik fungerer funksjonene

**Følg offisielle nettverk (fottur / sykling).** Av som standard. Når på, får
kanter som er medlemmer av matchende `type=route`-relasjoner
(`route=hiking|foot` + `network=iwn|nwn|rwn|lwn`, eller `route=bicycle|mtb` +
`icn|ncn|rcn|lcn`) en myk kostnadspreferanse — ikke-nettverk forblir
tilgjengelig så hull aldri feiler planen. Vanskelighets-tagger (`sac_scale`,
`mtb:scale`, …) vises som informasjons-`route_metadata`. Navngitte ruters
`name`/`ref`/`operator` indekseres i FTS for Til/Via. Kjente begrensninger
denne runden: ett nivå `type=superroute`; Benelux-node-nettverk og
nivåvektet preferanse er utsatt.

**Rutingsstabel.** En regional `.pbf` parses til en veigraf. Med øko på
omvektes kanter med DEM-høyde og `EcoConfig`-fysikk (`segment_energy_joules`),
deretter lagres (`NAVIGPH1`-buffer) så neste oppstart hopper over full
omvekt. A* finner en korridor; Android-verten tegner polylinjen og
destinasjonsmarkør på MapLibre.

**Øko vs lengde.** Ren lengderuting ignorerer bakker. Øko foretrekker lavere
energi (stigninger koster PE). Forbrenningsprofiler beholder regen 0; elektriske
profiler krediterer en andel av nedstignings-PE via `EcoConfig::for_profile`.
Live OBD/J1939 kan senere forbedre kostnader via `LiveEnergySnapshot`
([`docs/ECU.md`](docs/ECU.md)); i dag brukes lagret tank / påfylt drivstoff når
ECU mangler.

**POI og søk.** Kategorier og taggregler står i [`docs/poi.md`](docs/poi.md)
(inkl. **Fishing** / `leisure=fishing`, ikon `fish.svg`). Foreslåtte
nettverkshytte- / løypepreferanseradius står i
[`docs/poi-search-defaults.md`](docs/poi-search-defaults.md). FTS indekserer også
navngitte offisielle ruterelasjoner. Søketreff setter Til/Via og flytter kamera.

**Hvile / overnatting.** Hvileparametre er profilavhengige. Standardverdier for
fottur og sykling kommer fra skandinavisk *rast*- / *vei*-tradisjon — se
[`docs/historisk-bakgrunn.md`](docs/historisk-bakgrunn.md)
([engelsk](docs/historical-background.md)).
Sikkerhetsregler avviser overnattingsskandidater for nær bygninger eller
isbreer (koblet inn i hiking-FFI via `OvernightProximityIndex`).
Bygningsavstandssjekken følger norsk **allemannsrett** — det juridiske
rammeverket er norsk og **gjelder ikke nødvendigvis i andre land**.
HUD-bryteren «Pauser» styrer påminnelsestekst; intervall/varighet redigeres i
kjøreinnstillinger.

**Kart og HUD.** MapLibre Vulkan tegner grunnkartet. Kollapset topp-HUD viser
høyde; trykk åpner kartinnstillinger (rotasjon, tur-ETA, pauser, auto-zoom).
Kollapset bunn-HUD viser zoom −/+, pausetid, tur-ETA og øko-blad; trykk åpner
kjøre-/hvile-/drivstoffinnstillinger. Nær en sving viser den midlertidige
tilnærmingsboksen manøverikon + avstand + neste gate
([`docs/approach-instructions.md`](docs/approach-instructions.md)).

**Høyde på emulatoren.** Android Studios Automotive-emulator GNSS rapporterer
ofte feil vertikal fiksering. Det er en **emulatorbegrensning**, ikke en
appfeil. HUD foretrekker DEM-terrenghøyde fra disk når en flis dekker fikset;
på ekte maskinvare kan GPS-høyde brukes når DEM mangler.

**Spor.** `TrackStore` oppdaterer stasjoner etter id, utløper etter tidsavbrudd
og filtrerer med Haversine-rekkevidde ([`docs/APRS.md`](docs/APRS.md)).
RF-dekoding leveres ikke; IQ via `rtl-sdr-rs` er planlagt
([`docs/APRS-SDR.md`](docs/APRS-SDR.md)).

## Innstillinger

Innstillinger lagres i appens SQLite-config under enhetens datakatalog
(UniFFI `load*` / `save*`). Bruk på kjøreinnstillinger skriver og lukker;
Avbryt forkaster uten å lagre den redigeringsøkten.

### Topp-HUD (kollapset som standard — trykk for kartinnstillinger)

| Kontroll | Oppførsel |
|---|---|
| **Kollapset stripe** | Viser kartetikett, høyde, rotasjonstips; trykk veksler kart-/visningsinnstillinger |
| **Høyde** | DEM-terrenghøyde når flis dekker fikset; ellers GPS-høyde (`Alt --` til noe er tilgjengelig). Emulator-GNSS-høyde er ofte feil — det er AVD, ikke appen |
| **Kompass / Reise / N-opp** | I kartinnstillinger: kameraretning fra magnetisk kurs, GPS-kurs eller nord opp |
| **Tur-ETA** | I kartinnstillinger: aktiverer ETA-linje på bunnlinjen |
| **Pauser** | I kartinnstillinger: aktiverer/deaktiverer pausepåminnelsestekst på bunnlinjen |
| **Auto-zoom** | I kartinnstillinger: når på, setter zoom til konfigurert nivå (−/+ 0,5 steg) |

**Estimater før avreise** (vist før kjøretøy/turgåer/syklist begynner å bevege
seg) er beregnede estimater, ikke live målinger — basert på skiltet `maxspeed`
(med motorvei-klasse som reserve) for bil/MC/lastebil, og faste
gjennomsnittsfarter (16 min/km fottur, ~4 min/km sykling) for fottur/sykling.
Dette er startestimater; faktisk tid varierer med forhold, trafikk, vær, form
og terreng, og oppdateres automatisk når ekte bevegelse/GPS-hastighet finnes.

### Bunn-HUD (kollapset — trykk statusområdet for kjøreinnstillinger)

| Kontroll | Oppførsel |
|---|---|
| **Zoom − / +** | Appens egen kartzoom (AAOS klima − 63 + i systemkrom er ikke zoom) |
| **Pause / ETA** | Tid til pause og tur-ETA (ingen svingstubb — se tilnærmingsinstruksjoner) |
| **Øko-blad** | Vises på denne linjen bare når økomodus er aktiv for profilen |
| **Trykk status** | Åpner kjøre- / hvile- / drivstoffinnstillinger |

### Kjøreinnstillinger (trykk bunn-HUD — lagres)

| Felt | Lagres som | Merknad |
|---|---|---|
| Timer mellom pauser | Bil-hvilestandarder | Profilstandard for bil, ikke engangs overstyring |
| Hviletid (minutter) | Bil-hvilestandarder | Samme lagring som pauseintervall |
| Økomodus | Bil-hvile `ecoModeEnabled` | Blad på bunn-HUD når på |
| Enheter liter / gallon | `FuelConfig.prefer_liters` | Visningspreferanse; lagring er alltid liter |
| Tankkapasitet | `FuelConfig.tank_capacity_l` | Konverteres fra gal→L ved lagring når enhet er gallon |
| Drivstoff påfylt | `FuelConfig.fuel_added_l` | Gir adaptivt forbruk når live ECU mangler |

Auto-zoom-nivå redigeres i **kartinnstillinger** (topplinje), lagres via
`MapHudPrefs`.

### Profil- / kjøretøypanel (verktøy-UI — lagres)

| Kontroll | Lagres som | Merknad |
|---|---|---|
| Reiseprofil-chip | I minne + hvilelast ved bytte | Menyfokus: bil, sykling, fottur, motorsykkel (lastebil / bobil / el i enum) |
| Øko-bryter | Med hvile- / profilstandarder | Fottur og sykling låser øko på; motorprofiler kan veksle |
| **Følg offisielle tur-/sykkelnettverk** | `prefer_official_networks` (av som standard) | Bare fottur / sykling — myk preferanse; hull faller tilbake til vanlige stier |
| Unngå hovedvei / bom / ferge | Sendes inn i `plan_car_route` ved plan | Endrer faktisk rute (ikke bare rapporttekst) |
| Kjøretøygrenser | `VehicleLimits` | Brukes ved plan for motorprofiler; brudd på OSM-frihøyde utelukkes |

### Spor (APRS-stil)

| Innstilling | Grenser | API |
|---|---|---|
| Visningsrekkevidde | Begrenset **50–150 km** | `TrackStore::set_range_km` / `visible` |
| Stasjons-tidsavbrudd | Maks **3600 s** | `TrackStore::set_timeout_s` / `expire` |

Mer detalj: [`architecture.md`](architecture.md), [`docs/API.md`](docs/API.md),
[`docs/real-hardware-testing.md`](docs/real-hardware-testing.md).

## Fungerende app (emulatorskjermbilder)

Tatt på Android Automotive-emulator med MapLibre + OpenFreeMap liberty.
Kollapset topp-/bunn-kjøre-HUD (søk skjult):

![Idle begge linjer](docs/images/hud/hud_idle_both_bars.png)

Bilrute Helgøya → Atnbrua på Automotive-emulatoren (HUD viser høyde;
AVD GNSS-høyde er ofte feil — se merknad over). Én rast er synlig:

![Helgøya til Atnbrua-rute](docs/images/terrain/hike_eldabu_ramshogda_3d.png)

Alle andre skjermbilder (kartzoom, ruteoverlegg, menyer, innstillinger,
øko-blad, rotasjon, kurs, bevegelige ikoner):
[`docs/bilder.md`](docs/bilder.md).

## Dokumenter

| Dokument | Beskrivelse |
|---|---|
| [`architecture.md`](architecture.md) | Kassekobling, trådlag, SQLite / FTS / grafbuffer, plugins |
| [`docs/bilder.md`](docs/bilder.md) | Emulatorskjermbildegalleri (norsk) |
| [`docs/pictures.md`](docs/pictures.md) | Emulatorskjermbildegalleri (engelsk) |
| [`docs/historisk-bakgrunn.md`](docs/historisk-bakgrunn.md) | Rast/vei-grunnlag for standard pauseintervaller (fottur og sykling); [engelsk](docs/historical-background.md) |
| [`docs/hud-layout.md`](docs/hud-layout.md) | Størrelse og plassering av kjøre-HUD og menyer |
| [`docs/map-styles.md`](docs/map-styles.md) | Online Liberty vs frakoblet Protomaps PMTiles; 3D-port |
| [`docs/approach-instructions.md`](docs/approach-instructions.md) | Midlertidig manøver-tilnærmingsboks |
| [`docs/poi.md`](docs/poi.md) | Søkbar POI-kategorier (inkl. Fishing), OSM-taggregler |
| [`docs/poi-search-defaults.md`](docs/poi-search-defaults.md) | Foreslåtte hytte-/løyperadius for fottur og sykling (DNT-avstand) |
| [`docs/osm-updates.md`](docs/osm-updates.md) | Valgfri Geofabrik-sjekk / `.osc.gz` / full nedlasting |
| [`docs/plugins.md`](docs/plugins.md) | Plugin-vert-status (bevisst: ingen innholdsplugins ennå) + HostApi, isolasjon, veikart |
| [`docs/plugins/right-to-roam-camping-spec.md`](docs/plugins/right-to-roam-camping-spec.md) | Spesifikasjon: allemannsrett / flerland villcamping (plugin) |
| [`docs/icons.md`](docs/icons.md) | Ikonoversikt; egne SVG-ikoner; Navit GPL-v2 |
| [`docs/API.md`](docs/API.md) | UniFFI / vert-API-oversikt |
| [`docs/PROTOCOLS.md`](docs/PROTOCOLS.md) | Protokollindeks |
| [`docs/ECU.md`](docs/ECU.md) | ECU-protokoller: OBD-II, J1939, MegaSquirt + EV |
| [`docs/mathematical-formulas.md`](docs/mathematical-formulas.md) | Formler: MAF/J1939/MegaSquirt-drivstoff, rekkevidde, øko-segmentenergi |
| [`docs/APRS.md`](docs/APRS.md) | APRS-felter, TrackStore-filtrering, bevegelige ikoner |
| [`docs/APRS-SDR.md`](docs/APRS-SDR.md) | APRS SDR DSP; RTL-SDR; planlagt `rtl-sdr-rs` |
| [`docs/CAT.md`](docs/CAT.md) | CAT VFO auto-tune fra NFM-repeatere |
| [`docs/voice-guidance.md`](docs/voice-guidance.md) | Planlagt stemmeveiledningsplugin |
| [`docs/android-build.md`](docs/android-build.md) | Bygg native `libnavi.so`, UniFFI og Gradle-APK |
| [`docs/build-linux.md`](docs/build-linux.md) | Linux: Rust-kjerne, integrasjonstester, gpsd + IMU |
| [`docs/imu-calibration.md`](docs/imu-calibration.md) | Utsatt: IMU pitch/roll-nullstilling for øko-høyde |
| [`docs/debugging.md`](docs/debugging.md) | Vert- + Android-feilsøkingsløkker |
| [`docs/real-hardware-testing.md`](docs/real-hardware-testing.md) | **Påkrevd:** sjekkliste for fysisk enhet vs emulator |
| [`test-results.md`](test-results.md) | Vert-integrasjonstestnotater |
| [`android-test-results.md`](android-test-results.md) | Resultater på enhet / emulator |

## Ikoner (Navit)

Se [`docs/icons.md`](docs/icons.md) for full ikonsystembeskrivelse. Kort:
POI-/manøver-/statusikoner under `core/src/icons` er Navit-avledet (**GPL v2**).
Oppløsning foretrekker brukeroverskrivninger, deretter medfølgende sett, deretter
`unknown.svg`.

**Egne ikoner:** bruk **SVG** (eller `.svgz`). Statisk kunst i
[Inkscape](https://inkscape.org/); animasjoner i
[Synfig Studio](https://www.synfig.org/). Navngi etter semantisk nøkkel — steg
i [`docs/icons.md`](docs/icons.md#adding-custom-icons).

## Bygge Android-pakker

Full guide: [`docs/android-build.md`](docs/android-build.md).

```bash
# 1) Rust CDYLIB + UniFFI Kotlin (emulator-ABI)
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/<version>}"
./scripts/build-android-native.sh x86_64-linux-android release

# 2) APK
./gradlew :app:assembleDebug          # → app/build/outputs/apk/debug/
./gradlew :app:installDebug           # installer på adb-enhet

# Enhet / AAOS arm64 i stedet for emulator:
# ./scripts/build-android-native.sh aarch64-linux-android release

./scripts/launch-navi-emulator.sh      # start MainActivity på AAOS AVD
```

Oppdater `.cargo/config.toml` linker-stier til NDK før første native bygg.
`minSdk` 26, `compileSdk` / `targetSdk` 35, JDK 17.

## Ytelseskrav (minimum: 8 kjerner ~2 GHz, 4 GB RAM)

**Minimum maskinvare** for tiltenkt Automotive- / innebygd klasse:

| Ressurs | Minimum |
|---|---|
| CPU | **8 kjerner**, ca. **2 GHz**-klasse |
| RAM | **4 GB** |

Estimater nedenfor er ikke målt på den enhetsklassen ennå.

| Oppgave | Dataskala | Estimert tid | Merknad |
|---|---|---|---|
| OSM `.pbf` parse + grafbygg | ~1,5M noder / ~1,26M kanter | ~30–90 s | Hovedsakelig enkeltpass CPU + I/O |
| POI R-tre | Få tusen POI | &lt; 1 s | Nesten lineær bulk lasting |
| Øko-omvekt (høyde) | ~1,26M kanter, ~9 DEM-fliser | ~10–60 s, én gang per region | Bufre dekomprimerte fliser |
| A* én rute | ~1,26M kanter | &lt; 1 s (ofte 100–300 ms) | |
| Flere dager + hyttevalg | Regional graf | 1–3 s | På allerede lastet graf |

### Hard begrensning: RAM

- **4 GB er den bindende grensen**, ikke CPU-frekvens.
- Standard arbeidssett: **fylkes-/regionsuttrekk** (~1,5M noder).
- Landsskala for store land risikerer OOM på 4 GB — behandle som valgfritt med
  advarsel i appen.
- 9M-noders referanse krevde under 5 GB på desktop; den skalaen er ikke en trygg
  minnestandard på denne enhetsklassen.

### Minimum ledig lagring (SD-kort / intern)

Frakoblet **rutingsdata** (Geofabrik `.osm.pbf` + grafbuffer + sted/FTS + DEM +
scratch for oppdateringer). Omfatter **ikke** MapLibre-grunnkartfliser med mindre
du også laster regionale **PMTiles**. Se [`docs/map-styles.md`](docs/map-styles.md).

Geofabrik `.osm.pbf`-størrelser (ca., midten av 2026):

| Land / uttrekk | Bare `.osm.pbf` | **Minimum ledig plass å budsjettere** |
|---|---|---|
| **Sverige** | ~0,8 GB | **~3–5 GB** |
| **Norge** | ~1,3 GB | **~4–6 GB** |
| **Russland** | ~4,1 GB | **~12–16 GB** |
| **Tyskland** | ~4,8 GB | **~14–18 GB** |
| **USA** | ~12 GB | **~36–48 GB** |

Tommelfingerregel: hold ca. **3–4×** `.osm.pbf` ledig. Foretrekk **regionalt**
uttrekk (f.eks. Østlandet ~0,4 GB PBF) på 4 GB RAM-enheter.

### Påkrevde tiltak

1. Begrens standard last til regionale uttrekk; landsskala er valgfritt + advarsel.
2. Lagre omvektet graf etter øko-omvekt — ikke beregn på nytt ved hver oppstart.
3. Strøm/flis DEM-oppslag via LRU-flisbuffer.
4. Kjør grafparse/bygg på bakgrunnstråd (ruting-nivå) med fremdrifts-UI.

Arbeiderpooler må bruke `std::thread::available_parallelism()` (eller tilsvarende)
og la det være rom for lyd/UI. Rutingarbeid kjører med lavere OS-prioritet enn
lyd/UI.

## Arbeidsområdets struktur

- `core/` (`driver-break-core`) — høyde, ruting, POI, hvile/sikkerhet, søk, ikoner, spor, SQLite.
- `navi-ffi/` — UniFFI CDYLIB for Android og andre verter.
- `app/` — Android-vert (Kotlin/Compose) som kobler til kjernen via UniFFI.
- `plugin-host/` / `plugin-sdk/` / `plugins/` — sandkasse WASM-vert (innholdsplugins utsatt; se [`docs/plugins.md`](docs/plugins.md)).
- Hvordan kasser og databaser henger sammen: [`architecture.md`](architecture.md).
- `test-results.md` / `android-test-results.md` — integrasjonsrapporter.

## Vertstester

```bash
cargo test -p driver-break-core --test planner_options_routes
cargo test --test kongsvinger_lillehammer_integration -- --nocapture --ignored
cargo test --test dnt_hiking_integration -- --nocapture --ignored
cargo test -p navi-plugin-host --test isolation -- --nocapture
cargo test -p driver-break-core fishing -- --nocapture
cargo test -p driver-break-core osm_update::
```

## Kjente problemer

- **Plugins (innhold):** WASM-vert/sandkasse er klar; produktplugins er bevisst
  utsatt for uavhengige bidragsytere — se [`docs/plugins.md`](docs/plugins.md).
  Ikke en feil i navigasjonskjernen.
- **GUI-puss:** Compose HUD / søk / verktøy fungerer, men trenger fortsatt
  visuelt og UX-puss (avstand, typografi, tetthet på Automotive-skjermer).
  Bidrag er velkomne.
- **Bevegelige ikoner (APRS):** fikset. Instrumentert test
  `MovingIconInstrumentedTest` består. Markører tegnes via Compose
  skjermplass-overlegg. Se [`docs/bilder.md`](docs/bilder.md) og
  [`docs/real-hardware-testing.md`](docs/real-hardware-testing.md).
- **Kartrotasjon SIGSEGV (emulator GLES):** fikset ved bytte til
  `org.maplibre.gl:android-sdk-vulkan` 11.8.8. Se
  [maplibre-native#2371](https://github.com/maplibre/maplibre-native/issues/2371).
  Verifisert i [`docs/bilder.md`](docs/bilder.md).
