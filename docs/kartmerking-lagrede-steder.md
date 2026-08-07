# Slik bruker du: kartmerking og lagrede steder

Merk et punkt på kartet med **trykk-og-hold i ca. 4 sekunder**, og sett det som
**Fra**, **Via** eller **Til**, eller **lagre** det som et navngitt sted.

Dette er noe annet enn **Lagrede ruter**, som lagrer en hel planlagt korridor.
Et **lagret sted** er bare ett koordinat med navn.

Engelsk versjon (kanonisk): [`map-marking-saved-places.md`](map-marking-saved-places.md).
Skjermbilder (SM-P613): [`images/map-long-press/`](images/map-long-press/).

---

## Merk et sted på kartet

1. Åpne kartet. Lukk planleggingspanelet med **Close** om du trenger fri flate.
2. Hold én finger stille på kartet i **omtrent 4 sekunder**.
3. En **blå ring** fylles rundt fingeren mens holdet gjenkjennes.
4. Når holdet er ferdig, kommer arket **Marked location** med foreslått navn
   (nærmeste adresse/sted innen ~12 m når det finnes, ellers koordinater) og:
   - **Set as From / Start**
   - **Set as Via** (kan gjentas for flere via-punkter)
   - **Set as To / Destination**
   - **Save this place**
   - **Cancel** (eller trykk utenfor arket)

### Hva avbryter et langt trykk

| Handling | Resultat |
|---|---|
| Hold ~4 s uten å gli | Meny åpnes |
| Slipp før ~4 s | Ingen meny |
| Pan / gli utover liten toleranse | Hold avbrutt; kartet panorerer |
| Knip / andre finger | Hold avbrutt |

---

## Lagre dette stedet

1. Fra merke-arket: **Save this place**.
2. Bekreft eller rediger **navn**, deretter **Save**.
3. Stedet lagres i app-databasen (`navi.db`, tabell `saved_places`).

---

## Bruke lagrede steder

1. Åpne **Route**-planlegging.
2. Bla til **Saved places** (under **Saved routes**).
3. Trykk **From**, **Via** eller **To** på en rad — samme feltfylling som søk /
   **Use GPS**.
4. **Rename** / **Delete** etter behov, deretter **Plan route**.

Et lagret sted er ikke knyttet til én rolle; samme punkt kan være Fra på én tur
og Til på en annen.

---

## Lagrede steder vs lagrede ruter

| | Lagrede steder | Lagrede ruter |
|---|---|---|
| Innhold | Ett lat/lon + navn | Hel korridor (start, mål, via, profil) |
| UI | **Saved places** | **Saved routes** |
