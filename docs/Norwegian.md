**[English README](../README.md)**

# AI-bistand

Dette prosjektet er laget med hjelp fra AI-verktøy (Cursor). Forfatteren har en
nevrologisk tilstand knyttet til dyskalkuli som gjør programmering vanskeligere
på en måte som ligner hvordan dyskalkuli gjør matematikk vanskeligere. AI ble
brukt til å omsette designidéer til fungerende kode og dokumentasjon.
Forfatteren har likevel valgt produktreglene, gjennomgått arbeidet og styrt
testen.

# Testere ønskes

Vi trenger folk som prøver Navi på **ekte enheter** — bilskjermer, nettbrett og
telefoner. Referansesjekker så langt: Samsung Galaxy Tab S6 Lite (**SM-P613**) og
Google Pixel 9a (**tegu**, kamerahull / API 36+). Biler og andre formater
oppfører seg fortsatt annerledes for GPS, kart, GPU og layout. Sjekkliste:
[`real-hardware-testing.md`](real-hardware-testing.md).
Resultater på enhet og emulator:
[`android-test-results.md`](android-test-results.md).

**Oversettere ønskes.** UI-språkpakker er spesifisert, men ikke levert (bare
engelsk i menyene i dag). Fyll eller gjennomgå arbeidstabellen og følg
spesifikasjonen:
[`plugins/i18n-translation-spec.md`](plugins/i18n-translation-spec.md)
(katalog: [`plugins/translations.csv`](plugins/translations.csv);
ord/frasesammenheng:
[`plugins/translations-context.md`](plugins/translations-context.md)).
Engelsk-kolonnen lister land/regioner i overskriften; dialektkolonner følger
`land, - område, - dialekt` (se spesifikasjonen). Ikke innfør en språkbryter
før den pluginen finnes.

**Hvordan bidra:** testere, dokumentasjon, oversettelser og kode starter i
[`CONTRIBUTING.md`](CONTRIBUTING.md) (engelsk). Den siden forklarer hvordan du
**forker repoet og jobber på `dev`** (ikke `main`), grunnleggende GitHub-bruk
(klone din fork, synke med upstream, åpne pull request mot `dev`), og hva slags
hjelp som er nyttig. Testere kan følge sjekklisten over og åpne et issue; du
trenger ikke skrive kode.

## Innhold

1. [Hva dette er](#hva-dette-er)
2. [Støtt Navi](#støtt-navi)
3. [Funksjoner](#funksjoner)
   - [Hva du må laste ned](#hva-du-må-laste-ned)
   - [Indeksering (bakgrunn etter nedlasting)](#indeksering-bakgrunn-etter-nedlasting)
   - [Når du forlater en nedlastet region](#når-du-forlater-en-nedlastet-region)
   - [Slik bruker du](#slik-bruker-du)
   - [Slik fungerer funksjonene](#slik-fungerer-funksjonene)
4. [Innstillinger](#innstillinger)
5. [Pauseteller vs tur-ETA](#pauseteller-vs-tur-eta)
6. [Ruting og sikkerhet](#ruting-og-sikkerhet)
7. [Minimum maskinvare og lagring](#minimum-maskinvare-og-lagring)
8. [Skjermbilder](#skjermbilder)
9. [Dokumenter](#dokumenter)
10. [Plugins](#plugins)
    - [Ikoner (hvor de ligger)](#ikoner-hvor-de-ligger)
11. [Kodestandarder og bidrag](#kodestandarder-og-bidrag)
12. [Bygge og installere](#bygge-og-installere)
    - [Installer en forhåndsbygd APK](#installer-en-forhåndsbygd-apk)
    - [Utgivelsesbygg (APK / AAB)](#utgivelsesbygg-apk--aab)
13. [Hvor kartdataene kommer fra](#hvor-kartdataene-kommer-fra)
14. [Kjente problemer](#kjente-problemer)
15. [TODO](#todo)

Mer detalj ligger i lenkede dokumenter (arkitektur, lastebilhvile, kartstiler,
feilsøking osv.). For å bidra, start med
[`CONTRIBUTING.md`](CONTRIBUTING.md) (engelsk): fork fra **`dev`**,
GitHub-grunnlag, og hva du kan jobbe med.

# Hva dette er

**Navi** er en navigasjonsapp som er laget for **å fungere uten nett** når
kartfilene først er lastet ned. Du laster ned kartdata én gang, og planlegger
deretter ruter på enheten uten å trenge internett for hver tur.

Den kan:

- Planlegge ruter for bil, sykkel, elsykkel, fottur, motorsykkel, lastebil og bobil
- Foretrekke veier som bruker mindre energi når **økomodus** er på (bakker teller)
- Foreslå rastestopp og overnatting på lengre turer
- Respektere lastebilers kjøre-/hviletidsregler der den kjenner landets regler
- Vise et enkelt kart med rute, svinger og stedsnavn

Kartbildet på skjermen tegnes med MapLibre. På nett kan det bruke OpenFreeMap
Liberty-fliser; frakoblet brukes en nedlastet regional kartfil (Protomaps).
Selve ruting bruker et eget OpenStreetMap-uttrekk du laster ned under
**Tools** — det er «hjernen» som finner veier og stier, ikke bare det pene
kartet.

Lisens: se `LICENSE` (GPL-3.0-or-later med mindre annet er angitt). Mange små
ikoner kommer fra Navit (**GPL v2**); se [`icons.md`](icons.md).

# Støtt Navi

Navi er fri programvare med åpen kildekode, utviklet uavhengig. Om du vil støtte
utviklingen, er donasjoner velkomne via bankoverføring:

- **IBAN:** NO02 1802 0334 084
- **BIC/SWIFT:** SHEDNO22

Skriv «Navi donation» som betalingsreferanse/melding, så det er tydelig på
kontoutskriften.

Dette er helt valgfri støtte, ikke en betalingsmur — Navi er og forblir gratis.

# Funksjoner

| Funksjon | På vanlig norsk | Status |
|---|---|---|
| **Reisemåter** | Velg bil, sykkel, elsykkel, fottur, motorsykkel, lastebil eller bobil. | Ferdig |
| **Kjøretøystørrelse** | Lagre høyde/bredde/lengde/vektgrenser så ruten unngår veier som er for trange. | Ferdig |
| **Elsykkel-data** | Batteristørrelse, motormoment og hjulstørrelse hjelper til å anslå batteribruk og bratte bakker. Live kabel-telemetri er planlagt senere. | Ferdig (planlegging); live data senere |
| **Unngåelser** | Du kan be om å unngå motorvei, bom eller ferge. Motorvei her betyr OSM `highway=motorway` / `motorway_link`, `motorroad=yes` / `expressway=yes`, eller tofelts enveiskjørt veg med `maxspeed>=90` — ikke all E-veg eller by-innfart. | Ferdig |
| **Offisielle løyper** | For fottur/sykling kan du valgfritt foretrekke merkede langturer (av som standard). Vanlige stier fungerer fortsatt hvis merket løype har hull. | Ferdig |
| **Sykkel-underlag** | Sykkel / elsykkel i Kjøre-innstillinger: **Road / Gravel / MTB**. Uegnede OSM-underlag og stier hard-filtreres etter at grafen er lastet (bygger ikke pakker på nytt). Standard er Gravel (trekking). | Ferdig |
| **Motor-underlagspreferanse** | Bil, lastebil, bobil og motorsykkel: myk preferanse for gode kjørbare underlag (`surface` / `tracktype`) på snap-punkter og langs ruten; utaggede `highway=track` behandles forsiktig. Kun intern kostnad — ingen varsler i UI. Standard **Car**; **Offroad** / 4×4 slakker vektingen (config; ingen bryter i Kjøre-menyen ennå). | Ferdig |
| **Økoruting** | Foretrekk ruter som bruker mindre energi ved å ta hensyn til bakker. Et lite bladikon vises når øko er på. | Ferdig |
| **Frakoblet planlegging** | Last ned en region én gang, planlegg og se ruten på enheten. | Ferdig |
| **Indeksering** | Etter regionsnedlasting gjør en bakgrunnsjobb OSM-uttrekket om til kompakte rutingpakker, så senere planer går raskt. Du kan planlegge mens den kjører; konvertering og stedsindeks **pauser** under en forgrunnsplan, slik at PBF-reservestien ikke sulter. | Ferdig |
| **Stedssøk** | Søk steder og sett Fra / Via / Til. Mens stedsindeksen fortsatt er tom/bygges, viser søket en byggehint (koordinater og karttrykk fungerer fortsatt). | Ferdig |
| **Bruk GPS** | Fyll Fra / Via / Til fra live-posisjon: koordinater kommer med en gang, deretter eventuelt oppgradering til veiknavn i nærheten. Feltet er chipen som var aktiv da du trykket — ikke den som er valgt etter at oppslaget er ferdig. | Ferdig |
| **Kartmerke og lagrede steder** | Hold på kartet ~4 s for å merke et punkt; sett Fra / Via / Til eller lagre et navngitt sted (skilt fra Lagrede ruter). | Ferdig |
| **Avvik / omberegning** | Vedvarende avvik viser **Off route**; motorprofiler omplanlegger automatisk fra live posisjon; fottur spør først. | Ferdig |
| **Pauser og hvile** | Påminner når pause er «forfalt» og kan foreslå stopp. Bil bruker timer mellom pauser; fottur/sykling bruker rasteavstander; lastebil bruker juridiske kjøretidsregler der de er kjent. | Ferdig |
| **Kjørefelt** | Topp: høyde (tilpasset kamerahull; meter, eller fot i US-enhetsprofilen). Bunn: zoom, live GPS-fart, skiltet fartsgrense når kjent, pauseteller, tur-ETA, veinavn, økoblad. Fart og avstand følger **enheter**. Fartslinjen bruker feilfarge ved overskridelse (kun visning — ikke taleskjenning). | Ferdig |
| **Enheter** | Kjøreinnstillinger: **Metric** (km, km/t), **US** (ft / mi, mph, høyde i fot), eller **UK** (yd deretter mi, mph, høyde i meter). Første installasjon utledes én gang fra SIM/nettverksland (GB → UK, US/LR/MM → US, ellers metrisk; emulatorer forblir metriske). Brikkene overstyrer alltid. | Ferdig |
| **GPS-følge** | Kartet følger deg som standard. Panorer bort, trykk deretter **Recenter**. | Ferdig |
| **Kartrotasjon** | Nord opp, kompass eller kjøreretning. | Ferdig |
| **Bevegelige ikoner** | Kan tegne nærliggende spormarkører på kartet. Live radiomating er ikke innebygd ennå. | Delvis |
| **Norske vegskiltvarsler** | Vendoret `NO:`-katalog for tilnærmingsikoner i Norge; eksplisitte OSM `traffic_sign` / `hazard`-tagger. Samme 750 / 150 / 25 m-faser som svinginstruksjoner. Se [`road-signs.md`](road-signs.md). | Ferdig |
| **Look forward / Se fremover** | Uten planlagt rute ser GPS-posisjon + heading **300 m** fremover (±60°) etter katalogskilt, fartshumper (`NO:109`), barnefasiliteter (generisk **142**), opt-in fartskamera og kommende skiltet fartsgrense fra eksisterende veinavn-/cellegraf — samme tilnærmingsboks (750 / 150 / 25 m) og jurisdiksjonsregler som rute-korridoren. Kompakte punkter lastes én gang per region. Detaljer: [`road-signs.md`](road-signs.md). | Ferdig |
| **Barnefasiliteter i nærheten** | Når ingen tagget barn-/skolevarselskilt er aktivt, gir skoler, barnehager og lekeplasser fortsatt et generisk **142 Barn**-tilnærmingsvarsel (nærmeste anlegg; tagget `NO:142` går foran). **Med planlagt rute:** innen **200 m** fra korridoren. **Uten rute:** dekkes av **Look forward** (300 m kjegle). Detaljer: [`road-signs.md`](road-signs.md). | Ferdig |
| **Fartskameravarsler** | Punktkamera bruker samme tilnærmingsfaser; snittfart / strekningskontroll har egen inn-/utboks. Jurisdiksjonsstyrt (Norge/UK opt-in; flere land avslår) — se [`jurisdiction-rules.md`](jurisdiction-rules.md). Første-gangs opt-in-dialog. Virker både på planlagt-rute-korridor og **Look forward**. | Ferdig (kun visning/varsel) |
| **Kartoppdateringer** | Bare når du ber om det — sjekk OpenStreetMap-oppdateringer eller last en fersk region. Aldri i det stille. | Ferdig |
| **Diagnostisk logging** | **Tools → Diagnostic logging** (av som standard). Når på, skrives en datert øktlogg under **Intern lagring → Documents → debug** (`navi_session_*.log`) for kopiering over USB/MTP — adb trengs ikke. Dekker GPS, kamera, brytere, ruteplan/trinn, øko, POI, pauser, instruksjoner, drivstoff, system. Lastes ikke opp. **Export diagnostic log** deler siste fil. Detalj: [Innstillinger → Tools](#tools-nedlastinger-og-diagnostisk-logging) og [`debugging.md`](debugging.md#3b-diagnostic-session-log-on-device-file). | Ferdig |
| **Plugins** | En trygg sandkasse for fremtidige tillegg finnes; produktplugins er ikke levert ennå. | Vert klar |

**Maskinvare:** Ekte enhetssjekker inkluderer Samsung Galaxy Tab S6 Lite
(**SM-P613**) og Google Pixel 9a. Bilskjermer trenger fortsatt mer testing i
virkeligheten før dette behandles som klart til levering. Se
[Skjermbilder](#skjermbilder) og
[`real-hardware-testing.md`](real-hardware-testing.md).

## Hva du må laste ned

Ingenting nyttig ligger ferdig i appen. Bruk **Tools** (med internett), deretter
kan du gå frakoblet.

| Nedlasting | Trengs? | Hva det er | Knapp i Tools |
|---|---|---|---|
| **Kartregion (veier og steder)** | **Ja** for ruting og søk | OpenStreetMap-uttrekk fra [Geofabrik](https://download.geofabrik.de/) (eksempel: `europe/norway/ostlandet`) | **Download region + build place index** |
| **Høyde** | Sterkt anbefalt for øko / bakker | Høydedata for området | Følger vanligvis med regionsnedlasting |
| **Frakoblet grunnkart** | Trengs for kartgrafikk uten nett | Visuelle kartfliser (Protomaps) | **Download basemap (PMTiles)** |
| **3D-terreng** | Valgfritt | Ekstra høydefliser for skyggelegging | **Download terrain DEM (Mapterhorn)** |
| **OSM-oppdateringer** | Valgfritt | Ferskere veier/POI-er | **Check for OSM updates** (aldri automatisk) |

**Minimum for å planlegge rute:** regionsnedlasting + stedsindeks.  
**Minimum for et brukbart frakoblet kartbilde:** det samme pluss basemap-PMTiles
(eller bli på nett med Liberty).  
Foretrekk en **region** (ikke et helt stort land) på nettbrett med begrenset
RAM — se [Minimum maskinvare og lagring](#minimum-maskinvare-og-lagring).

Når regionsfilen ligger på disken, **indekserer** Navi den i bakgrunnen slik at
senere planer går raskt — se
[Indeksering (bakgrunn etter nedlasting)](#indeksering-bakgrunn-etter-nedlasting).

## Indeksering (bakgrunn etter nedlasting)

Når **Download region + build place index** har lagret OpenStreetMap-uttrekket,
starter Navi en **indekseringsjobb i bakgrunnen**. Det er ikke kartbildet på
skjermen (grunnkartfliser) og ikke selve `.osm.pbf`-filen — det er en
engangskonvertering av uttrekket til kompakte **indekserte pakker** som
planleggeren kan laste raskt, i stedet for å skanne hele uttrekket på hver tur.

Du kan søke og trykke **Plan route** så snart nedlastingen er ferdig. Inntil
indekseringen er ferdig, bruker planlegging den tregere rå `.osm.pbf`-stien.
Tools viser fremdrift som **Indexed maps (background)**; når det står
**Indexed maps: ready (pack-hit)**, bruker neste plan pakkene — typisk ca.
1,5–2 sekunder på referansenettbrettet i stedet for titalls sekunder.

Mens **Plan route** (eller auto-omberegning) kjører på PBF-reservestien,
viker bakgrunnskonvertering og stedsindeks, slik at de ikke konkurrerer om
samme uttrekk. GPS-utløste fartsgrense-kjegle- / veinær-bygginger **hopper
over** i det vinduet (én mistet HUD-oppdatering) og fortsetter på neste
fix etter planen. Planfremdrift har egen kanal, så konverterings-/kjegle-
etiketter ikke flytter planlinjen.

Det bakgrunnsjobben skriver:

| Pakke | Hva den brukes til |
|---|---|
| **Vei- / stinett** | Nettverket A* følger, per reisemåte (bil, fot, sykkel osv.). Store regioner deles i romlige fliser, slik at en 4 GB-klasse enhet ikke må holde hele regionen i RAM samtidig. |
| **POI og barrierer** | Hytter, rasteplasser, overnatting, isbrepolygoner og liknende trekk brukt til hvile-/overnattingsplanlegging og fottursikkerhetsfiltre. |
| **Våtmark** | Myr- og vannpolygoner fottur bruker for å holde seg unna myr (klopp/bru blir på grafen). |

En separat **stedsindeks** (navn til Fra / Via / Til-søk) bygges som del av
nedlastingsknappen, før denne bakgrunnsjobben starter.

Hvis pakker mangler, er utdaterte, eller fortsatt konverteres, fungerer
planlegging likevel via PBF-reservestien. Bygg på nytt fra en fil som allerede
ligger på enheten med **Rebuild indexed maps (local PBF, background)** — uten
ny nedlasting. Mer:
[`indexed-map-format-plan.md`](indexed-map-format-plan.md). Minnebuffert på
svakere 4 GB-enheter under konvertering: [Kjente problemer](#kjente-problemer).

## Når du forlater en nedlastet region

En regionsnedlasting (OSM-uttrekk + indekserte pakker) og det frakoblede
grunnkartet dekker bare det uttrekkets område. Navi finner ikke opp veier eller
fliser utenfor det.

**Planlegge en tur som går utenfor dataene dine.** Før **Plan route** sjekkes
From / Via / To mot avgrensningsboksene til nedlastede Geofabrik-uttrekk.

- Hvis et veipunkt ligger utenfor alle nedlastede områder, **blokkeres**
  planlegging (ingen delvis eller gjettet rute).
- Dialogen **Map data needed** foreslår en nedlasting (for eksempel Vestlandet
  eller Nord-Norge). Du kan laste ned derfra, eller avbryte og velge et annet
  mål.
- Hvis From og To trenger **ulike** landsdeler (eller tilsvarende delinger),
  foreslås et **landsuttrekk** (f.eks. Norge). Planleggeren bruker **én**
  regionsfil og syr ikke sammen to uttrekk til én tur.

**Allerede underveis.** Det finnes ingen kontinuerlig «du forlot kartet»-grense
mens du kjører.

- **Grunnkart:** flisene stopper der den nedlastede Protomaps-regionen slutter
  (eller du faller tilbake til Liberty på nett hvis nettverk er tilgjengelig).
- **Veiledning:** følger ruten du allerede har planlagt så lenge du holder deg
  til den.
- **Omruting ved avvik:** bruker det lokale regionsuttrekket på nytt. Utenfor
  det uttrekket kan snap / veifinning feile; du får **ikke**
  nedlastingsdialogen fra planlegging ved automatisk omruting. Last ned den
  dekkende regionen under Tools før du trenger å planlegge på nytt der.

Indekserte pakker matcher uttrekket de ble bygget fra. Å forlate det området
betyr ingen frakoblet graf for nye planer — ikke en myk overgang.

## Slik bruker du

Steg-for-steg brukerveiledning (planlegging, Tools, pauser, lagrede steder/ruter,
per-modus valg, pilegrim):
**[How to use Navi](how-to-use.md)** (engelsk).

Et eksempel på en flersone-biltur (OSM ciderprodusenter, sør til nord):
[`cider-route.md`](cider-route.md) (engelsk).

## Slik fungerer funksjonene

**Planlegge rute.** Sett **From** og **To** (og valgfrie via-punkter), velg
reisemåte, deretter **Plan route**. From settes ofte med **Use GPS** (velg
**From** / **To** / **Via**-chip først; knappeetiketten følger chipen).
Fotstier krever **Hiking**-modus — planlegging med Car bruker veinettet og
følger ikke stier skikkelig.

**Øko vs kortest.** Kortest ignorerer bakker. Øko gjør bratte stigninger
«dyrere». Elektriske modi får noe kreditt for energi tilbake i nedoverbakke.

**Offisielle nettverk.** Valgfri myk preferanse for merkede tur-/sykkelruter
(**Follow official hiking/cycling networks**). Vanlige stier forblir
tilgjengelige, så et hull aldri stenger hele turen. Egne brytere styrer
**nettverkshytter** som via-punkter og om du er **nettverkshytte-medlem** for
overnatting (se [Kjøring / kjøretøy](#kjøring--kjøretøy-trykk-bunnstatus)).

**Steder.** Søk fyller Fra / Via / Til. Hva som teller som hytte, rasteplass
osv. er beskrevet i [`poi.md`](poi.md).

**Kartmerking og lagrede steder.** Hold én finger på kartet i ca. **4 sekunder**
for å merke et punkt, sett det som Fra / Via / Til, eller lagre under
**Saved places** (ett navngitt koordinat — ikke en hel **Saved route**).
Brukerveiledning:
[`kartmerking-lagrede-steder.md`](kartmerking-lagrede-steder.md)
(engelsk: [`map-marking-saved-places.md`](map-marking-saved-places.md)).

**Hvile og overnatting.** Hver modus har egne standarder. Lange lastebilturer
kan deles i dager med juridiske hvileregler (EU- eller US-pakker der de er
kjent). Lange bil-/sykkel-/fotturer kan foreslå overnatting. Fottur-overnatting
filtreres bort fra bygninger og isbreer (1 km til isbreens **polygonkant**);
avviste nåler viser en klar grunn. Med **Network hut member** av (standard)
foretrekkes ikke-nettverkshytter; DNT/STF-lignende nettverkshytte er bare
siste utvei og merkes som medlemskapskrevd. Medlemskap gir **ikke** adgang —
det endrer bare overnattingspreferanse. Bryteren **Breaks** i bunnstripen
viser eller skjuler bare påminnelsen — den lager ikke ny hvilelov.

**Kartstriper.** Trykk toppstripen for kart-/skjerminnstillinger. Trykk
bunnstatusen for kjøre-/kjøretøyinnstillinger (modus, pauseintervall, drivstoff,
elsykkel osv.). Bunnstripen viser også **live GPS-fart / skiltet grense**
(km/t eller mph etter **enheter**) når fiks og gjeldende grense er kjent; overskridelse er bare farge i dag
([`current-street.md`](current-street.md)). Talt, eskalerende varsel er en
**pluginspesifikasjon**, ikke levert:
[`plugins/adaptive-speed-warning-spec.md`](plugins/adaptive-speed-warning-spec.md).

**Barnefasiliteter i nærheten.** Hvis OSM mangler tagget barn-/skolevarselskilt,
varsler Navi likevel når skole, barnehage eller lekeplass er i nærheten —
generisk skilt **142**, samme tilnærmingsboks som andre vegskilt. Langs en
**planlagt rute** betyr det innen **200 m** fra korridoren; ved **live kjøring
uten rute** dekker **Look forward** dem innen **300 m** heading-kjegle.
Eksplisitt tagget `NO:142` (eller tilsvarende) går foran denne reserven. Se
[`road-signs.md`](road-signs.md).

**Look forward / Se fremover.** Uten planlagt rute ser GPS-posisjon + heading
**300 m** fremover (±60°) og styrer samme tilnærmingschrome for katalogskilt,
fartshumper, barnesoner, opt-in kamera og kommende skiltet fartsgrense fra
eksisterende veinavn-/cellegraf. Kompakte punkter lastes én gang per region
(ikke omparses hvert GPS-tick). Detaljer: [`road-signs.md`](road-signs.md),
[`route-simulation.md`](route-simulation.md).

# Innstillinger

**Språk:** appens menyer er **bare engelsk** i dag. Det finnes ingen
språkmeny ennå, og Navi velger **ikke** UI-språk fra GPS eller SIM-land (det
ville overstyre språket brukeren allerede har satt på telefonen). Denne filen
(`docs/Norwegian.md`) er dokumentasjon, ikke en språkpakke i appen. En
fremtidig oversettelsesplugin (valgbare pakker, **tilbakefall til engelsk**
når en nøkkel mangler) er beskrevet i
[`plugins/i18n-translation-spec.md`](plugins/i18n-translation-spec.md).
En arbeids-CSV for oversettere ligger ved siden av den spesifikasjonen:
[`plugins/translations.csv`](plugins/translations.csv).
Sammenheng for ord og fraser:
[`plugins/translations-context.md`](plugins/translations-context.md).
Engelsk-kolonnen lister land/regioner; dialektkolonner bruker
`land, - område, - dialekt` (dokumentert i i18n-spesifikasjonen).

Innstillinger lagres på enheten (hvile/drivstoff/kjøretøy i en liten database;
kartvisning i app-preferanser).

### Kart / skjerm (trykk toppstripe)

| Innstilling | Enkel forklaring |
|---|---|
| **Compass / Travel / N-up** | Hvordan kartet roterer |
| **Snap rotation back to mode** | Etter manuell rotasjon, gå tilbake til valgt modus (på som standard) |
| **Trip ETA** | Vis tid igjen til målet i bunnstripen |
| **Breaks** | Vis linjen «Break in …» (endrer ikke hvordan stopp planlegges) |
| **Auto-zoom** | Hold valgt zoom mens du beveger deg |
| **3D (experimental)** | Valgfri bakkeskygge på kartet |
| **Map tilt** | Tippe kameraet (0° / 35° / 45° / 60°) |

### Kjøring / kjøretøy (trykk bunnstatus)

| Innstilling | Enkel forklaring |
|---|---|
| **Travel mode** | Bil, sykkel, fottur, lastebil, … |
| **Bike type** | Sykkel / elsykkel: **Road / Gravel / MTB** (hard-filtrerer uegnede stier) |
| **Surface routing mode** | Bare bil / lastebil / bobil / motorsykkel: **`car`** (standard) foretrekker gode underlag; **`offroad`** / **`4x4`** slakker vekting. Config/API i dag — ikke bryter i Kjøre-menyen ennå |
| **Follow official hiking/cycling networks** | Fottur / sykkel / elsykkel: myk preferanse for merkede nettverk (vanlige stier fortsatt brukbare) |
| **Use networked cabins** | Fottur / sykkel / elsykkel: tillat DNT/STF-lignende **nettverkshytter** som auto-via / via-kandidater (av som standard). Endrer **ikke** overnattingsmedlemskapsregler |
| **Network hut member (DNT/STF/…)** | Bare fottur: når på, kan overnatting foretrekke nettverkshytter; når av (standard), foretrekk ikke-nettverk og merk nettverkstopp som medlemskapskrevd |
| **Follow pilgrim routes** | Bare fottur; myk preferanse (av som standard), faller tilbake til vanlig fottur |
| **Hours between breaks** | Hvor ofte du *ønsker* pause (bil), eller lastebilens pålagte pause-etter-tid |
| **Rest time** | Hvor lenge pausen bør vare (forslag / lastebil sammenhengende pause) |
| **Next break as Time / Distance** | Vis nedtelling i minutter, eller som km/mi ved antatt cruisehastighet |
| **Units** | Metrisk, US (ft / mph) eller UK (mi / mph). Første-gangs standard fra SIM/nettverksland; alltid overstyrbar. UK-høyde forblir meter (ikke US-fot). |
| **Eco mode** | Energikost med bakker (låst på for fottur/sykling) |
| **POI search radius** | Hvor langt til siden planleggeren kan lete etter hytter / stopp |
| **Vehicle limits** | Høyde/bredde/lengde/aksellast for frihøyde |

Ruteplanlegging (**Route**): From / To / Via, Plan, Simulate, unngåelser
(**Avoid motorways** ekskluderer `highway=motorway` / `motorway_link`, `motorroad=yes` / `expressway=yes`, og tofelts enveiskjørt veg med `maxspeed>=90`; E-veg `ref` er bare visning),
lagrede ruter.

### Tools (nedlastinger og diagnostisk logging)

Åpne **Tools** fra planleggingspanelet (samme skjerm som region-/grunnkart-
nedlasting).

| Innstilling / handling | Enkel forklaring |
|---|---|
| **Download region / basemap / DEM** | Frakoblede kartdata (se hva du må laste ned ovenfor) |
| **Check for OSM updates** / **Apply pending** | Valgfri oppdatering; aldri stille autodownload |
| **Weekly update reminder** | Valgfri påminnelse — laster ikke ned selv |
| **Diagnostic logging** | **Debug-bryter** (av som standard). Når **på**, skriver Navi en rør-separert **øktlogg** på enheten så du kan feilsøke planlegging, GPS og innstillinger uten `adb logcat`. Når **av**, skrives ingen ny øktfil, og native per-trinns ruteplan-timing er også av |
| **Export diagnostic log** | Åpner Androids delingsark for siste øktfil (eller ber deg skru logging på først) |

**Hva loggen er til:** feilrapporter, planleggingstid (`ROUTE_PLAN` /
`ROUTE_PLAN_STAGES`), og bekreftelse av brytere på ekte maskinvare. Den er
**ikke** et logcat-speil, og Navi **laster den ikke opp**.

**Hvor filen ligger** (USB filoverføring / MTP — adb trengs ikke):

```text
Intern lagring → Documents → debug → navi_session_YYYY-MM-DD_HH-mm-ss.log
```

(Fallback: `Download/debug`, deretter app-privat lagring.) Eldre økter
roteres (siste 10 beholdes). Full kategoriliste:
[`debugging.md`](debugging.md#3b-diagnostic-session-log-on-device-file).

Lengre kontrollister og lastebil-/jurisdiksjonsdetaljer ligger i dokumentene
under [Dokumenter](#dokumenter).

# Pauseteller vs tur-ETA

Disse to tallene i bunnstripen svarer på **ulike spørsmål**. De er ikke ment å
være like hele tiden.

| Linje | Hva den betyr |
|---|---|
| **Break in XXX min** (eller km/mi) | «Når er *neste planlagte pause forfalt*?» — basert på pause**intervallet** ditt (for eksempel hver 2. time) minus hvor lenge du allerede har kjørt siden forrige pause. |
| **ETA XXX min** | «Når forventer vi å *være framme*?» — basert på gjenværende rute. |

Hvis turen bare er **45 minutter**, men pauser er satt til hver **2. time**,
kan du se noe som **Break in 120 min** ved siden av **ETA 45 min**. Det er
forventet: pausepåminnelsen følger intervallet du har satt, ikke slutten av
turen. På en kort tur kan du være framme før pausen er «forfalt».

Andre vanlige grunner til at de skiller lag:

1. **Intervallet er lengre enn turen** — sett kortere «timer mellom pauser»
   (eller godta at midtveis-pause ikke trengs).
2. **Du er et stykke ute på ruten** — pausetiden teller ned fra intervallet;
   ETA teller ned gjenværende vei.
3. **Pause vist som avstand** — minutter omregnes til km/mi med en fast antatt
   cruisehastighet (~80 km/t bare for visning). Det er ikke din live GPS-fart,
   så avstandslinjen er bare et grovt anslag.
4. **Lastebil-juridiske klokker** — i lastebilmodus kan intervallet følge
   kjøretidsregler (for eksempel pause etter 4,5 t kjøring), som fortsatt ikke
   er det samme som «tid til målet».
5. **Før du begynner å bevege deg** — begge linjene bruker plananslag
   (skiltet fart eller fast gange-/sykkelfart). De oppdateres fra faktisk
   fremdrift når GPS eller simulering er i gang.

**Tips:** velg et pauseintervall som passer inn i en typisk kjøredag for turen
din. Det finnes foreløpig ingen knapp for «del denne turen i N like etapper» —
bruk intervallet (og foreslåtte stopp) i stedet.

# Ruting og sikkerhet

Navi hjelper deg å planlegge; den erstatter ikke skjønn, lokal lov eller
forholdene langs stien.

- Fottur og partier utenfor sti kan kreve forsiktighet — bruk øynene og lokal
  kunnskap.
- Standardavstander for villcamping følger en Norge-orientert
  allemannsrettstanke og **gjelder ikke nødvendigvis i andre land**.
- Lastebilhvilepakker gjelder bare der appen kjenner igjen jurisdiksjonen;
  ellers later den ikke som den er et juridisk fartsskriver.
- Behandle kartdata (OpenStreetMap) som muligens ufullstendige eller utdaterte
  til du selv oppdaterer dem.

# Minimum maskinvare og lagring

**Minimum påkrevd maskinvare** for tiltenkt Automotive- / innebygd klasse:

| Del | Minimum / praktisk råd |
|---|---|
| **CPU** | **8 kjerner**, ca. **2 GHz**-klasse |
| **RAM** | **4 GB**. Foretrekk **regionale** uttrekk på den klassen; hele store land i ett jafs er ofte for tungt. |
| **Lagring** | La det være plass til regionsfil, stedsindeks, frakoblet grunnkart og valgfri DEM — ofte flere GB for en region. |
| **GPU** | MapLibre GLES er standardstien brukt på det testede nettbrettet. |

Tiltak i designet: regionale nedlastinger som standard, bufrede grafer,
bygging i bakgrunnen, og arbeidspooler som lar UI få plass. Mer:
[`architecture.md`](architecture.md).

# Skjermbilder

Hovedeksempler (Samsung Galaxy Tab S6 Lite **SM-P613** og rutesimulering):

Landskap med valgfri **3D**-skygge (frakoblet Protomaps + lokalt terreng):

![SM-P613 frakoblet Protomaps + Mapterhorn DEM-hillshade (landskap)](images/Screenshot_20260731_123844.jpg)

Fotturkorridor Skolla → Rondvassbu (**SIMULATING**):

![Skolla til Rondvassbu](images/terrain/hike_eldabu_ramshogda_3d.png)

GPS-følge under simulering:

![Følge under simulering](images/follow_gps/01_simulating_follow.png)

### Ekte maskinvare (SM-P613)

Portrett, frakoblet Østlandet-Protomaps, 3D av:

![SM-P613 frakoblet Protomaps 2D (portrett)](images/Screenshot_20260731_123746.jpg)

Testing på bilskjerm er fortsatt åpen —
[`real-hardware-testing.md`](real-hardware-testing.md).

### Flere bilder

Idle HUD:

![Idle begge linjer](images/hud/hud_idle_both_bars.png)

Karttilt 45° (3D av / på):

![45° tilt, 3D av](images/tilt45_3d_off.png)

![45° tilt, 3D på](images/tilt45_3d_on.png)

Følge / pan / Recenter / rotasjon:

![Følge under simulering](images/follow_gps/01_simulating_follow.png)

![Etter pan](images/follow_gps/02_after_pan.png)

![Etter Recenter](images/follow_gps/05_after_recenter.png)

![Rotasjonsmodi](images/follow_gps/06_rotation_modes_ok.png)

Fullt galleri: [`bilder.md`](bilder.md) (engelsk:
[`pictures.md`](pictures.md)).

# Dokumenter

| Dokument | Hva det er til |
|---|---|
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | Hvordan bidra (engelsk): fork fra **`dev`**, GitHub-grunnlag (PR mot `dev`), testing, docs, plugins, CI |
| [`architecture.md`](architecture.md) | Hvordan delene henger sammen |
| [`codebase-map.md`](codebase-map.md) | Hvor man endrer kode for en gitt funksjon |
| [`bilder.md`](bilder.md) / [`pictures.md`](pictures.md) | Skjermbildegallerier |
| [`icons.md`](icons.md) | Hvor ikonfilene ligger, lisens og hvordan man legger til SVG |
| [`map-styles.md`](map-styles.md) | Online vs frakoblet kartutseende; 3D |
| [`poi.md`](poi.md) | Stedstyper og søk |
| [How to use Navi](how-to-use.md) | Brukerveiledning (planlegging, Tools, pauser, lagrede steder/ruter, profiler) — engelsk |
| [`cider-route.md`](cider-route.md) | Forslag til **Siderveien** (norsk cider-rute): OSM `brewery=cider`-stopp sør→nord, med Navi-region/etappe-notater — engelsk |
| [`current-street.md`](current-street.md) | Bunn-HUD: veinavn, GPS-fart / skiltet grense, overskridelsesfarge |
| [`road-signs.md`](road-signs.md) | Norske fareskilt, barnesone-nærhet, Look forward (300 m kjegle), tilnærmingsfaser |
| [`kartmerking-lagrede-steder.md`](kartmerking-lagrede-steder.md) | Trykk-og-hold (4 s) og lagrede steder (engelsk: [`map-marking-saved-places.md`](map-marking-saved-places.md)) |
| [`historisk-bakgrunn.md`](historisk-bakgrunn.md) | Rast/vei-grunnlag for pauseintervaller (fottur/sykling) |
| [`ec-561-truck-rest.md`](ec-561-truck-rest.md) | EU lastebil kjøre-/hviletid |
| [`fmcsa-truck-rest.md`](fmcsa-truck-rest.md) | US lastebil kjøre-/hviletid |
| [`jurisdiction-rules.md`](jurisdiction-rules.md) | Land-/regionregelpakker |
| [`osm-updates.md`](osm-updates.md) | Valgfrie kartoppdateringer |
| [`android-build.md`](android-build.md) | Bygge Android-appen |
| [`build-linux.md`](build-linux.md) | Linux- / skrivebordsbygg (verktøy, gpsd, adb) |
| [`build-macos.md`](build-macos.md) | macOS-bygg (verktøy, Android NDK, adb) |
| [`build-windows.md`](build-windows.md) | Windows-bygg (MSVC, verktøy, Android NDK, adb) |
| [`debugging.md`](debugging.md) | Feilsøking |
| [`real-hardware-testing.md`](real-hardware-testing.md) | Sjekkliste for fysisk enhet |
| [`status.md`](status.md) | Hvilke dokumenter er live status vs historikk |
| [`future-proofing-audit-2026-07.md`](future-proofing-audit-2026-07.md) | Fremtidssikring / åpne risikoer |
| [`indexed-map-format-plan.md`](indexed-map-format-plan.md) | Indekserte rutingkart (fased evaluering) |
| [`plugins.md`](plugins.md) | Plugin-vert og veikart (av/på; USB/Bluetooth) |

Se `docs/`-mappen for mer spesialiserte emner (stemme, APRS, ECU, formler osv.).

# Plugins

En sandkasse for plugins finnes, så fremtidige tillegg kan kjøre trygt.
**Ingen produktplugins leveres i appen ennå** — det er med vilje. Oversikt:
[`plugins.md`](plugins.md). Systemkrav: hver plugin skal kunne **slås av/på**,
og maskinvareplugins skal kunne snakke over **USB** / **Bluetooth** via verten.

| Spesifikasjon | Emne |
|---|---|
| [`plugins/i18n-translation-spec.md`](plugins/i18n-translation-spec.md) | Fremtidige UI-språk (bare engelsk i dag). Oversettertabell: [`translations.csv`](plugins/translations.csv); sammenheng: [`translations-context.md`](plugins/translations-context.md) |
| [`plugins/right-to-roam-camping-spec.md`](plugins/right-to-roam-camping-spec.md) | Villcamping-forslag (plugin, ikke kjerne) |
| [`plugins/safety-resupply.md`](plugins/safety-resupply.md) | Drivstoff-/vannforsyning |
| [`plugins/weather-plugin.md`](plugins/weather-plugin.md) | Vær-overlay (Meteocons-ikoner levert; gjest ikke levert) |
| [`plugins/weather-icons-reference.md`](plugins/weather-icons-reference.md) | Hva hvert vær-ikon betyr (fill-stil) |
| [`plugins/instrument-cluster-agl-spec.md`](plugins/instrument-cluster-agl-spec.md) | Eksportere nav-tilstand og tilnærmingsvarsler til instrumentcluster |
| [`plugins/animated-icons-spec.md`](plugins/animated-icons-spec.md) | Animerte ikoner |
| [`plugins/custom-alert-sounds-spec.md`](plugins/custom-alert-sounds-spec.md) | Korte varselyder (skilt, kamera, overskridelse-earcon) |
| [`plugins/horse-trekking-spec.md`](plugins/horse-trekking-spec.md) | Ridning: forsyning og adgangsveiledning (Hiking er midlertidig stopgap) |
| [`plugins/adaptive-speed-warning-spec.md`](plugins/adaptive-speed-warning-spec.md) | Talt, eskalerende fartsvarsel (prosenttrinn; ikke levert) |

## Ikoner (hvor de ligger)

Kart-, sving-, POI- og statusikoner er filer i repoet — de lages ikke ved
kjøring. Forfatterguide, oppslagsrekkefølge og lisenser: [`icons.md`](icons.md).

| Sti | Hva som ligger der |
|---|---|
| [`core/src/icons/`](../core/src/icons/) | **Fullt sett** (kilde for skrivebord / kjerne). Mest Navit (**GPL v2**). Egne Navi-filer her inkluderer `leaf.svg` (øko) og `speed_camera.svg`. |
| [`app/src/main/assets/icons/`](../app/src/main/assets/icons/) | Android **lean-pakke** — en nedskalert kopi som følger med hver APK. Nøkler som mangler her faller tilbake til `unknown.svg` på enheten. |
| [`core/src/icons/road-signs/`](../core/src/icons/road-signs/) | Norske trafikkskilt-SVG-er (**NLOD 2.0**, ikke Navit). Android-kopi: [`app/src/main/assets/icons/road-signs/`](../app/src/main/assets/icons/road-signs/). |
| [`core/src/icons/aprs/`](../core/src/icons/aprs/) | APRS-symboler for bevegelige ikoner. Android-kopi: [`app/src/main/assets/icons/aprs/`](../app/src/main/assets/icons/aprs/). |
| [`app/src/main/res/mipmap-*`](../app/src/main/res/) | Android-**launcher** (startskjerm / app-skuff). Egen Navi-merkevare, ikke fra Navit. |
| [`docs/icons/open-app.svg`](icons/open-app.svg) | Splash- / åpne-app-merke (Inkscape-kilde). Android-drawables: `app/src/main/res/drawable/ic_splash*.xml`. |

For å legge til eller overstyre et kart-/POI-ikon, legg en SVG i `core/src/icons/`
(og kopier den inn i Android lean-pakken hvis den skal vises på enheten). Se
[`icons.md`](icons.md).

# Kodestandarder og bidrag

Les **[`CONTRIBUTING.md`](CONTRIBUTING.md)** (engelsk) først. Den forklarer
hvordan du **forker** [github.com/Supermagnum/Navi](https://github.com/Supermagnum/Navi),
sjekker ut **`dev`**, holder forken oppdatert, og åpner en pull request med
**base-gren `dev`**. Klone-stegene under [Bygge og installere](#bygge-og-installere)
er for et lokalt bygg; de erstatter ikke en fork hvis du vil sende inn en
endring.

Kortversjon av CI-forventninger:

| Område | Forventning |
|---|---|
| Rust | `cargo fmt`, Clippy uten advarsler, tester |
| Kotlin | ktlint, detekt, enhetstester |
| Android | `./gradlew :app:assembleDebug` |

# Bygge og installere

Hent koden fra GitHub. Utvikling skjer på **`dev`** (nyeste funksjoner);
**`main`** er standardgren ved `git clone` (en vanlig `git clone` sjekker ut
`main`). Installer Git ved behov
(`sudo apt install git` på Debian/Ubuntu — andre systemer i
[`build-linux.md`](build-linux.md#getting-the-code)).

**For å bygge eller installere lokalt** (uten pull request):

```bash
git clone https://github.com/Supermagnum/Navi.git
cd Navi
git checkout dev
```

**For å bidra med en endring:** ikke bare klone denne URL-en. Fork repoet,
klone **din** fork, og jobb på **`dev`** — steg for steg i
[`CONTRIBUTING.md`](CONTRIBUTING.md#fork-from-dev-and-basic-github-usage)
(engelsk).

## Installer en forhåndsbygd APK

En debug-signert APK ligger i
[`compiled/navi-debug.apk`](../compiled/navi-debug.apk) (arm64, samme pakke som
`./gradlew :app:assembleDebug`). Du trenger ikke Rust/NDK-verktøykjede for å
installere den.

1. På enheten: slå på **Utvikleralternativer** og **USB-feilsøking**.
2. Koble til med `adb devices` og bekreft at enheten er listet.
3. Hvis en eldre Navi-bygg med **annen signatur** allerede er installert,
   avinstaller først (`adb uninstall no.navi.app`).
4. Installer og start:

```bash
adb install -r compiled/navi-debug.apk
adb shell am start -n no.navi.app/.MainActivity
```

Denne APK-en er signert med Androids **debug**-nøkkel. Den er for testere, ikke
en Play Store- / F-Droid-utgivelse. For å bygge fra kilde, følg avsnittene
nedenfor.

Du kan også laste ned
[`compiled/navi-debug.apk`](https://github.com/Supermagnum/Navi/blob/main/compiled/navi-debug.apk)
direkte i nettleseren på Android-enheten og installere den derfra (tillat
installasjon fra nettleseren hvis du blir bedt om det). Bruk nedlastingsknappen
på den siden, eller råfilen
[`raw/main/compiled/navi-debug.apk`](https://github.com/Supermagnum/Navi/raw/main/compiled/navi-debug.apk).

## Android-appen (alle vertsplattformer)

APK-en bygges likt på **Linux**, **macOS** og **Windows**: kompiler `libnavi.so`
med NDK, deretter Gradle. Vertsspesifikk oppsett (SDK-stier, NDK-clang, `adb`)
står i OS-guidene; felles oppskrift er
[`android-build.md`](android-build.md).

| Vert | Installer verktøy, NDK, `adb` | Deretter |
|---|---|---|
| Linux | [`build-linux.md`](build-linux.md) | [`android-build.md`](android-build.md) |
| macOS | [`build-macos.md`](build-macos.md) | samme |
| Windows | [`build-windows.md`](build-windows.md) (**Git Bash** for `scripts/*.sh`; `.\gradlew.bat` fra PowerShell) | samme |

**Én gang per maskin:** Rust Android-mål, JDK 17, Android SDK (API 36), NDK,
og `ANDROID_HOME` / `ANDROID_NDK_HOME`. Pek `.cargo/config.toml` mot NDK-ens
verts-prebuilt (`linux-x86_64`, `darwin-arm64` / `darwin-x86_64`, eller
`windows-x86_64`).

### Emulator (x86_64-bilde)

```bash
# Fra repo-roten (bash: Linux/macOS, eller Git Bash på Windows)
export ANDROID_HOME=…                 # se OS-guiden for typisk sti
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/<version>"
rustup target add x86_64-linux-android   # én gang
./scripts/build-android-native.sh x86_64-linux-android release
./gradlew :app:assembleDebug          # Windows PowerShell: .\gradlew.bat …
./gradlew :app:installDebug
./scripts/launch-navi-emulator.sh
```

### Nettbrett / telefon (arm64)

```bash
export ANDROID_HOME=…
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/<version>"
rustup target add aarch64-linux-android   # én gang
./scripts/build-android-native.sh aarch64-linux-android release
./gradlew :app:assembleDebug
./gradlew :app:installDebug
adb shell am start -n no.navi.app/.MainActivity
```

Bekreft at APK-en inneholder arm64-biblioteket:

```bash
unzip -l app/build/outputs/apk/debug/app-debug.apk | grep 'lib/arm64-v8a/libnavi.so'
```

### Utgivelsesbygg (APK / AAB)

Debug bruker Android **debug**-nøkkel. Et **release**-pakke er det du sidelaster
som utgivelse, kjører F-Droid-lignende sjekker på, eller røyktester som AAB.

1. **Native bibliotek** for hver ABI du leverer (butikk-AAB trenger vanligvis begge):

```bash
export ANDROID_HOME=…
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/<version>"
./scripts/build-android-native.sh aarch64-linux-android release
./scripts/build-android-native.sh x86_64-linux-android release
```

2. **Lokal opplastingsnøkkel** (valgfritt, anbefalt for installerbar release).
   Lager gitignorert `app/keystore/navi-upload.jks` — **kun lokal testing**, ikke
   Play-produksjonssignering:

```bash
./scripts/make-upload-keystore.sh
```

3. **Bygg**:

```bash
./gradlew :app:assembleRelease
# → app/build/outputs/apk/release/app-release.apk

./gradlew :app:bundleRelease
# → app/build/outputs/bundle/release/app-release.aab
```

4. **Installer release-APK** (annen signatur enn debug — avinstaller debug først
   hvis `adb` nekter oppgradering):

```bash
adb uninstall no.navi.app
adb install -r app/build/outputs/apk/release/app-release.apk
adb shell am start -n no.navi.app/.MainActivity
```

Mer detalj (bundletool, F-Droid Podman): engelsk
[README — Release build](../README.md#release-build-apk--aab) og
[`android-build.md`](android-build.md).

## Skrivebord / kjerne (valgfritt)

| Vert | Guide |
|---|---|
| Linux (`navi-desktop`, gpsd, kjernetester) | [`build-linux.md`](build-linux.md) |
| macOS | [`build-macos.md`](build-macos.md) |
| Windows | [`build-windows.md`](build-windows.md) |

### Arbeidsområdets struktur

- `core/` — ruting, steder, hvileregler, ikoner (Rust). Ikon-SVG-er: `core/src/icons/`
- `navi-ffi/` — bro til Android og andre verter
- `app/` — Android-UI (Kotlin). Ikonpakke på enheten: `app/src/main/assets/icons/`
- `plugin-host/` / `plugin-sdk/` / `plugins/` — fremtidige plugins
- [`architecture.md`](architecture.md) — hvordan det henger sammen

### Vertstester (eksempler)

```bash
cargo test -p driver-break-core --test planner_options_routes
cargo test -p navi-plugin-host --test isolation -- --nocapture
```

Store kartfil-integrasjonstester er vanligvis merket `#[ignore]` og trenger
fiksturer under `core/target/integration-fixtures`. Se
[`test-results.md`](test-results.md).

# Hvor kartdataene kommer fra

Navi er **offline-first**. Nett brukes bare når du selv velger å laste ned
eller oppdatere.

| Data | Kilde | Brukes til |
|---|---|---|
| Veier og steder | OpenStreetMap via Geofabrik `.osm.pbf` | Ruting og søk |
| Kartoppdateringer | Geofabrik-diff / ferskt uttrekk | Bare valgfri oppdatering |
| Høyde | Offentlige DEM-fliser | Øko / bakker |
| Kartbilde | OpenFreeMap Liberty (online) eller Protomaps PMTiles (frakoblet) | Det du ser på skjermen |
| Posisjon | Enhetens GPS (eller gpsd på Linux) | Hvor du er |
| Ikoner | Medfølgende Navit-avledet SVG | Markører og svinger |

Land-/regionvisuelle uttrekk kan også lages med
[PMT-splitter](https://github.com/Supermagnum/PMT-splitter/tree/main).

# Kjente problemer

- **Plugins:** innholdstillegg er bevisst ikke levert ennå, fordi de ikke er
  laget ennå.
- **UI-finish:** skjermene fungerer, men trenger fortsatt visuell opprydding på
  bilskjermer.
- **Bevegelige ikoner:** tegnes med Compose-overlegg i dag; native
  kartsymbol-lag er ikke hovedstien ennå.
- **Kart / GPU-særheter:** noen emulator- og telefon-GPU-oppsett har historisk
  krasjet eller vasket ut bakkeskygge; prosjektet gikk over til MapLibre GLES
  etter nettbrettsjekker. Detaljer i [`map-styles.md`](map-styles.md)
  og [`debugging.md`](debugging.md).
- **Bare-i-skjermbilde innsjøkant:** en myk blå kant rundt vann kan dukke opp i
  skjermbilder, men ikke under vanlig bruk — se
  [`map-styles.md`](map-styles.md).
- **Treg fotturplan på store områder (løst):** overnattingsbygninger bruker et
  1,5 km korridorfilter og én PBF-skanning for POI + bygninger (nøyaktig 150 m
  allemannsretten-sjekk uendret). Målt på DNT Åkersætra→Rondvassbu
  (~**139,9 km** korridor; `overnight_scan_bench`, debug): bbox-alle
  **102 556 bygninger / ~180,7 s** lasting → korridor **487 bygninger / ~83,1 s**
  lasting (tidligere ~177,6 s for en hel plan når bbox-alle matet
  overnattingssjekker). Gjenværende kostnad er mest obligatorisk
  full-extract-dekoding pluss andre PBF-skanninger under planlegging.
  Under en forgrunnsplan viker bakgrunnskonvertering/stedsindeks, og
  GPS-kjegle-/veinær-bbox-bygging hopper over, slik at samtidige PBF-lesere
  ikke strekker pack-miss-planer til mange minutter (se
  [Indeksering](#indeksering-bakgrunn-etter-nedlasting)).
- **Pauseteller ≠ tur-ETA:** med vilje — se
  [Pauseteller vs tur-ETA](#pauseteller-vs-tur-eta).
- **Ikke implementert ennå:** sjekk av om koden kan optimaliseres for
  tegning/rendering.

# TODO

(Bare fremtidig arbeid — leverte funksjoner står i tabellen Funksjoner over.)

**UI-språkpakker** — menyene er bare engelsk i dag. En fremtidig `i18n` /
`ui_translation`-plugin skal gi valgbare språk med **tilbakefall til engelsk**
når en oversettelse eller pakke mangler. Spesifikasjon:
[`plugins/i18n-translation-spec.md`](plugins/i18n-translation-spec.md).
Ikke utled UI-språk fra GPS eller SIM-land. Ikke innfør en språkbryter før den
pluginen finnes.

Visnings**enheter** (metrisk / US / UK) er levert (kjøreinnstillinger). Norske
fartsgrenseskilt forblir offisielle km/t-plater; mph-*platekunst* er fortsatt
fremtid (`new-signs/`). Frakoblede Protomaps-etiketter for alkoholutsalg
(`pois.kind=alcohol`, z16+) er levert — se [`map-styles.md`](map-styles.md).

**Historiske norske/norrøne avstandsenheter** som valgbar visningsmodus
(f.eks. rast, dagsvei, fjerdingvei), ved siden av de eksisterende
Metric / US / UK-profilene. Bare visning — historisk/kulturell modus, ikke
standard og ikke et seriøst navigasjonsalternativ til metrisk/imperial.
**Fart:** norrøn **mil per time** kan erstatte km/t; mindre avstander
(steinens kast, pileskudd, fjerdingvei, …). Trenger en designrunde før
implementering (hvilke avstands-/fartsfelt, samspill med `DisplayUnits`,
periode-spesifikk eller ett kanonisk omregningssett).
Kontekst og omregningstall: [README — TODO](../README.md#todo).

Engelsk: [README — TODO](../README.md#todo).
