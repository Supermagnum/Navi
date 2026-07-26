**[English README](README.md)**

# Testere ønskes

**Testere ønskes** for testing på **ekte maskinvare** (Android Automotive /
skjermenheter). Utviklingen så langt er kun på emulator — ekte enheter oppfører
seg annerledes for GPS, MapLibre, Vulkan/GLES og ytelse. Sjekkliste:
[`docs/real-hardware-testing.md`](docs/real-hardware-testing.md).
Hvordan bidra (testing og mer): [`CONTRIBUTING.md`](CONTRIBUTING.md).

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

Mer å lese i depotet: hvordan delene henger sammen i
[`docs/architecture.md`](docs/architecture.md); plugin-idéer i
[`docs/plugins.md`](docs/plugins.md); Android-byggesteg i
[`docs/android-build.md`](docs/android-build.md); Linux-kjernebygg i
[`docs/build-linux.md`](docs/build-linux.md); feilsøking i
[`docs/debugging.md`](docs/debugging.md); HUD-layout i
[`docs/hud-layout.md`](docs/hud-layout.md); kartstiler / frakoblede kart / 3D i
[`docs/map-styles.md`](docs/map-styles.md); lastebil kjøre-/hviletidsregler i
[`docs/ec-561-truck-rest.md`](docs/ec-561-truck-rest.md); US FMCSA i
[`docs/fmcsa-truck-rest.md`](docs/fmcsa-truck-rest.md); land-/regionregelpakker i
[`docs/jurisdiction-rules.md`](docs/jurisdiction-rules.md); IMU-monteringskalibrering (utsatt) i
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
pausestopp langs planlagt rute (fasiliteter / hytter / telt / hotell /
rasteplass avhengig av profil), anvende kjøretøygrenser og unngå
hovedvei/bom/ferge ved planlegging, og valgfritt **følge offisielle
tur-/sykkelnettverk** (myk preferanse, av som standard). Du kan sette
bil-pauseintervaller. Lastebilprofiler bruker EC 561/2006 kjøre-/hviletidsregler
med flerdagers døgn-/ukeshvile når turen varer lenger enn én pliktdag. Den har
et minnebasert bevegelig-ikon-lager (`TrackStore`) og en sandkasse
WASM-pluginvert for fremtidige plugins; plugins er ikke laget ennå
([`docs/plugins.md`](docs/plugins.md)).

## Funksjoner

| Funksjon | Hva du får | Status |
|---|---|---|
| **Reisemåter** | Bil, motorsykkel, sykkel, fottur, lastebil og bobil. Elektriske varianter finnes til senere bruk; hovedknappene er hverdagsmodusene. | Ferdig |
| **Kjøretøystørrelse** | Lagre høyde, bredde, lengde, aksellast og lignende. Ruter unngår veier kartet sier er for trange eller for lave for kjøretøyet ditt. | Ferdig |
| **Unngåelser** | Slå på unngå motorvei, bom eller ferge — den planlagte ruten endres faktisk. | Ferdig |
| **Følg offisielle nettverk** | For fottur og sykling: foretrekk merkede langturer og sykkelruter når valget er på (av som standard). Vanlige stier forblir tilgjengelige, så et hull i merket nett aldri stopper hele turen. Navngitte ruter er søkbare. | Ferdig |
| **Økoruting** | Foretrekk ruter som bruker mindre energi ved å ta hensyn til bakker. Elektriske modus får kreditt for energi tilbake i nedoverbakke. Formler: [`docs/mathematical-formulas.md`](docs/mathematical-formulas.md). | Ferdig |
| **Frakoblet ruteplan** | Last ned en kartregion én gang, planlegg på enheten, og se ruten pluss foreslåtte stopp på kartet. | Ferdig |
| **Stedssøk** | Søk steder og sett Fra / Via / Til ([`docs/poi.md`](docs/poi.md)). Inkluderer fiskeplasser og veiledning for hytteradius ([`docs/poi-search-defaults.md`](docs/poi-search-defaults.md)). | Ferdig |
| **Hvile og pauser** | Pausepåminnelser og foreslåtte stopp langs ruten. Fottur og sykling bruker tradisjonelle skandinaviske rasteavstander ([bakgrunn](docs/historisk-bakgrunn.md)). **Lastebil** / **lastebil elektrisk** velger jurisdiksjonspakke ved start: EU EC 561/2006 ([`docs/ec-561-truck-rest.md`](docs/ec-561-truck-rest.md)) eller US FMCSA ([`docs/fmcsa-truck-rest.md`](docs/fmcsa-truck-rest.md)); ukjent jurisdiksjon avslår juridisk sporing. Flerdagers dagskort vises i plan-UI. **Bil** / **motorsykkel** / **sykkel** / **bobil** bruker myk flerdagers overnatting når turen overstiger et daglig budsjett (8 t kjøring eller 100 km sykling) med hotell/camping/rasteplass-forslag ([`docs/poi.md`](docs/poi.md)). Fottur-pausestopp prefererer hytter/telt og holder avstand til hus og isbreer; dag-for-dag flerdagers overnatting planlegges i `planHikingRoute`. Land-/regionpakker: [`docs/jurisdiction-rules.md`](docs/jurisdiction-rules.md). | Ferdig |
| **Kjørefelt (HUD)** | Slim toppstripe (høyde; trykk for kartinnstillinger) og bunnstripe (zoom, pausetid, tur-ETA, økoblad; trykk for kjøreinnstillinger). | Ferdig |
| **Kartrotasjon** | Rett kartet etter kompass, etter kjøreretning, eller med nord alltid opp. | Ferdig |
| **Bevegelige ikoner** | Vis nærliggende spormarkører på kartet (for eksempel radiostasjoner) innen ca. 50–150 km. | **Delvis** — tegning virker; live radiomating er ikke innebygd ennå |
| **Kartoppdateringer** | Når du velger det: sjekk OpenStreetMap-oppdateringer og bruk dem, eller last en fersk region ([`docs/osm-updates.md`](docs/osm-updates.md)). Aldri stille i bakgrunnen. | Ferdig |
| **Plugins** | Sandkasse-WASM-vert er klar. Produktplugins er ikke levert ennå med vilje; flere er spesifisert for bidragsytere ([`docs/plugins.md`](docs/plugins.md) — camping, forsyning, instrumentcluster/AGL, UI-oversettelse, ECU, APRS, …). | Vert klar; innhold utsatt |

**Ekte maskinvare:** Så langt er appen utviklet og sjekket hovedsakelig på Android
Automotive-**emulatoren**. Den **må fortsatt testes på ekte bilskjermer** før
noen behandler den som klar til levering — GPS, sensorer, grafikk og hastighet
skiller seg på ekte biler. Sjekkliste:
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
Land-/regionsuttrekk av PMTiles kan forberedes med
[PMT-splitter](https://github.com/Supermagnum/PMT-splitter).

## Slik fungerer funksjonene

**Følg offisielle nettverk (fottur / sykling).** Av som standard. Når på, foretrekker
planleggeren merkede tur- og sykkelnett der de finnes, men kan fortsatt bruke
vanlige stier, så et manglende stykke merket løype aldri stopper hele turen.
Vanskelighetsmerknader kan komme som ekstra info på planen. Navngitte offisielle
ruter er med i stedssøk. Ikke ennå: å foretrekke høyere nettverksnivå over lokale,
og enkelte «nodenett»-stiler brukt i deler av Europa.

**Slik planlegges en rute.** Du laster ned et regionalt OpenStreetMap-uttrekk én
gang. Navi bygger et veinett fra det. Med øko på endrer bakker hvor «dyrt» hvert
veistykke er, og resultatet bufres så neste plan går raskere. Appen finner en
vei og tegner den på kartet med destinasjon og eventuelle foreslåtte pauser.

**Øko vs kortest.** Korteste vei ignorerer bakker. Øko foretrekker lavere
energibruk, så bratte stigninger koster mer. Bensin- og dieselmodus behandler
ikke nedoverbakke som «gratis»; elektriske modus får delvis kreditt for energi
gjenvunnet i nedoverbakke. Hvis en bilcomputer (OBD / lignende) kobles til
senere, kan live drivstofforbruk forbedre dette ([`docs/ECU.md`](docs/ECU.md)).
Uten det i dag kan appen lære av tankstørrelse og påfylt drivstoff.

**Steder og søk.** Hva som teller som kafe, hytte, fiskeplass og så videre står i
[`docs/poi.md`](docs/poi.md). Foreslåtte søkeavstander for nettverkshytter og
løyper står i [`docs/poi-search-defaults.md`](docs/poi-search-defaults.md).
Søketreff setter Fra / Via / Til og flytter kartet. Grunnkartet viser egne
etiketter; appmarkører bruker medfølgende ikoner.

**Hvile og overnatting.** Hver reisemåte har egne pausestandarder. Bil og
motorsykkel bruker timer mellom pauser; fottur og sykling bruker tradisjonelle
skandinaviske rasteavstander
([`docs/historisk-bakgrunn.md`](docs/historisk-bakgrunn.md));
**lastebil** / **lastebil elektrisk** følger EU EC 561/2006
([`docs/ec-561-truck-rest.md`](docs/ec-561-truck-rest.md)), inkludert flerdagers
døgnhvile (11 t / redusert 9 t / delt 3+9), ukeshvile etter høyst seks
påfølgende arbeidsdager når turen ikke får plass i gjenværende dagsbudsjett,
**kompensasjonsbok** etter redusert ukeshvile (Art. 8-shortfall + frist,
synlig i planrapporten), og **omvei-/anleggsbasert** overnattingsskåring
(`highway=services` foretrekkes fremfor bare rasteplasser innenfor lignende
omvei). **Bobil** beholder bil-lignende myke påminnelser (ikke HGV-juridisk sporing).
Når bil / motorsykkel / bobil / sykkeltur overstiger det myke daglige budsjettet
(standard **8 t** kjøring eller **100 km** sykling), deler planleggeren turen i
dager og foreslår overnatting ved hotell, camping eller rasteplass nær
dagsgrensen (informativt hvis ingen POI finnes — se [`docs/poi.md`](docs/poi.md)
**Lodging** / **RestArea**). For fottur plasserer `planHikingRoute`
hytte-/teltpauser langs rastintervaller, avviser overnattingskandidater for
nær hus eller isbreer, og når turen overstiger daglig distansebudsjett
(standard **40 km**) deler den i dager med overnattingshytter nær
dagsgrensen (`plan_hiking_multi_day` i core; samme skåringsånd som
DNT-integrasjonshjelperen).
Bygningsavstanden følger norsk **allemannsrett**: villcamping er vanligvis lov
når du holder respektfull avstand til hus og dyrket mark. Det er en
Norge-orientert standard og **gjelder ikke nødvendigvis andre steder** — lokal
campinglov kan være strengere; landpakker følger
[`docs/jurisdiction-rules.md`](docs/jurisdiction-rules.md). Bryteren «Pauser»
styrer bare om påminnelsen vises; rediger tider i kjøreinnstillinger (bil vs
lastebil når lastebilprofil er valgt).

**Kart og skjermstriper.** Kartet tegnes med MapLibre. Kollapset toppstripe viser
høyde; trykk for kartinnstillinger (rotasjon, tur-ETA, pauser, auto-zoom,
3D-hillshade, kameratilt). Kollapset bunnstripe viser zoom, pausetid, navnet på veien eller gaten du er på, tur-ETA og
økoblad; trykk for reisemodus, hvile og drivstoff. **Fotturruter krever at
reisemodus Hiking/Fottur er valgt** — planlegging med bil (eller annen
motorprofil) bruker veinettet og vil feile eller gi ubrukelig sti for fotstier.
Nær en sving viser en kort instruksjonsboks manøver, avstand og neste gate
([`docs/approach-instructions.md`](docs/approach-instructions.md)).

**Høyde på emulatoren.** Automotive-emulatorens GPS-høyde er ofte feil (for
eksempel 0 m eller et stort avvik på et kjent sted). Det er en
**emulatorbegrensning**, ikke en appfeil. Høydevisningen foretrekker
terrenghøyde fra nedlastede høydefiler når de finnes; på ekte enhet kan
GPS-høyde brukes når slike filer mangler.

**Bevegelige markører.** Nærliggende sporstasjoner kan vises på kartet og
forsvinne når de er utdaterte ([`docs/APRS.md`](docs/APRS.md)). Live
radiodekoding er ikke med ennå; USB-SDR er planlagt
([`docs/APRS-SDR.md`](docs/APRS-SDR.md)).

## Innstillinger

**Språk i appen:** brukergrensesnittet er foreløpig **kun engelsk**. Det finnes
**ingen språkbryter** i kart- eller kjøreinnstillinger. Norsk tekst her og i
andre markdown-filer er **dokumentasjon**, ikke oversatte appstrenger. En
fremtidig UI-oversettelsesplugin er spesifisert i
[`docs/plugins/i18n-translation-spec.md`](docs/plugins/i18n-translation-spec.md).

Innstillinger lagres under appens datakatalog: hvile/drivstoff/kjøretøy via
SQLite (UniFFI `load*` / `save*`); kart-HUD (auto-zoom, 3D, tilt, pausevisning)
via `MapHudPrefs`. **Save** skriver og lukker; **Close** lukker (kartbrytere
kan allerede være lagret umiddelbart).

### Kart- / visningsinnstillinger (trykk topp-HUD)

| Innstilling | Hva den gjør |
|---|---|
| **Compass** | Roterer kartet etter magnetisk / kompasskurs |
| **Travel** | Roterer kartet etter GPS- (eller simulert) kjøreretning |
| **N-up** | Holder kartet med nord opp (bearing 0°) |
| **Trip ETA** | Viser gjenstående tur-ETA på bunnlinjen |
| **Breaks** | Viser pausepåminnelsen på bunnlinjen (`Break in …` / av). Endrer ikke planlagt stoppavstand alene |
| **Auto-zoom** | Når på, snapper zoom til satt nivå under bevegelse |
| **Auto-zoom − / +** | Endrer måzoom i 0,5-steg (ca. z 3–20) |
| **3D (experimental)** | Valgfri Mapterhorn DEM-**hillshade** (Vulkan-port). Uavhengig av kameratilt; se [`docs/map-styles.md`](docs/map-styles.md) |
| **Map tilt** | Snapper kameravinkel til **0° / 35° / 45° / 65°** (Vulkan-port; låst til 0° uten Vulkan). Fungerer med 3D på eller av |
| **Save / Close** | Lagrer kart-HUD-preferanser og lukker, eller lukker |

**Estimater før avreise** bruker skiltet `maxspeed` (med veiklasse-reserve) for
motorprofiler, og fast tempo (ca. 16 min/km fottur, ~4 min/km sykling) for
fottur/sykling. De er startestimater; live fremdrift oppdaterer når GPS /
simulering gir hastighet.

### Bunn-HUD

| Kontroll | Hva den gjør |
|---|---|
| **Zoom − / +** | Appens egen kartzoom (AAOS klima − 63 + er ikke zoom) |
| **Pause- / ETA-linjer** | Tid (eller avstand) til neste pause, og tur-ETA når aktivert |
| **Øko-blad** | Synlig når økomodus er på for aktiv profil |
| **Trykk statusområdet** | Åpner kjøre- / kjøretøyinnstillinger (ikke zoom-knappene) |

### Kjøre- / kjøretøyinnstillinger (trykk bunn-HUD)

| Innstilling | Hva den gjør |
|---|---|
| **Travel mode** | **Car / Bicycle / Hiking / Motorcycle / Truck / Mobile home.** Velger planlegger og hvilepakke. **Hiking må være valgt for fotturruter** — ellers brukes veinettet og planlegging feiler eller misrouter stier |
| **Hours between breaks** | Mykt bilintervall (eller lastebilens «pause etter X timer»). Lagres som profilstandard, ikke engangsoverstyring |
| **Rest time (minutes)** | Foreslått hvilengde (lastebil: sammenhengende pauselengde) |
| **Split break 15+30** | Bare lastebil — foretrekk delt pause i stedet for én sammenhengende |
| **Arm +1 h exceptional** | Bare lastebil — eksplisitt valg for eksepsjonell forlengelse |
| **Next break shown as Time / Distance** | Bunnlinjens pause som minutter eller km/mi ved antatt cruisehastighet |
| **Distance units km / miles** | Når pause-som-avstand er på, metrisk eller imperial |
| **Eco mode** | Terrengbevisst energikost; blad på bunn-HUD. Fottur/sykling låser øko på; motorprofiler kan veksle |
| **Fuel units liters / gallons** | Visningspreferanse; lagring er liter |
| **Fuel tank capacity** | Tankstørrelse for adaptive forbruksheuristikker |
| **Fuel added** | Siste påfylling for samme heuristikker |
| **Save / Close** | Skriv hvile/drivstoff til SQLite og lukk, eller lukk uten den lagringen |

### Rute- / verktøypanel (hovedkrom)

| Innstilling | Hva den gjør |
|---|---|
| **Follow official hiking/cycling networks** | Myk preferanse for merkede nettverk (av som standard). Bare fottur/sykling |
| **Avoid motorways / tolls / ferries** | Endrer neste motorplan (ikke bare rapporttekst) |
| **Vehicle limits** | Høyde, bredde, lengde, aksel/boggi/vekt m.m. Motorplaner utelukker OSM-kanter som bryter frihøyde |

### Spor (APRS-stil)

| Innstilling | Hva den gjør |
|---|---|
| **Display range** | Vis stasjoner innen **50–150 km** (begrenset; ingen ubegrenset global) |
| **Station timeout** | Fjern utdaterte stasjoner etter maks **3600 s** |

Mer detalj: [`docs/architecture.md`](docs/architecture.md),
[`docs/codebase-map.md`](docs/codebase-map.md), [`docs/API.md`](docs/API.md),
[`docs/hud-layout.md`](docs/hud-layout.md), [`docs/real-hardware-testing.md`](docs/real-hardware-testing.md).

## Fungerende app (emulatorskjermbilder)

Tatt på Android Automotive-emulator med MapLibre + OpenFreeMap liberty.
Kollapset topp-/bunn-kjøre-HUD (søk skjult):

![Idle begge linjer](docs/images/hud/hud_idle_both_bars.png)

Bilrute Helgøya → Atnbrua på Automotive-emulatoren (HUD viser høyde;
AVD GNSS-høyde er ofte feil — se merknad over). Én rast er synlig:

![Helgøya til Atnbrua-rute](docs/images/terrain/hike_eldabu_ramshogda_3d.png)

Kartkamera-tilt (0° / 35° / 45° / 65°) er uavhengig av valgfri 3D-hillshade.
Hamar kort løkke ved **45°** — flat 2D, deretter 3D på:

![45° tilt, 3D av](docs/images/tilt45_3d_off.png)

![45° tilt, 3D på](docs/images/tilt45_3d_on.png)

Alle andre skjermbilder (kartzoom, ruteoverlegg, menyer, innstillinger,
øko-blad, rotasjon, kurs, bevegelige ikoner):
[`docs/bilder.md`](docs/bilder.md).

## Dokumenter

| Dokument | Beskrivelse |
|---|---|
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | Hvordan bidra (testing, plugins, jurisdiksjoner, kodekrav; engelsk) |
| [`docs/architecture.md`](docs/architecture.md) | Hvordan delene henger sammen (databaser, tråder, plugins) |
| [`docs/rust-crates.md`](docs/rust-crates.md) | Rust-crates: egen kode vs uendrede crates.io-avhengigheter (engelsk) |
| [`docs/codebase-map.md`](docs/codebase-map.md) | Bidragsyter-filkart: hvor feil fikses, zoom, tilnærming, ruting, HUD |
| [`docs/bilder.md`](docs/bilder.md) | Emulatorskjermbildegalleri (norsk) |
| [`docs/pictures.md`](docs/pictures.md) | Emulatorskjermbildegalleri (engelsk) |
| [`docs/historisk-bakgrunn.md`](docs/historisk-bakgrunn.md) | Rast/vei-grunnlag for standard pauseintervaller (fottur og sykling); [engelsk](docs/historical-background.md) |
| [`docs/ec-561-truck-rest.md`](docs/ec-561-truck-rest.md) | Lastebil EU kjøre-/hviletid: duty-tak, flerdagers hvile, kompensasjonsbok, overnattingsskåring |
| [`docs/fmcsa-truck-rest.md`](docs/fmcsa-truck-rest.md) | Lastebil US FMCSA HOS (11 t / 14 t / 8 t pause / 70 t-syklus) |
| [`docs/jurisdiction-rules.md`](docs/jurisdiction-rules.md) | Mønster for land-/regionavhengige regler (EC 561 + FMCSA + allemannsrett) |
| [`docs/horse-profile.md`](docs/horse-profile.md) | Arbeidseksempel: legge til Horse-profil (kun dokumentert; ikke implementert) |
| [`docs/hud-layout.md`](docs/hud-layout.md) | Størrelse og plassering av kjøre-HUD og menyer |
| [`docs/map-styles.md`](docs/map-styles.md) | Online Liberty vs frakoblet Protomaps PMTiles; 3D-port |
| [`docs/approach-instructions.md`](docs/approach-instructions.md) | Midlertidig manøver-tilnærmingsboks |
| [`docs/current-street.md`](docs/current-street.md) | Bunn-HUD «Currently on …» veinavn + policy uten rute |
| [`docs/unicode-road-names.md`](docs/unicode-road-names.md) | UTF-8-pipeline for æ/å/ø/ä/ü i navn |
| [`docs/poi.md`](docs/poi.md) | Søkbar POI-kategorier (Fishing, RestArea, Lodging, …), OSM-taggregler |
| [`docs/poi-search-defaults.md`](docs/poi-search-defaults.md) | Foreslåtte hytte-/løyperadius for fottur og sykling (DNT-avstand) |
| [`docs/osm-updates.md`](docs/osm-updates.md) | Valgfri Geofabrik-sjekk / `.osc.gz` / full nedlasting |
| [`docs/plugins.md`](docs/plugins.md) | Plugin-vert-status (bevisst: ingen innholdsplugins ennå) + HostApi, isolasjon, veikart |
| [`docs/plugins/right-to-roam-camping-spec.md`](docs/plugins/right-to-roam-camping-spec.md) | Spesifikasjon: allemannsrett / flerland villcamping (plugin) |
| [`docs/plugins/safety-resupply.md`](docs/plugins/safety-resupply.md) | Spesifikasjon: drivstoff-/vannforsyning langs rute (plugin) |
| [`docs/plugins/instrument-cluster-agl-spec.md`](docs/plugins/instrument-cluster-agl-spec.md) | Spesifikasjon: eksport av nav-tilstand til cluster/AGL (plugin) |
| [`docs/plugins/i18n-translation-spec.md`](docs/plugins/i18n-translation-spec.md) | Spesifikasjon: UI-språk/oversettelsespakker (plugin; appen er engelsk i dag) |
| [`docs/plugins/animated-icons-spec.md`](docs/plugins/animated-icons-spec.md) | Spesifikasjon: animerte ikoner (Synfig → SVG-rammer; plugin) |
| [`docs/icons.md`](docs/icons.md) | Ikonoversikt; egne statiske SVG (Inkscape); Navit GPL-v2 |
| [`docs/API.md`](docs/API.md) | UniFFI vert-API + plugin HostApi-referanse |
| [`docs/PROTOCOLS.md`](docs/PROTOCOLS.md) | Protokollindeks |
| [`docs/ECU.md`](docs/ECU.md) | ECU-protokoller: OBD-II, J1939, MegaSquirt + EV |
| [`docs/mathematical-formulas.md`](docs/mathematical-formulas.md) | Formler: MAF/J1939/MegaSquirt-drivstoff, rekkevidde, øko-segmentenergi |
| [`docs/APRS.md`](docs/APRS.md) | APRS-felter, TrackStore-filtrering, bevegelige ikoner |
| [`docs/APRS-SDR.md`](docs/APRS-SDR.md) | APRS SDR DSP; RTL-SDR; planlagt `rtl-sdr-rs` |
| [`docs/CAT.md`](docs/CAT.md) | CAT VFO auto-tune fra NFM-repeatere |
| [`docs/voice-guidance.md`](docs/voice-guidance.md) | Planlagt stemmeveiledningsplugin |
| [`docs/android-build.md`](docs/android-build.md) | Bygg native `libnavi.so`, UniFFI og Gradle-APK |
| [`docs/build-linux.md`](docs/build-linux.md) | Linux: Rust-kjerne, `navi-desktop` kartskall, integrasjonstester, gpsd + IMU |
| [`docs/imu-calibration.md`](docs/imu-calibration.md) | Utsatt: IMU pitch/roll-nullstilling for øko-høyde |
| [`docs/debugging.md`](docs/debugging.md) | Vert- + Android-feilsøkingsløkker |
| [`docs/real-hardware-testing.md`](docs/real-hardware-testing.md) | **Påkrevd:** sjekkliste for fysisk enhet vs emulator |
| [`docs/test-results.md`](docs/test-results.md) | Vert-integrasjonstestnotater |
| [`docs/android-test-results.md`](docs/android-test-results.md) | Resultater på enhet / emulator |

## Ikoner (Navit)

Se [`docs/icons.md`](docs/icons.md) for full ikonsystembeskrivelse. Kort:
POI-/manøver-/statusikoner under `core/src/icons` er Navit-avledet (**GPL v2**).
Oppløsning foretrekker brukeroverskrivninger, deretter medfølgende sett, deretter
`unknown.svg`.

**Egne ikoner:** bruk **SVG** (eller `.svgz`). Statisk kunst i
[Inkscape](https://inkscape.org/); navngi etter semantisk nøkkel og legg i
override-katalog eller `core/src/icons` — steg i
[`docs/icons.md`](docs/icons.md#adding-custom-icons). Animasjoner i
[Synfig Studio](https://www.synfig.org/) — se
[`docs/plugins/animated-icons-spec.md`](docs/plugins/animated-icons-spec.md).

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
- Hvordan delene henger sammen: [`docs/architecture.md`](docs/architecture.md).
- Hvor funksjoner endres / feil fikses: [`docs/codebase-map.md`](docs/codebase-map.md).
- Kallbare API-er: [`docs/API.md`](docs/API.md).
- [`docs/test-results.md`](docs/test-results.md) /
  [`docs/android-test-results.md`](docs/android-test-results.md) — integrasjonsrapporter.

## Vertstester

```bash
cargo test -p driver-break-core --test planner_options_routes
cargo test -p driver-break-core --test truck_driving_history -- --nocapture
cargo test -p driver-break-core truck_multi_day -- --nocapture
cargo test -p driver-break-core motor_multi_day -- --nocapture
cargo test -p driver-break-core rest_area -- --nocapture
cargo test -p driver-break-core lodging -- --nocapture
cargo test --test kongsvinger_lillehammer_integration -- --nocapture --ignored
cargo test --test dnt_hiking_integration -- --nocapture --ignored
cargo test -p navi-plugin-host --test isolation -- --nocapture
cargo test -p driver-break-core fishing -- --nocapture
cargo test -p driver-break-core osm_update::
```

**Live-GPS lastebilplan (vert):** startkoordinater må komme fra
`adb shell dumpsys location` (ingen hardkodede korridorstarter). Sett
`NAVI_START_LAT` / `NAVI_START_LON` fra den fiksasjonen, velg destinasjon først
etter at start er kjent, deretter:

```bash
cargo run -p navi-ffi --bin plan-truck-live-gps --release
```

Se `navi-ffi/src/bin/plan_truck_live_gps.rs`. Flerdagers døgnhvile og
historikk les/skriv er bekreftet på en live-GPS Norge-tur (Minnesund-beltet →
Bodø).

## Kjente problemer

- **Plugins (innhold):** WASM-vert/sandkasse er klar; produktplugins er bevisst
  utsatt for uavhengige bidragsytere (camping, forsyning, instrumentcluster/AGL,
  UI-oversettelse, ECU, APRS, …) — se [`docs/plugins.md`](docs/plugins.md).
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
