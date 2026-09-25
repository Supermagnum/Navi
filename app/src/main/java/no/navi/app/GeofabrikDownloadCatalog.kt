package no.navi.app

/**
 * Tools → Download scope country picker catalog.
 *
 * Taxonomy uses the standard seven continents. Country chips are sourced from
 * Geofabrik's published index (`https://download.geofabrik.de/index-v1.json`,
 * verified 2026-08-13): each [GeofabrikCountry.path] is a real
 * `{path}-latest.osm.pbf` leaf on that server.
 *
 * Geofabrik's own folders (`africa/`, `asia/`, `australia-oceania/`,
 * `central-america/`, `europe/`, `north-america/`, `south-america/`, plus
 * root-level `antarctica` / `russia`) are kept as download paths. UI continents
 * map Central America extracts under **North America** (seven-continent model);
 * Russia stays under **Europe** (conventional general-audience placement).
 *
 * Offline bboxes for these paths live in core `GEOFABRIK_PATH_BBOX` (computed
 * from the same index geometries). A chip implies Geofabrik PBF + PMTiles/DEM
 * queueing can resolve a bbox. [supportNote] states partial coverage honestly
 * (most entries are maps-only).
 */
enum class GeofabrikContinent(
    val label: String,
    val testTag: String,
) {
    Asia("Asia", "chip_continent_asia"),
    Africa("Africa", "chip_continent_africa"),
    NorthAmerica("North America", "chip_continent_north_america"),
    SouthAmerica("South America", "chip_continent_south_america"),
    Antarctica("Antarctica", "chip_continent_antarctica"),
    Europe("Europe", "chip_continent_europe"),
    AustraliaOceania("Australia (Oceania)", "chip_continent_australia_oceania"),
}

data class GeofabrikCountry(
    val label: String,
    /** Geofabrik download path (e.g. `europe/norway`, `africa/kenya`). */
    val path: String,
    val continent: GeofabrikContinent,
    val iso: String,
    /**
     * Shown when this country is selected: maps + which jurisdiction features
     * actually apply (partial coverage made explicit).
     */
    val supportNote: String,
    val testTag: String,
)

object GeofabrikDownloadCatalog {
    /** Fixed UI order matching the standard seven-continent model. */
    val continents: List<GeofabrikContinent> = GeofabrikContinent.entries

    val countries: List<GeofabrikCountry> =
        listOf(
            GeofabrikCountry(
                label = "China",
                path = "asia/china",
                continent = GeofabrikContinent.Asia,
                iso = "cn",
                supportNote =
                    "Offline maps (provinces per navi-server current.json). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_asia_china",
            ),
            GeofabrikCountry(
                label = "India",
                path = "asia/india",
                continent = GeofabrikContinent.Asia,
                iso = "in",
                supportNote =
                    "Offline maps (zones per navi-server current.json). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_asia_india",
            ),
            GeofabrikCountry(
                label = "Indonesia",
                path = "asia/indonesia",
                continent = GeofabrikContinent.Asia,
                iso = "id",
                supportNote =
                    "Offline maps (islands / regions per navi-server current.json). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_asia_indonesia",
            ),
            GeofabrikCountry(
                label = "Iran",
                path = "asia/iran",
                continent = GeofabrikContinent.Asia,
                iso = "ir",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_asia_iran",
            ),
            GeofabrikCountry(
                label = "Japan",
                path = "asia/japan",
                continent = GeofabrikContinent.Asia,
                iso = "jp",
                supportNote =
                    "Offline maps (regions per navi-server current.json). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_asia_japan",
            ),
            GeofabrikCountry(
                label = "Kazakhstan",
                path = "asia/kazakhstan",
                continent = GeofabrikContinent.Asia,
                iso = "kz",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_asia_kazakhstan",
            ),
            GeofabrikCountry(
                label = "Malaysia, Singapore, Brunei",
                path = "asia/malaysia-singapore-brunei",
                continent = GeofabrikContinent.Asia,
                iso = "my",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_asia_malaysia_singapore_brunei",
            ),
            GeofabrikCountry(
                label = "Nepal",
                path = "asia/nepal",
                continent = GeofabrikContinent.Asia,
                iso = "np",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_asia_nepal",
            ),
            GeofabrikCountry(
                label = "Pakistan",
                path = "asia/pakistan",
                continent = GeofabrikContinent.Asia,
                iso = "pk",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_asia_pakistan",
            ),
            GeofabrikCountry(
                label = "Philippines",
                path = "asia/philippines",
                continent = GeofabrikContinent.Asia,
                iso = "ph",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_asia_philippines",
            ),
            GeofabrikCountry(
                label = "South Korea",
                path = "asia/south-korea",
                continent = GeofabrikContinent.Asia,
                iso = "kr",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_asia_south_korea",
            ),
            GeofabrikCountry(
                label = "Thailand",
                path = "asia/thailand",
                continent = GeofabrikContinent.Asia,
                iso = "th",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_asia_thailand",
            ),
            GeofabrikCountry(
                label = "Uzbekistan",
                path = "asia/uzbekistan",
                continent = GeofabrikContinent.Asia,
                iso = "uz",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_asia_uzbekistan",
            ),
            GeofabrikCountry(
                label = "Vietnam",
                path = "asia/vietnam",
                continent = GeofabrikContinent.Asia,
                iso = "vn",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_asia_vietnam",
            ),
            GeofabrikCountry(
                label = "Algeria",
                path = "africa/algeria",
                continent = GeofabrikContinent.Africa,
                iso = "dz",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_africa_algeria",
            ),
            GeofabrikCountry(
                label = "Egypt",
                path = "africa/egypt",
                continent = GeofabrikContinent.Africa,
                iso = "eg",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_africa_egypt",
            ),
            GeofabrikCountry(
                label = "Ethiopia",
                path = "africa/ethiopia",
                continent = GeofabrikContinent.Africa,
                iso = "et",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_africa_ethiopia",
            ),
            GeofabrikCountry(
                label = "Ghana",
                path = "africa/ghana",
                continent = GeofabrikContinent.Africa,
                iso = "gh",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_africa_ghana",
            ),
            GeofabrikCountry(
                label = "Kenya",
                path = "africa/kenya",
                continent = GeofabrikContinent.Africa,
                iso = "ke",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_africa_kenya",
            ),
            GeofabrikCountry(
                label = "Madagascar",
                path = "africa/madagascar",
                continent = GeofabrikContinent.Africa,
                iso = "mg",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_africa_madagascar",
            ),
            GeofabrikCountry(
                label = "Morocco",
                path = "africa/morocco",
                continent = GeofabrikContinent.Africa,
                iso = "ma",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_africa_morocco",
            ),
            GeofabrikCountry(
                label = "Nigeria",
                path = "africa/nigeria",
                continent = GeofabrikContinent.Africa,
                iso = "ng",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_africa_nigeria",
            ),
            GeofabrikCountry(
                label = "Senegal and Gambia",
                path = "africa/senegal-and-gambia",
                continent = GeofabrikContinent.Africa,
                iso = "sn",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_africa_senegal_and_gambia",
            ),
            GeofabrikCountry(
                label = "South Africa",
                path = "africa/south-africa",
                continent = GeofabrikContinent.Africa,
                iso = "za",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_africa_south_africa",
            ),
            GeofabrikCountry(
                label = "Tanzania",
                path = "africa/tanzania",
                continent = GeofabrikContinent.Africa,
                iso = "tz",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_africa_tanzania",
            ),
            GeofabrikCountry(
                label = "Tunisia",
                path = "africa/tunisia",
                continent = GeofabrikContinent.Africa,
                iso = "tn",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_africa_tunisia",
            ),
            GeofabrikCountry(
                label = "Uganda",
                path = "africa/uganda",
                continent = GeofabrikContinent.Africa,
                iso = "ug",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_africa_uganda",
            ),
            GeofabrikCountry(
                label = "Zimbabwe",
                path = "africa/zimbabwe",
                continent = GeofabrikContinent.Africa,
                iso = "zw",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_africa_zimbabwe",
            ),
            GeofabrikCountry(
                label = "Belize",
                path = "central-america/belize",
                continent = GeofabrikContinent.NorthAmerica,
                iso = "bz",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_central_america_belize",
            ),
            GeofabrikCountry(
                label = "Canada",
                path = "north-america/canada",
                continent = GeofabrikContinent.NorthAmerica,
                iso = "ca",
                supportNote =
                    "Offline maps (provinces / territories per navi-server current.json). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_north_america_canada",
            ),
            GeofabrikCountry(
                label = "Costa Rica",
                path = "central-america/costa-rica",
                continent = GeofabrikContinent.NorthAmerica,
                iso = "cr",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_central_america_costa_rica",
            ),
            GeofabrikCountry(
                label = "Cuba",
                path = "central-america/cuba",
                continent = GeofabrikContinent.NorthAmerica,
                iso = "cu",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_central_america_cuba",
            ),
            GeofabrikCountry(
                label = "Greenland",
                path = "north-america/greenland",
                continent = GeofabrikContinent.NorthAmerica,
                iso = "gl",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_north_america_greenland",
            ),
            GeofabrikCountry(
                label = "Guatemala",
                path = "central-america/guatemala",
                continent = GeofabrikContinent.NorthAmerica,
                iso = "gt",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_central_america_guatemala",
            ),
            GeofabrikCountry(
                label = "Honduras",
                path = "central-america/honduras",
                continent = GeofabrikContinent.NorthAmerica,
                iso = "hn",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_central_america_honduras",
            ),
            GeofabrikCountry(
                label = "Jamaica",
                path = "central-america/jamaica",
                continent = GeofabrikContinent.NorthAmerica,
                iso = "jm",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_central_america_jamaica",
            ),
            GeofabrikCountry(
                label = "Mexico",
                path = "north-america/mexico",
                continent = GeofabrikContinent.NorthAmerica,
                iso = "mx",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_north_america_mexico",
            ),
            GeofabrikCountry(
                label = "Nicaragua",
                path = "central-america/nicaragua",
                continent = GeofabrikContinent.NorthAmerica,
                iso = "ni",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_central_america_nicaragua",
            ),
            GeofabrikCountry(
                label = "Panama",
                path = "central-america/panama",
                continent = GeofabrikContinent.NorthAmerica,
                iso = "pa",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_central_america_panama",
            ),
            GeofabrikCountry(
                label = "United States",
                path = "north-america/us",
                continent = GeofabrikContinent.NorthAmerica,
                iso = "us",
                supportNote =
                    "Offline maps (US states / California norcal+socal per navi-server current.json; large). Truck HOS: FMCSA from GPS. Speed cameras: decline.",
                testTag = "chip_country_north_america_us",
            ),
            GeofabrikCountry(
                label = "Argentina",
                path = "south-america/argentina",
                continent = GeofabrikContinent.SouthAmerica,
                iso = "ar",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_south_america_argentina",
            ),
            GeofabrikCountry(
                label = "Bolivia",
                path = "south-america/bolivia",
                continent = GeofabrikContinent.SouthAmerica,
                iso = "bo",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_south_america_bolivia",
            ),
            GeofabrikCountry(
                label = "Brazil",
                path = "south-america/brazil",
                continent = GeofabrikContinent.SouthAmerica,
                iso = "br",
                supportNote =
                    "Offline maps (macro-regions per navi-server current.json). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_south_america_brazil",
            ),
            GeofabrikCountry(
                label = "Chile",
                path = "south-america/chile",
                continent = GeofabrikContinent.SouthAmerica,
                iso = "cl",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_south_america_chile",
            ),
            GeofabrikCountry(
                label = "Colombia",
                path = "south-america/colombia",
                continent = GeofabrikContinent.SouthAmerica,
                iso = "co",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_south_america_colombia",
            ),
            GeofabrikCountry(
                label = "Ecuador",
                path = "south-america/ecuador",
                continent = GeofabrikContinent.SouthAmerica,
                iso = "ec",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_south_america_ecuador",
            ),
            GeofabrikCountry(
                label = "Guyana",
                path = "south-america/guyana",
                continent = GeofabrikContinent.SouthAmerica,
                iso = "gy",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_south_america_guyana",
            ),
            GeofabrikCountry(
                label = "Paraguay",
                path = "south-america/paraguay",
                continent = GeofabrikContinent.SouthAmerica,
                iso = "py",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_south_america_paraguay",
            ),
            GeofabrikCountry(
                label = "Peru",
                path = "south-america/peru",
                continent = GeofabrikContinent.SouthAmerica,
                iso = "pe",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_south_america_peru",
            ),
            GeofabrikCountry(
                label = "Suriname",
                path = "south-america/suriname",
                continent = GeofabrikContinent.SouthAmerica,
                iso = "sr",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_south_america_suriname",
            ),
            GeofabrikCountry(
                label = "Uruguay",
                path = "south-america/uruguay",
                continent = GeofabrikContinent.SouthAmerica,
                iso = "uy",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_south_america_uruguay",
            ),
            GeofabrikCountry(
                label = "Venezuela",
                path = "south-america/venezuela",
                continent = GeofabrikContinent.SouthAmerica,
                iso = "ve",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_south_america_venezuela",
            ),
            GeofabrikCountry(
                label = "Antarctica",
                path = "antarctica",
                continent = GeofabrikContinent.Antarctica,
                iso = "aq",
                supportNote =
                    "Offline maps (Geofabrik Antarctica extract; sparse road network). Truck HOS: decline. Speed cameras: decline.",
                testTag = "chip_country_antarctica",
            ),
            GeofabrikCountry(
                label = "Austria",
                path = "europe/austria",
                continent = GeofabrikContinent.Europe,
                iso = "at",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_austria",
            ),
            GeofabrikCountry(
                label = "Belgium",
                path = "europe/belgium",
                continent = GeofabrikContinent.Europe,
                iso = "be",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_belgium",
            ),
            GeofabrikCountry(
                label = "Bulgaria",
                path = "europe/bulgaria",
                continent = GeofabrikContinent.Europe,
                iso = "bg",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_bulgaria",
            ),
            GeofabrikCountry(
                label = "Croatia",
                path = "europe/croatia",
                continent = GeofabrikContinent.Europe,
                iso = "hr",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_croatia",
            ),
            GeofabrikCountry(
                label = "Czech Republic",
                path = "europe/czech-republic",
                continent = GeofabrikContinent.Europe,
                iso = "cz",
                supportNote =
                    "Offline maps (kraje per navi-server current.json). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_czech_republic",
            ),
            GeofabrikCountry(
                label = "Denmark",
                path = "europe/denmark",
                continent = GeofabrikContinent.Europe,
                iso = "dk",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_denmark",
            ),
            GeofabrikCountry(
                label = "Estonia",
                path = "europe/estonia",
                continent = GeofabrikContinent.Europe,
                iso = "ee",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_estonia",
            ),
            GeofabrikCountry(
                label = "Finland",
                path = "europe/finland",
                continent = GeofabrikContinent.Europe,
                iso = "fi",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_finland",
            ),
            GeofabrikCountry(
                label = "France",
                path = "europe/france",
                continent = GeofabrikContinent.Europe,
                iso = "fr",
                supportNote =
                    "Offline maps (regions per navi-server current.json). Truck HOS: EC 561 from GPS. Speed cameras: decline (product policy).",
                testTag = "chip_country_europe_france",
            ),
            GeofabrikCountry(
                label = "Germany",
                path = "europe/germany",
                continent = GeofabrikContinent.Europe,
                iso = "de",
                supportNote =
                    "Offline maps (Bundesländer / Regierungsbezirke matching navi-server " +
                        "current.json). Truck HOS: EC 561 from GPS. Speed cameras: decline (product policy).",
                testTag = "chip_country_europe_germany",
            ),
            GeofabrikCountry(
                label = "Great Britain",
                path = "europe/great-britain",
                continent = GeofabrikContinent.Europe,
                iso = "gb",
                supportNote =
                    "Offline maps (England+Scotland+Wales country extract, no NI). " +
                        "For nations/counties use United Kingdom. Truck HOS: decline. Speed cameras: opt-in.",
                testTag = "chip_country_europe_great_britain",
            ),
            GeofabrikCountry(
                label = "United Kingdom",
                path = "europe/united-kingdom",
                continent = GeofabrikContinent.Europe,
                iso = "gb",
                supportNote =
                    "Offline maps (UK extract + nation/county chips). Includes Northern Ireland. " +
                        "London borough extracts were retired — use Greater London. " +
                        "Truck HOS: decline. Speed cameras: opt-in.",
                testTag = "chip_country_europe_united_kingdom",
            ),
            GeofabrikCountry(
                label = "Greece",
                path = "europe/greece",
                continent = GeofabrikContinent.Europe,
                iso = "gr",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_greece",
            ),
            GeofabrikCountry(
                label = "Hungary",
                path = "europe/hungary",
                continent = GeofabrikContinent.Europe,
                iso = "hu",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_hungary",
            ),
            GeofabrikCountry(
                label = "Iceland",
                path = "europe/iceland",
                continent = GeofabrikContinent.Europe,
                iso = "is",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_iceland",
            ),
            GeofabrikCountry(
                label = "Ireland and Northern Ireland",
                path = "europe/ireland-and-northern-ireland",
                continent = GeofabrikContinent.Europe,
                iso = "ie",
                supportNote =
                    "Offline maps (Ireland + Northern Ireland extract). Truck HOS: IE points use EC 561 from GPS; GB/NI declines until a UK pack exists. Speed cameras: decline outside NO/UK allow-list.",
                testTag = "chip_country_europe_ireland_and_northern_ireland",
            ),
            GeofabrikCountry(
                label = "Italy",
                path = "europe/italy",
                continent = GeofabrikContinent.Europe,
                iso = "it",
                supportNote =
                    "Offline maps (macro-regions per navi-server current.json). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_italy",
            ),
            GeofabrikCountry(
                label = "Latvia",
                path = "europe/latvia",
                continent = GeofabrikContinent.Europe,
                iso = "lv",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_latvia",
            ),
            GeofabrikCountry(
                label = "Lithuania",
                path = "europe/lithuania",
                continent = GeofabrikContinent.Europe,
                iso = "lt",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_lithuania",
            ),
            GeofabrikCountry(
                label = "Luxembourg",
                path = "europe/luxembourg",
                continent = GeofabrikContinent.Europe,
                iso = "lu",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_luxembourg",
            ),
            GeofabrikCountry(
                label = "Netherlands",
                path = "europe/netherlands",
                continent = GeofabrikContinent.Europe,
                iso = "nl",
                supportNote =
                    "Offline maps (provinces per navi-server current.json). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_netherlands",
            ),
            GeofabrikCountry(
                label = "Norway",
                path = "europe/norway",
                continent = GeofabrikContinent.Europe,
                iso = "no",
                supportNote =
                    "Offline maps (country + landsdel regions). Truck HOS: EC 561 from GPS. Speed cameras: opt-in. Right-to-roam camping: plugin spec (allemannsretten).",
                testTag = "chip_country_europe_norway",
            ),
            GeofabrikCountry(
                label = "Poland",
                path = "europe/poland",
                continent = GeofabrikContinent.Europe,
                iso = "pl",
                supportNote =
                    "Offline maps (voivodeships per navi-server current.json). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_poland",
            ),
            GeofabrikCountry(
                label = "Portugal",
                path = "europe/portugal",
                continent = GeofabrikContinent.Europe,
                iso = "pt",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_portugal",
            ),
            GeofabrikCountry(
                label = "Romania",
                path = "europe/romania",
                continent = GeofabrikContinent.Europe,
                iso = "ro",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_romania",
            ),
            GeofabrikCountry(
                label = "Russia",
                path = "russia",
                continent = GeofabrikContinent.Europe,
                iso = "ru",
                supportNote =
                    "Offline maps (federal districts per navi-server current.json). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_russia",
            ),
            GeofabrikCountry(
                label = "Serbia",
                path = "europe/serbia",
                continent = GeofabrikContinent.Europe,
                iso = "rs",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_europe_serbia",
            ),
            GeofabrikCountry(
                label = "Slovakia",
                path = "europe/slovakia",
                continent = GeofabrikContinent.Europe,
                iso = "sk",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_slovakia",
            ),
            GeofabrikCountry(
                label = "Slovenia",
                path = "europe/slovenia",
                continent = GeofabrikContinent.Europe,
                iso = "si",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_slovenia",
            ),
            GeofabrikCountry(
                label = "Spain",
                path = "europe/spain",
                continent = GeofabrikContinent.Europe,
                iso = "es",
                supportNote =
                    "Offline maps (autonomous communities per navi-server current.json). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_spain",
            ),
            GeofabrikCountry(
                label = "Sweden",
                path = "europe/sweden",
                continent = GeofabrikContinent.Europe,
                iso = "se",
                supportNote =
                    "Offline maps: Geofabrik publishes only the country extract; län chips install " +
                        "pack-server regions and use the Sweden PBF for place index. " +
                        "Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_sweden",
            ),
            GeofabrikCountry(
                label = "Switzerland",
                path = "europe/switzerland",
                continent = GeofabrikContinent.Europe,
                iso = "ch",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (product policy).",
                testTag = "chip_country_europe_switzerland",
            ),
            GeofabrikCountry(
                label = "Turkey",
                path = "europe/turkey",
                continent = GeofabrikContinent.Europe,
                iso = "tr",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_europe_turkey",
            ),
            GeofabrikCountry(
                label = "Ukraine",
                path = "europe/ukraine",
                continent = GeofabrikContinent.Europe,
                iso = "ua",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_europe_ukraine",
            ),
            GeofabrikCountry(
                label = "Australia",
                path = "australia-oceania/australia",
                continent = GeofabrikContinent.AustraliaOceania,
                iso = "au",
                supportNote =
                    "Offline maps (states / territories per navi-server current.json). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_australia_oceania_australia",
            ),
            GeofabrikCountry(
                label = "Fiji",
                path = "australia-oceania/fiji",
                continent = GeofabrikContinent.AustraliaOceania,
                iso = "fj",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_australia_oceania_fiji",
            ),
            GeofabrikCountry(
                label = "New Caledonia",
                path = "australia-oceania/new-caledonia",
                continent = GeofabrikContinent.AustraliaOceania,
                iso = "nc",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_australia_oceania_new_caledonia",
            ),
            GeofabrikCountry(
                label = "New Zealand",
                path = "australia-oceania/new-zealand",
                continent = GeofabrikContinent.AustraliaOceania,
                iso = "nz",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_australia_oceania_new_zealand",
            ),
            GeofabrikCountry(
                label = "Papua New Guinea",
                path = "australia-oceania/papua-new-guinea",
                continent = GeofabrikContinent.AustraliaOceania,
                iso = "pg",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_australia_oceania_papua_new_guinea",
            ),
            GeofabrikCountry(
                label = "Samoa",
                path = "australia-oceania/samoa",
                continent = GeofabrikContinent.AustraliaOceania,
                iso = "ws",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_australia_oceania_samoa",
            ),
            GeofabrikCountry(
                label = "Solomon Islands",
                path = "australia-oceania/solomon-islands",
                continent = GeofabrikContinent.AustraliaOceania,
                iso = "sb",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_australia_oceania_solomon_islands",
            ),
            GeofabrikCountry(
                label = "Vanuatu",
                path = "australia-oceania/vanuatu",
                continent = GeofabrikContinent.AustraliaOceania,
                iso = "vu",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_australia_oceania_vanuatu",
            ),
        )

    fun countriesIn(continent: GeofabrikContinent): List<GeofabrikCountry> = countries.filter { it.continent == continent }

    fun findByPath(path: String): GeofabrikCountry? {
        val norm = canonicalizePath(path)
        return countries.firstOrNull { it.path == norm }
            ?: countries.firstOrNull { norm.startsWith(it.path + "/") }
    }

    /**
     * Remap retired / relocated Geofabrik paths. Keep in sync with core
     * `canonicalize_geofabrik_region_path`.
     */
    fun canonicalizePath(path: String): String {
        val norm = path.trim().trim('/').lowercase()
        val greaterLondon = "europe/united-kingdom/england/greater-london"
        return when {
            norm == "europe/united-kingdom/england/london" ||
                norm.startsWith("europe/united-kingdom/england/london/") ||
                norm == "enfield" ||
                norm == "london" -> greaterLondon
            norm.startsWith("europe/great-britain/") ->
                "europe/united-kingdom/" + norm.removePrefix("europe/great-britain/")
            else -> norm
        }
    }

    /**
     * Path used for Geofabrik `-latest.osm.pbf` / updates. Sweden län keep their
     * pack region id but download the country extract (Geofabrik has no län PBFs).
     * Mirrors core `geofabrik_extract_path`.
     */
    fun extractPathForPbf(path: String): String {
        val norm = canonicalizePath(path)
        return if (norm.startsWith("europe/sweden/")) "europe/sweden" else norm
    }

    fun continentForPath(path: String): GeofabrikContinent = findByPath(path)?.continent ?: GeofabrikContinent.Europe

    /** Norway landsdeler, Sweden län, Germany/UK, and other pack-server subregion chips. */
    fun hasRegionChips(path: String): Boolean {
        val norm = canonicalizePath(path)
        return regionChipBasePath(norm) != null
    }

    /**
     * Parent path for chip rows, or null.
     * Nested parents (Bayern, California, …) are matched before country roots.
     * Slugs follow navi-server `current.json` (not every Geofabrik historical extract).
     */
    fun regionChipBasePath(path: String): String? {
        val norm = canonicalizePath(path)
        return when {
            norm == "europe/norway" || norm.startsWith("europe/norway/") -> "europe/norway"
            norm == "europe/sweden" || norm.startsWith("europe/sweden/") -> "europe/sweden"
            norm == "europe/germany/baden-wuerttemberg" ||
                norm.startsWith("europe/germany/baden-wuerttemberg/") ->
                "europe/germany/baden-wuerttemberg"
            norm == "europe/germany/bayern" ||
                norm.startsWith("europe/germany/bayern/") ->
                "europe/germany/bayern"
            norm == "europe/germany/nordrhein-westfalen" ||
                norm.startsWith("europe/germany/nordrhein-westfalen/") ->
                "europe/germany/nordrhein-westfalen"
            norm == "europe/germany" || norm.startsWith("europe/germany/") -> "europe/germany"
            norm == "europe/united-kingdom/england" ||
                norm.startsWith("europe/united-kingdom/england/") ->
                "europe/united-kingdom/england"
            norm == "europe/united-kingdom" ||
                norm.startsWith("europe/united-kingdom/") ->
                "europe/united-kingdom"
            norm == "north-america/us/california" ||
                norm.startsWith("north-america/us/california/") ->
                "north-america/us/california"
            norm == "north-america/us" || norm.startsWith("north-america/us/") ->
                "north-america/us"
            norm == "north-america/canada/british-columbia" ||
                norm.startsWith("north-america/canada/british-columbia/") ->
                "north-america/canada/british-columbia"
            norm == "north-america/canada/nunavut" ||
                norm.startsWith("north-america/canada/nunavut/") ->
                "north-america/canada/nunavut"
            norm == "north-america/canada" ||
                norm.startsWith("north-america/canada/") ->
                "north-america/canada"
            norm == "asia/china" || norm.startsWith("asia/china/") -> "asia/china"
            norm == "europe/france" || norm.startsWith("europe/france/") -> "europe/france"
            norm == "europe/spain" || norm.startsWith("europe/spain/") -> "europe/spain"
            norm == "europe/poland" || norm.startsWith("europe/poland/") -> "europe/poland"
            norm == "europe/czech-republic" ||
                norm.startsWith("europe/czech-republic/") ->
                "europe/czech-republic"
            norm == "europe/netherlands" ||
                norm.startsWith("europe/netherlands/") ->
                "europe/netherlands"
            norm == "australia-oceania/australia" ||
                norm.startsWith("australia-oceania/australia/") ->
                "australia-oceania/australia"
            norm == "russia" || norm.startsWith("russia/") -> "russia"
            norm == "asia/japan" || norm.startsWith("asia/japan/") -> "asia/japan"
            norm == "asia/indonesia" || norm.startsWith("asia/indonesia/") -> "asia/indonesia"
            norm == "asia/india" || norm.startsWith("asia/india/") -> "asia/india"
            norm == "south-america/brazil" ||
                norm.startsWith("south-america/brazil/") ->
                "south-america/brazil"
            norm == "europe/italy" || norm.startsWith("europe/italy/") -> "europe/italy"
            else -> null
        }
    }

    /** Default leaf when switching Country → Region in country. */
    fun defaultRegionChipPath(path: String): String? =
        when (regionChipBasePath(path)) {
            "europe/norway" -> "europe/norway/ostlandet"
            "europe/sweden" -> "europe/sweden/stockholm"
            "europe/germany" -> "europe/germany/bremen"
            "europe/germany/baden-wuerttemberg" ->
                "europe/germany/baden-wuerttemberg/stuttgart-regbez"
            "europe/germany/bayern" -> "europe/germany/bayern/oberbayern"
            "europe/germany/nordrhein-westfalen" ->
                "europe/germany/nordrhein-westfalen/duesseldorf-regbez"
            "europe/united-kingdom" -> "europe/united-kingdom/england"
            "europe/united-kingdom/england" ->
                "europe/united-kingdom/england/greater-london"
            "north-america/us" -> "north-america/us/washington"
            "north-america/us/california" -> "north-america/us/california/norcal"
            "north-america/canada" -> "north-america/canada/ontario"
            "north-america/canada/british-columbia" ->
                "north-america/canada/british-columbia/southcoast-admreg"
            "north-america/canada/nunavut" -> "north-america/canada/nunavut/qikiqtaaluk"
            "asia/china" -> "asia/china/beijing"
            "europe/france" -> "europe/france/ile-de-france"
            "europe/spain" -> "europe/spain/madrid"
            "europe/poland" -> "europe/poland/mazowieckie"
            "europe/czech-republic" -> "europe/czech-republic/praha"
            "europe/netherlands" -> "europe/netherlands/noord-holland"
            "australia-oceania/australia" ->
                "australia-oceania/australia/new-south-wales"
            "russia" -> "russia/central-fed-district"
            "asia/japan" -> "asia/japan/kanto"
            "asia/indonesia" -> "asia/indonesia/java"
            "asia/india" -> "asia/india/northern-zone"
            "south-america/brazil" -> "south-america/brazil/sudeste"
            "europe/italy" -> "europe/italy/nord-ovest"
            else -> null
        }

    /** Slug → display label for the active country's region chips. */
    fun regionChipsFor(path: String): List<Pair<String, String>>? =
        when (regionChipBasePath(path)) {
            "europe/norway" -> norwayRegions
            "europe/sweden" -> swedenRegions
            "europe/germany" -> germanyRegions
            "europe/germany/baden-wuerttemberg" -> germanyBadenWuerttembergRegions
            "europe/germany/bayern" -> germanyBayernRegions
            "europe/germany/nordrhein-westfalen" -> germanyNordrheinWestfalenRegions
            "europe/united-kingdom" -> unitedKingdomNations
            "europe/united-kingdom/england" -> englandCounties
            "north-america/us" -> usStates
            "north-america/us/california" -> usCaliforniaRegions
            "north-america/canada" -> canadaRegions
            "north-america/canada/british-columbia" -> canadaBritishColumbiaRegions
            "north-america/canada/nunavut" -> canadaNunavutRegions
            "asia/china" -> chinaRegions
            "europe/france" -> franceRegions
            "europe/spain" -> spainRegions
            "europe/poland" -> polandRegions
            "europe/czech-republic" -> czechRepublicRegions
            "europe/netherlands" -> netherlandsRegions
            "australia-oceania/australia" -> australiaRegions
            "russia" -> russiaRegions
            "asia/japan" -> japanRegions
            "asia/indonesia" -> indonesiaRegions
            "asia/india" -> indiaRegions
            "south-america/brazil" -> brazilRegions
            "europe/italy" -> italyRegions
            else -> null
        }

    /**
     * True when [path] is a pack-server region id the UI may index under:
     * a country root from the catalog, a chip parent, or a known chip leaf.
     * Rejects invented paths such as `europe/norway/niedersachsen`.
     */
    fun isKnownPackRegionId(path: String): Boolean {
        val n = canonicalizePath(path)
        if (n.isEmpty()) return false
        val country = findByPath(n) ?: return false
        if (n == country.path) return true
        val base = regionChipBasePath(n) ?: return false
        if (n == base) return true
        if (!n.startsWith("$base/")) return false
        val chips = regionChipsFor(base) ?: return false
        val rest = n.removePrefix("$base/")
        return chips.any { (slug, _) -> rest == slug || rest.startsWith("$slug/") }
    }

    /**
     * Expand [root] chip row into pack-catalog leaf paths (for ready-pill coverage).
     * Parents that have their own chip row (Bayern, California, …) expand to their leaves.
     */
    fun packCatalogLeafPaths(root: String): List<String> {
        val base = regionChipBasePath(root) ?: return emptyList()
        val chips = regionChipsFor(base) ?: return emptyList()
        val leaves = mutableListOf<String>()
        for ((slug, _) in chips) {
            val child = "$base/$slug"
            if (regionChipBasePath(child) == child) {
                leaves += packCatalogLeafPaths(child)
            } else {
                leaves += child
            }
        }
        return leaves
    }

    /** All pack-catalog leaf paths under `europe/germany` (for ready-pill coverage). */
    fun germanyPackLeafPaths(): List<String> = packCatalogLeafPaths("europe/germany")

    val norwayRegions: List<Pair<String, String>> =
        listOf(
            // Slugs match navi-server current.json under europe/norway.
            "nord-norge" to "Nord-Norge",
            "ostlandet" to "Østlandet",
            "sorlandet" to "Sørlandet",
            "svalbard-janmayen" to "Svalbard / Jan Mayen",
            "trondelag" to "Trøndelag",
            "vestlandet" to "Vestlandet",
        )

    /**
     * Sweden län chips — pack-server `region_id` leaves under `europe/sweden`.
     * Geofabrik has no län PBFs; [extractPathForPbf] uses the country extract.
     */
    val swedenRegions: List<Pair<String, String>> =
        listOf(
            "blekinge" to "Blekinge",
            "dalarna" to "Dalarna",
            "gavleborg" to "Gävleborg",
            "gotland" to "Gotland",
            "halland" to "Halland",
            "jamtland" to "Jämtland",
            "jonkoping" to "Jönköping",
            "kalmar" to "Kalmar",
            "kronoberg" to "Kronoberg",
            "norrbotten" to "Norrbotten",
            "orebro" to "Örebro",
            "ostergotland" to "Östergötland",
            "skane" to "Skåne",
            "sodermanland" to "Södermanland",
            "stockholm" to "Stockholm",
            "uppsala" to "Uppsala",
            "varmland" to "Värmland",
            "vasterbotten" to "Västerbotten",
            "vasternorrland" to "Västernorrland",
            "vastmanland" to "Västmanland",
            "vastra_gotaland" to "Västra Götaland",
        )

    /**
     * German Bundesländer chips — parents for BW/Bayern/NRW drill into
     * Regierungsbezirk leaves published in navi-server `current.json`.
     */
    val germanyRegions: List<Pair<String, String>> =
        listOf(
            "baden-wuerttemberg" to "Baden-Württemberg",
            "bayern" to "Bayern",
            "berlin" to "Berlin",
            "brandenburg" to "Brandenburg",
            "bremen" to "Bremen",
            "hamburg" to "Hamburg",
            "hessen" to "Hessen",
            "mecklenburg-vorpommern" to "Mecklenburg-Vorpommern",
            "niedersachsen" to "Niedersachsen",
            "nordrhein-westfalen" to "Nordrhein-Westfalen",
            "rheinland-pfalz" to "Rheinland-Pfalz",
            "saarland" to "Saarland",
            "sachsen" to "Sachsen",
            "sachsen-anhalt" to "Sachsen-Anhalt",
            "schleswig-holstein" to "Schleswig-Holstein",
            "thueringen" to "Thüringen",
        )

    /** Pack-catalog leaves under `europe/germany/baden-wuerttemberg`. */
    val germanyBadenWuerttembergRegions: List<Pair<String, String>> =
        listOf(
            "freiburg-regbez" to "Freiburg",
            "karlsruhe-regbez" to "Karlsruhe",
            "stuttgart-regbez" to "Stuttgart",
            "tuebingen-regbez" to "Tübingen",
        )

    /** Pack-catalog leaves under `europe/germany/bayern`. */
    val germanyBayernRegions: List<Pair<String, String>> =
        listOf(
            "mittelfranken" to "Mittelfranken",
            "niederbayern" to "Niederbayern",
            "oberbayern" to "Oberbayern",
            "oberfranken" to "Oberfranken",
            "oberpfalz" to "Oberpfalz",
            "schwaben" to "Schwaben",
            "unterfranken" to "Unterfranken",
        )

    /** Pack-catalog leaves under `europe/germany/nordrhein-westfalen`. */
    val germanyNordrheinWestfalenRegions: List<Pair<String, String>> =
        listOf(
            "arnsberg-regbez" to "Arnsberg",
            "detmold-regbez" to "Detmold",
            "duesseldorf-regbez" to "Düsseldorf",
            "koeln-regbez" to "Köln",
            "muenster-regbez" to "Münster",
        )

    /** Pack-catalog regions under `north-america/us` (navi-server current.json). */
    val usStates: List<Pair<String, String>> =
        listOf(
            "alabama" to "Alabama",
            "alaska" to "Alaska",
            "arizona" to "Arizona",
            "arkansas" to "Arkansas",
            "california" to "California",
            "colorado" to "Colorado",
            "connecticut" to "Connecticut",
            "delaware" to "Delaware",
            "district-of-columbia" to "District of Columbia",
            "florida" to "Florida",
            "georgia" to "Georgia",
            "hawaii" to "Hawaii",
            "idaho" to "Idaho",
            "illinois" to "Illinois",
            "indiana" to "Indiana",
            "iowa" to "Iowa",
            "kansas" to "Kansas",
            "kentucky" to "Kentucky",
            "louisiana" to "Louisiana",
            "maine" to "Maine",
            "maryland" to "Maryland",
            "massachusetts" to "Massachusetts",
            "michigan" to "Michigan",
            "minnesota" to "Minnesota",
            "mississippi" to "Mississippi",
            "missouri" to "Missouri",
            "montana" to "Montana",
            "nebraska" to "Nebraska",
            "nevada" to "Nevada",
            "new-hampshire" to "New Hampshire",
            "new-jersey" to "New Jersey",
            "new-mexico" to "New Mexico",
            "new-york" to "New York",
            "north-carolina" to "North Carolina",
            "north-dakota" to "North Dakota",
            "ohio" to "Ohio",
            "oklahoma" to "Oklahoma",
            "oregon" to "Oregon",
            "pennsylvania" to "Pennsylvania",
            "puerto-rico" to "Puerto Rico",
            "rhode-island" to "Rhode Island",
            "south-carolina" to "South Carolina",
            "south-dakota" to "South Dakota",
            "tennessee" to "Tennessee",
            "texas" to "Texas",
            "us-virgin-islands" to "US Virgin Islands",
            "utah" to "Utah",
            "vermont" to "Vermont",
            "virginia" to "Virginia",
            "washington" to "Washington",
            "west-virginia" to "West Virginia",
            "wisconsin" to "Wisconsin",
            "wyoming" to "Wyoming",
        )

    /** Pack-catalog regions under `asia/china` (navi-server current.json). */
    val chinaRegions: List<Pair<String, String>> =
        listOf(
            "anhui" to "Anhui",
            "beijing" to "Beijing",
            "chongqing" to "Chongqing",
            "fujian" to "Fujian",
            "gansu" to "Gansu",
            "guangdong" to "Guangdong",
            "guangxi" to "Guangxi",
            "guizhou" to "Guizhou",
            "hainan" to "Hainan",
            "hebei" to "Hebei",
            "heilongjiang" to "Heilongjiang",
            "henan" to "Henan",
            "hong-kong" to "Hong Kong",
            "hubei" to "Hubei",
            "hunan" to "Hunan",
            "inner-mongolia" to "Inner Mongolia",
            "jiangsu" to "Jiangsu",
            "jiangxi" to "Jiangxi",
            "jilin" to "Jilin",
            "liaoning" to "Liaoning",
            "macau" to "Macau",
            "ningxia" to "Ningxia",
            "qinghai" to "Qinghai",
            "shaanxi" to "Shaanxi",
            "shandong" to "Shandong",
            "shanghai" to "Shanghai",
            "shanxi" to "Shanxi",
            "sichuan" to "Sichuan",
            "tianjin" to "Tianjin",
            "tibet" to "Tibet",
            "xinjiang" to "Xinjiang",
            "yunnan" to "Yunnan",
            "zhejiang" to "Zhejiang",
        )

    /** Pack-catalog regions under `europe/france` (navi-server current.json). */
    val franceRegions: List<Pair<String, String>> =
        listOf(
            "alsace" to "Alsace",
            "aquitaine" to "Aquitaine",
            "auvergne" to "Auvergne",
            "basse-normandie" to "Basse-Normandie",
            "bourgogne" to "Bourgogne",
            "bretagne" to "Bretagne",
            "centre" to "Centre",
            "champagne-ardenne" to "Champagne-Ardenne",
            "corse" to "Corse",
            "franche-comte" to "Franche-Comté",
            "guadeloupe" to "Guadeloupe",
            "guyane" to "Guyane",
            "haute-normandie" to "Haute-Normandie",
            "ile-de-france" to "Île-de-France",
            "languedoc-roussillon" to "Languedoc-Roussillon",
            "limousin" to "Limousin",
            "lorraine" to "Lorraine",
            "martinique" to "Martinique",
            "mayotte" to "Mayotte",
            "midi-pyrenees" to "Midi-Pyrénées",
            "nord-pas-de-calais" to "Nord-Pas-de-Calais",
            "pays-de-la-loire" to "Pays de la Loire",
            "picardie" to "Picardie",
            "poitou-charentes" to "Poitou Charentes",
            "provence-alpes-cote-d-azur" to "Provence-Alpes-Côte d'Azur",
            "reunion" to "Réunion",
            "rhone-alpes" to "Rhône-Alpes",
        )

    /** Pack-catalog regions under `north-america/canada` (navi-server current.json). */
    val canadaRegions: List<Pair<String, String>> =
        listOf(
            "alberta" to "Alberta",
            "british-columbia" to "British Columbia",
            "manitoba" to "Manitoba",
            "new-brunswick" to "New Brunswick",
            "newfoundland-and-labrador" to "Newfoundland and Labrador",
            "northwest-territories" to "Northwest Territories",
            "nova-scotia" to "Nova Scotia",
            "nunavut" to "Nunavut",
            "ontario" to "Ontario",
            "prince-edward-island" to "Prince Edward Island",
            "quebec" to "Quebec",
            "saskatchewan" to "Saskatchewan",
            "yukon" to "Yukon",
        )

    /** Pack-catalog regions under `europe/spain` (navi-server current.json). */
    val spainRegions: List<Pair<String, String>> =
        listOf(
            "andalucia" to "Andalucía",
            "aragon" to "Aragón",
            "asturias" to "Asturias",
            "cantabria" to "Cantabria",
            "castilla-la-mancha" to "Castilla-La Mancha",
            "castilla-y-leon" to "Castilla y León",
            "cataluna" to "Cataluña",
            "ceuta" to "Ceuta",
            "extremadura" to "Extremadura",
            "galicia" to "Galicia",
            "islas-baleares" to "Islas Baleares",
            "la-rioja" to "La Rioja",
            "madrid" to "Madrid",
            "melilla" to "Melilla",
            "murcia" to "Murcia",
            "navarra" to "Navarra",
            "pais-vasco" to "País Vasco",
            "valencia" to "Valencia",
        )

    /** Pack-catalog regions under `europe/poland` (navi-server current.json). */
    val polandRegions: List<Pair<String, String>> =
        listOf(
            "dolnoslaskie" to "Dolnośląskie",
            "kujawsko-pomorskie" to "Kujawsko-Pomorskie",
            "lodzkie" to "Łódzkie",
            "lubelskie" to "Lubelskie",
            "lubuskie" to "Lubuskie",
            "malopolskie" to "Małopolskie",
            "mazowieckie" to "Mazowieckie",
            "opolskie" to "Opolskie",
            "podkarpackie" to "Podkarpackie",
            "podlaskie" to "Podlaskie",
            "pomorskie" to "Pomorskie",
            "slaskie" to "Śląskie",
            "swietokrzyskie" to "Świętokrzyskie",
            "warminsko-mazurskie" to "Warmińsko-Mazurskie",
            "wielkopolskie" to "Wielkopolskie",
            "zachodniopomorskie" to "Zachodniopomorskie",
        )

    /** Pack-catalog regions under `europe/czech-republic` (navi-server current.json). */
    val czechRepublicRegions: List<Pair<String, String>> =
        listOf(
            "jihocesky" to "Jihočeský",
            "jihomoravsky" to "Jihomoravský",
            "karlovarsky" to "Karlovarský",
            "kralovehradecky" to "Královéhradecký",
            "liberecky" to "Liberecký",
            "moravskoslezky" to "Moravskoslezský",
            "olomoucky" to "Olomoucký",
            "pardubicky" to "Pardubický",
            "plzensky" to "Plzeňský",
            "praha" to "Praha",
            "stredocesky" to "Středočeský",
            "ustecky" to "Ústecký",
            "vysocina" to "Vysočina",
            "zlinsky" to "Zlínský",
        )

    /** Pack-catalog regions under `europe/netherlands` (navi-server current.json). */
    val netherlandsRegions: List<Pair<String, String>> =
        listOf(
            "drenthe" to "Drenthe",
            "flevoland" to "Flevoland",
            "friesland" to "Friesland",
            "gelderland" to "Gelderland",
            "groningen" to "Groningen",
            "limburg" to "Limburg",
            "noord-brabant" to "Noord-Brabant",
            "noord-holland" to "Noord-Holland",
            "overijssel" to "Overijssel",
            "utrecht" to "Utrecht",
            "zeeland" to "Zeeland",
            "zuid-holland" to "Zuid-Holland",
        )

    /** Pack-catalog regions under `australia-oceania/australia` (navi-server current.json). */
    val australiaRegions: List<Pair<String, String>> =
        listOf(
            "act" to "ACT",
            "christmas-island" to "Christmas Island",
            "cocos-islands" to "Cocos Islands",
            "coral-sea-islands" to "Coral Sea Islands",
            "new-south-wales" to "New South Wales",
            "norfolk-island" to "Norfolk Island",
            "northern-territory" to "Northern Territory",
            "queensland" to "Queensland",
            "south-australia" to "South Australia",
            "tasmania" to "Tasmania",
            "victoria" to "Victoria",
            "western-australia" to "Western Australia",
        )

    /** Pack-catalog regions under `russia` (navi-server current.json). */
    val russiaRegions: List<Pair<String, String>> =
        listOf(
            "central-fed-district" to "Central Federal District",
            "crimean-fed-district" to "Crimean Federal District",
            "far-eastern-fed-district" to "Far Eastern Federal District",
            "kaliningrad" to "Kaliningrad",
            "north-caucasus-fed-district" to "North Caucasus Federal District",
            "northwestern-fed-district" to "Northwestern Federal District",
            "siberian-fed-district" to "Siberian Federal District",
            "south-fed-district" to "Southern Federal District",
            "ural-fed-district" to "Ural Federal District",
            "volga-fed-district" to "Volga Federal District",
        )

    /** Pack-catalog regions under `asia/japan` (navi-server current.json). */
    val japanRegions: List<Pair<String, String>> =
        listOf(
            "chubu" to "Chubu",
            "chugoku" to "Chugoku",
            "hokkaido" to "Hokkaido",
            "kansai" to "Kansai",
            "kanto" to "Kanto",
            "kyushu" to "Kyushu",
            "shikoku" to "Shikoku",
            "tohoku" to "Tohoku",
        )

    /** Pack-catalog regions under `asia/indonesia` (navi-server current.json). */
    val indonesiaRegions: List<Pair<String, String>> =
        listOf(
            "java" to "Java",
            "kalimantan" to "Kalimantan",
            "maluku" to "Maluku",
            "nusa-tenggara" to "Nusa Tenggara",
            "papua" to "Papua",
            "sulawesi" to "Sulawesi",
            "sumatra" to "Sumatra",
        )

    /** Pack-catalog regions under `asia/india` (navi-server current.json). */
    val indiaRegions: List<Pair<String, String>> =
        listOf(
            "central-zone" to "Central Zone",
            "eastern-zone" to "Eastern Zone",
            "north-eastern-zone" to "North-Eastern Zone",
            "northern-zone" to "Northern Zone",
            "southern-zone" to "Southern Zone",
            "western-zone" to "Western Zone",
        )

    /** Pack-catalog regions under `south-america/brazil` (navi-server current.json). */
    val brazilRegions: List<Pair<String, String>> =
        listOf(
            "centro-oeste" to "Centro-Oeste",
            "nordeste" to "Nordeste",
            "norte" to "Norte",
            "sudeste" to "Sudeste",
            "sul" to "Sul",
        )

    /** Pack-catalog regions under `europe/italy` (navi-server current.json). */
    val italyRegions: List<Pair<String, String>> =
        listOf(
            "centro" to "Centro",
            "isole" to "Isole",
            "nord-est" to "Nord-Est",
            "nord-ovest" to "Nord-Ovest",
            "sud" to "Sud",
        )

    /** Pack-catalog leaves under `north-america/us/california`. */
    val usCaliforniaRegions: List<Pair<String, String>> =
        listOf(
            "norcal" to "Northern California",
            "socal" to "Southern California",
        )

    /** Pack-catalog leaves under `north-america/canada/british-columbia`. */
    val canadaBritishColumbiaRegions: List<Pair<String, String>> =
        listOf(
            "interior-admreg" to "Interior",
            "island-admreg" to "Island",
            "kootenay-admreg" to "Kootenay",
            "north-admreg" to "North",
            "okanagan-admreg" to "Okanagan",
            "southcoast-admreg" to "South Coast",
        )

    /** Pack-catalog leaves under `north-america/canada/nunavut`. */
    val canadaNunavutRegions: List<Pair<String, String>> =
        listOf(
            "kitikmeot" to "Kitikmeot",
            "kivalliq" to "Kivalliq",
            "qikiqtaaluk" to "Qikiqtaaluk",
        )

    /** Top-level UK nation / territory chips (live pack catalog). */
    val unitedKingdomNations: List<Pair<String, String>> =
        listOf(
            "bermuda" to "Bermuda",
            "england" to "England",
            "falklands" to "Falklands",
            "scotland" to "Scotland",
            "wales" to "Wales",
        )

    /**
     * England ceremonial/county extracts from Geofabrik index-v1 (no London
     * borough leaves — only Greater London).
     */
    val englandCounties: List<Pair<String, String>> =
        listOf(
            "bedfordshire" to "Bedfordshire",
            "berkshire" to "Berkshire",
            "bristol" to "Bristol",
            "buckinghamshire" to "Buckinghamshire",
            "cambridgeshire" to "Cambridgeshire",
            "cheshire" to "Cheshire",
            "cornwall" to "Cornwall",
            "cumbria" to "Cumbria",
            "derbyshire" to "Derbyshire",
            "devon" to "Devon",
            "dorset" to "Dorset",
            "durham" to "Durham",
            "east-sussex" to "East Sussex",
            "east-yorkshire-with-hull" to "East Yorkshire with Hull",
            "essex" to "Essex",
            "gloucestershire" to "Gloucestershire",
            "greater-london" to "Greater London",
            "greater-manchester" to "Greater Manchester",
            "hampshire" to "Hampshire",
            "herefordshire" to "Herefordshire",
            "hertfordshire" to "Hertfordshire",
            "isle-of-wight" to "Isle of Wight",
            "kent" to "Kent",
            "lancashire" to "Lancashire",
            "leicestershire" to "Leicestershire",
            "lincolnshire" to "Lincolnshire",
            "merseyside" to "Merseyside",
            "norfolk" to "Norfolk",
            "north-yorkshire" to "North Yorkshire",
            "northamptonshire" to "Northamptonshire",
            "northumberland" to "Northumberland",
            "nottinghamshire" to "Nottinghamshire",
            "oxfordshire" to "Oxfordshire",
            "rutland" to "Rutland",
            "shropshire" to "Shropshire",
            "somerset" to "Somerset",
            "south-yorkshire" to "South Yorkshire",
            "staffordshire" to "Staffordshire",
            "suffolk" to "Suffolk",
            "surrey" to "Surrey",
            "tyne-and-wear" to "Tyne and Wear",
            "warwickshire" to "Warwickshire",
            "west-midlands" to "West Midlands",
            "west-sussex" to "West Sussex",
            "west-yorkshire" to "West Yorkshire",
            "wiltshire" to "Wiltshire",
            "worcestershire" to "Worcestershire",
        )

    const val EMPTY_CONTINENT_NOTE =
        "No supported map extracts for this continent yet."

    /**
     * Shown under **Region in country** when this catalog has no sub-region chips
     * for the selected country. Wording matches real Geofabrik granularity.
     */
    fun regionGranularityNote(path: String): String {
        val country = findByPath(path)?.path
        return when (country) {
            "europe/great-britain" ->
                "Great Britain is the GB-only country extract. For England/Scotland/Wales/counties " +
                    "(including Greater London), select United Kingdom. London borough extracts were retired."
            else ->
                "Sub-region chips follow navi-server current.json for Norway, Sweden, Germany, " +
                    "the United Kingdom, France, Spain, Poland, Czechia, the Netherlands, Italy, " +
                    "the US, Canada, China, Japan, India, Indonesia, Australia, Brazil, and Russia. " +
                    "Enter a Geofabrik subpath in the field below, or switch back to Country."
        }
    }
}
