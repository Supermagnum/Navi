package no.navi.app

/**
 * Geofabrik pack bake ids → CoMaps MWM leaf ids when `region_to_leaves` on the
 * device catalog omits non-Nordic corridors (Germany first). Keys are lowercase
 * with hyphens normalized to underscores (`europe_germany_niedersachsen`).
 * Verified against navi-mwm-catalog v2 / CoMaps release 261115 leaf ids.
 */
internal object LongTripMwmGeofabrikAliases {
    private val bakeIdToLeaves: Map<String, List<String>> = mapOf(
        "europe_germany" to listOf("Germany_Baden-Wurttemberg_Regierungsbezirk Freiburg", "Germany_Baden-Wurttemberg_Regierungsbezirk Karlsruhe", "Germany_Baden-Wurttemberg_Regierungsbezirk Stuttgart_Heilbronn", "Germany_Baden-Wurttemberg_Regierungsbezirk Stuttgart_Stuttgart", "Germany_Baden-Wurttemberg_Regierungsbezirk Tubingen", "Germany_Berlin", "Germany_Brandenburg_North", "Germany_Brandenburg_South", "Germany_Free State of Bavaria_Lower Bavaria", "Germany_Free State of Bavaria_Lower Franconia", "Germany_Free State of Bavaria_Middle Franconia", "Germany_Free State of Bavaria_Swabia", "Germany_Free State of Bavaria_Upper Bavaria_East", "Germany_Free State of Bavaria_Upper Bavaria_Ingolstadt", "Germany_Free State of Bavaria_Upper Bavaria_Munchen", "Germany_Free State of Bavaria_Upper Bavaria_South", "Germany_Free State of Bavaria_Upper Franconia", "Germany_Free State of Bavaria_Upper Palatinate", "Germany_Hamburg_main", "Germany_Hesse_Regierungsbezirk Darmstadt", "Germany_Hesse_Regierungsbezirk Giessen", "Germany_Hesse_Regierungsbezirk Kassel", "Germany_Lower Saxony_Braunschweig", "Germany_Lower Saxony_Bremen_Bremen", "Germany_Lower Saxony_Bremen_Munster", "Germany_Lower Saxony_Hannover", "Germany_Lower Saxony_Oldenburg", "Germany_Mecklenburg-Vorpommern", "Germany_North Rhine-Westphalia_Regierungsbezirk Arnsberg_Arnsberg", "Germany_North Rhine-Westphalia_Regierungsbezirk Arnsberg_Dortmund", "Germany_North Rhine-Westphalia_Regierungsbezirk Detmold", "Germany_North Rhine-Westphalia_Regierungsbezirk Dusseldorf_Dusseldorf", "Germany_North Rhine-Westphalia_Regierungsbezirk Dusseldorf_Mulheim", "Germany_North Rhine-Westphalia_Regierungsbezirk Dusseldorf_Wesel", "Germany_North Rhine-Westphalia_Regierungsbezirk Koln_Aachen", "Germany_North Rhine-Westphalia_Regierungsbezirk Koln_Koln", "Germany_North Rhine-Westphalia_Regierungsbezirk Munster_Munster", "Germany_North Rhine-Westphalia_Regierungsbezirk Munster_Recklinghausen", "Germany_Rhineland-Palatinate_Koblenz", "Germany_Rhineland-Palatinate_South", "Germany_Saarland", "Germany_Saxony-Anhalt_Halle", "Germany_Saxony-Anhalt_Magdeburg", "Germany_Saxony_Dresden", "Germany_Saxony_Leipzig", "Germany_Schleswig-Holstein_Flensburg", "Germany_Schleswig-Holstein_Kiel", "Germany_Thuringia"),
        "europe_germany_baden_wuerttemberg" to listOf("Germany_Baden-Wurttemberg_Regierungsbezirk Freiburg", "Germany_Baden-Wurttemberg_Regierungsbezirk Karlsruhe", "Germany_Baden-Wurttemberg_Regierungsbezirk Stuttgart_Heilbronn", "Germany_Baden-Wurttemberg_Regierungsbezirk Stuttgart_Stuttgart", "Germany_Baden-Wurttemberg_Regierungsbezirk Tubingen"),
        "europe_germany_baden_wuerttemberg_freiburg_regbez" to listOf("Germany_Baden-Wurttemberg_Regierungsbezirk Freiburg"),
        "europe_germany_baden_wuerttemberg_karlsruhe_regbez" to listOf("Germany_Baden-Wurttemberg_Regierungsbezirk Karlsruhe"),
        "europe_germany_baden_wuerttemberg_stuttgart_regbez" to listOf("Germany_Baden-Wurttemberg_Regierungsbezirk Stuttgart_Heilbronn", "Germany_Baden-Wurttemberg_Regierungsbezirk Stuttgart_Stuttgart"),
        "europe_germany_baden_wuerttemberg_tuebingen_regbez" to listOf("Germany_Baden-Wurttemberg_Regierungsbezirk Tubingen"),
        "europe_germany_bayern" to listOf("Germany_Free State of Bavaria_Lower Bavaria", "Germany_Free State of Bavaria_Lower Franconia", "Germany_Free State of Bavaria_Middle Franconia", "Germany_Free State of Bavaria_Swabia", "Germany_Free State of Bavaria_Upper Bavaria_East", "Germany_Free State of Bavaria_Upper Bavaria_Ingolstadt", "Germany_Free State of Bavaria_Upper Bavaria_Munchen", "Germany_Free State of Bavaria_Upper Bavaria_South", "Germany_Free State of Bavaria_Upper Franconia", "Germany_Free State of Bavaria_Upper Palatinate"),
        "europe_germany_bayern_mittelfranken" to listOf("Germany_Free State of Bavaria_Middle Franconia"),
        "europe_germany_bayern_niederbayern" to listOf("Germany_Free State of Bavaria_Lower Bavaria"),
        "europe_germany_bayern_oberbayern" to listOf("Germany_Free State of Bavaria_Upper Bavaria_East", "Germany_Free State of Bavaria_Upper Bavaria_Ingolstadt", "Germany_Free State of Bavaria_Upper Bavaria_Munchen", "Germany_Free State of Bavaria_Upper Bavaria_South"),
        "europe_germany_bayern_oberfranken" to listOf("Germany_Free State of Bavaria_Upper Franconia"),
        "europe_germany_bayern_oberpfalz" to listOf("Germany_Free State of Bavaria_Upper Palatinate"),
        "europe_germany_bayern_schwaben" to listOf("Germany_Free State of Bavaria_Swabia"),
        "europe_germany_bayern_unterfranken" to listOf("Germany_Free State of Bavaria_Lower Franconia"),
        "europe_germany_berlin" to listOf("Germany_Berlin"),
        "europe_germany_brandenburg" to listOf("Germany_Brandenburg_North", "Germany_Brandenburg_South"),
        "europe_germany_bremen" to listOf("Germany_Lower Saxony_Bremen_Bremen", "Germany_Lower Saxony_Bremen_Munster"),
        "europe_germany_hamburg" to listOf("Germany_Hamburg_main"),
        "europe_germany_hessen" to listOf("Germany_Hesse_Regierungsbezirk Darmstadt", "Germany_Hesse_Regierungsbezirk Giessen", "Germany_Hesse_Regierungsbezirk Kassel"),
        "europe_germany_mecklenburg_vorpommern" to listOf("Germany_Mecklenburg-Vorpommern"),
        "europe_germany_niedersachsen" to listOf("Germany_Lower Saxony_Braunschweig", "Germany_Lower Saxony_Bremen_Bremen", "Germany_Lower Saxony_Bremen_Munster", "Germany_Lower Saxony_Hannover", "Germany_Lower Saxony_Oldenburg"),
        "europe_germany_nordrhein_westfalen" to listOf("Germany_North Rhine-Westphalia_Regierungsbezirk Arnsberg_Arnsberg", "Germany_North Rhine-Westphalia_Regierungsbezirk Arnsberg_Dortmund", "Germany_North Rhine-Westphalia_Regierungsbezirk Detmold", "Germany_North Rhine-Westphalia_Regierungsbezirk Dusseldorf_Dusseldorf", "Germany_North Rhine-Westphalia_Regierungsbezirk Dusseldorf_Mulheim", "Germany_North Rhine-Westphalia_Regierungsbezirk Dusseldorf_Wesel", "Germany_North Rhine-Westphalia_Regierungsbezirk Koln_Aachen", "Germany_North Rhine-Westphalia_Regierungsbezirk Koln_Koln", "Germany_North Rhine-Westphalia_Regierungsbezirk Munster_Munster", "Germany_North Rhine-Westphalia_Regierungsbezirk Munster_Recklinghausen"),
        "europe_germany_nordrhein_westfalen_arnsberg_regbez" to listOf("Germany_North Rhine-Westphalia_Regierungsbezirk Arnsberg_Arnsberg", "Germany_North Rhine-Westphalia_Regierungsbezirk Arnsberg_Dortmund"),
        "europe_germany_nordrhein_westfalen_detmold_regbez" to listOf("Germany_North Rhine-Westphalia_Regierungsbezirk Detmold"),
        "europe_germany_nordrhein_westfalen_duesseldorf_regbez" to listOf("Germany_North Rhine-Westphalia_Regierungsbezirk Dusseldorf_Dusseldorf", "Germany_North Rhine-Westphalia_Regierungsbezirk Dusseldorf_Mulheim", "Germany_North Rhine-Westphalia_Regierungsbezirk Dusseldorf_Wesel"),
        "europe_germany_nordrhein_westfalen_koeln_regbez" to listOf("Germany_North Rhine-Westphalia_Regierungsbezirk Koln_Aachen", "Germany_North Rhine-Westphalia_Regierungsbezirk Koln_Koln"),
        "europe_germany_nordrhein_westfalen_muenster_regbez" to listOf("Germany_North Rhine-Westphalia_Regierungsbezirk Munster_Munster", "Germany_North Rhine-Westphalia_Regierungsbezirk Munster_Recklinghausen"),
        "europe_germany_rheinland_pfalz" to listOf("Germany_Rhineland-Palatinate_Koblenz", "Germany_Rhineland-Palatinate_South"),
        "europe_germany_saarland" to listOf("Germany_Saarland"),
        "europe_germany_sachsen" to listOf("Germany_Saxony_Dresden", "Germany_Saxony_Leipzig"),
        "europe_germany_sachsen_anhalt" to listOf("Germany_Saxony-Anhalt_Halle", "Germany_Saxony-Anhalt_Magdeburg"),
        "europe_germany_schleswig_holstein" to listOf("Germany_Schleswig-Holstein_Flensburg", "Germany_Schleswig-Holstein_Kiel"),
        "europe_germany_thueringen" to listOf("Germany_Thuringia")
    )

    fun normalizeBakeId(bakeId: String): String =
        bakeId.trim().lowercase().replace('-', '_')

    /** Leaf ids for a catalog bake id, or null when this table has no row. */
    fun leafIdsForBakeId(bakeId: String): List<String>? {
        val key = normalizeBakeId(bakeId)
        val hit = bakeIdToLeaves[key] ?: return null
        return if (hit.isEmpty()) null else hit
    }

    fun leafIdsForGeofabrikPath(geofabrikPath: String): List<String>? {
        val base =
            geofabrikPath
                .trim()
                .trim('/')
                .lowercase()
                .replace('/', '_')
                .replace('-', '_')
        return leafIdsForBakeId(base)
    }
}
