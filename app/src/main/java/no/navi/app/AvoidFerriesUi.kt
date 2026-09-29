package no.navi.app

/**
 * Avoid-ferries toggle availability from the last plan's loaded graph.
 *
 * Plan reports include `graph_ferry_edges=N` (pack or cold PBF). When N is 0 the
 * toggle is greyed out with an honest explanation — packs today often bake with
 * no ferry edges, so avoid_ferries would otherwise be a silent no-op.
 *
 * Recomputed on every successful plan (and cleared when the route is cleared);
 * not a one-time app-start check.
 *
 * The user's ON/OFF preference is separate: it lives in ConfigStore
 * (`app_config.avoid_ferries`) and must survive process death even while the
 * toggle is greyed out, so a later ferry-capable plan still honors it.
 */
object AvoidFerriesUi {
    const val NO_FERRY_DATA_NOTE = "No ferry crossings in this area's map data"

    fun parseGraphFerryEdges(report: String): Int? {
        val re = Regex("""(?:^|[;\s])graph_ferry_edges=(\d+)""")
        return re
            .find(report)
            ?.groupValues
            ?.getOrNull(1)
            ?.toIntOrNull()
    }

    /**
     * @param profileAllowsToggle profiles that historically expose Avoid ferries
     *   (motor), plus hiking/bike when the loaded graph actually has ferries.
     * @param graphFerryEdges null = no plan yet; motor keeps prior enablement,
     *   hiking/bike stay off until a plan reports ferry edges.
     */
    fun toggleEnabled(
        profileIsMotor: Boolean,
        profileIsHikingOrBike: Boolean,
        graphFerryEdges: Int?,
    ): Boolean {
        if (graphFerryEdges != null) {
            if (graphFerryEdges <= 0) return false
            return profileIsMotor || profileIsHikingOrBike
        }
        // No plan yet: motor can set the preference ahead of plan; hiking/bike
        // wait for graph evidence (pack-first hiking has no ferries today).
        return profileIsMotor
    }

    fun unavailableNote(graphFerryEdges: Int?): String? =
        if (graphFerryEdges != null && graphFerryEdges <= 0) {
            NO_FERRY_DATA_NOTE
        } else {
            null
        }
}
