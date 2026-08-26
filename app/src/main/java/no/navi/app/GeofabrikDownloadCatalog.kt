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
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_asia_china",
            ),
            GeofabrikCountry(
                label = "India",
                path = "asia/india",
                continent = GeofabrikContinent.Asia,
                iso = "in",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
                testTag = "chip_country_asia_india",
            ),
            GeofabrikCountry(
                label = "Indonesia",
                path = "asia/indonesia",
                continent = GeofabrikContinent.Asia,
                iso = "id",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
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
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
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
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
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
                    "Offline maps (country extract; large). Truck HOS: FMCSA from GPS. Speed cameras: decline.",
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
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
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
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
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
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (product policy).",
                testTag = "chip_country_europe_france",
            ),
            GeofabrikCountry(
                label = "Germany",
                path = "europe/germany",
                continent = GeofabrikContinent.Europe,
                iso = "de",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (product policy).",
                testTag = "chip_country_europe_germany",
            ),
            GeofabrikCountry(
                label = "Great Britain",
                path = "europe/great-britain",
                continent = GeofabrikContinent.Europe,
                iso = "gb",
                supportNote =
                    "Offline maps (country extract). Truck HOS: decline until a dedicated UK pack exists. Speed cameras: opt-in.",
                testTag = "chip_country_europe_great_britain",
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
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
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
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
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
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
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
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
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
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
                testTag = "chip_country_europe_spain",
            ),
            GeofabrikCountry(
                label = "Sweden",
                path = "europe/sweden",
                continent = GeofabrikContinent.Europe,
                iso = "se",
                supportNote =
                    "Offline maps (country extract). Truck HOS: EC 561 from GPS. Speed cameras: decline (not allow-listed).",
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
                    "Offline maps (country extract). Truck HOS: decline (no keyed pack). Speed cameras: decline.",
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
        val norm = path.trim().trim('/').lowercase()
        return countries.firstOrNull { it.path == norm }
            ?: countries.firstOrNull { norm.startsWith(it.path + "/") }
    }

    fun continentForPath(path: String): GeofabrikContinent = findByPath(path)?.continent ?: GeofabrikContinent.Europe

    /** Norway is the only country with landsdel region chips in Tools today. */
    fun hasRegionChips(path: String): Boolean {
        val norm = path.trim().trim('/').lowercase()
        return norm == "europe/norway" || norm.startsWith("europe/norway/")
    }

    val norwayRegions: List<Pair<String, String>> =
        listOf(
            "ostlandet" to "Østlandet",
            "vestlandet" to "Vestlandet",
            "trondelag" to "Trøndelag",
            "nord-norge" to "Nord-Norge",
            "sorlandet" to "Sørlandet",
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
            "europe/sweden" ->
                "Sweden is available as a country extract only — Geofabrik does not publish län-level files. Switch back to Country to download Sweden."
            "north-america/us" ->
                "US states are published by Geofabrik, but this picker lists the country extract. Enter a state path such as north-america/us/west-virginia, or switch back to Country."
            "europe/germany" ->
                "German states are published by Geofabrik, but this picker lists the country extract. Enter a state path such as europe/germany/bremen, or switch back to Country."
            "russia" ->
                "Geofabrik publishes Russian federal-district extracts, but this picker lists the country extract only. Enter a district path such as russia/kaliningrad, or switch back to Country."
            else ->
                "Sub-region chips are listed for Norway only today. " +
                    "Enter a Geofabrik subpath in the field below " +
                    "(e.g. europe/germany/bayern), or switch back to Country."
        }
    }
}
