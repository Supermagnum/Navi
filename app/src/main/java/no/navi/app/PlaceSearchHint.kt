package no.navi.app

/**
 * Place-search copy when the FTS index is empty or still building.
 * Distinct from a genuine zero-hit query on a populated index.
 */
fun placeSearchBuildingMessage(
    hitsEmpty: Boolean,
    indexHasEntries: Boolean,
    indexRunning: Boolean,
    onlineAvailable: Boolean = false,
): String? {
    if (!hitsEmpty) return null
    return when {
        onlineAvailable && !indexHasEntries ->
            "No offline place index yet — showing online results (Nominatim). " +
                "Selecting a hit also sets the Tools download region."
        onlineAvailable && indexHasEntries ->
            "No local match — showing online results (Nominatim). " +
                "Selecting a hit sets the Tools download region for that place."
        indexHasEntries -> null
        indexRunning ->
            "Place index is still building — try coordinates, map tap, or wait for Wi‑Fi search"
        else ->
            "No place index on device — connect to the network to search by name/address, " +
                "or enter coordinates / tap the map"
    }
}

/**
 * Skip GPS-triggered bbox graph work (speed-limit cone, road-near) while a
 * foreground plan owns the PBF. One missed HUD update is cheaper than
 * stretching a user-initiated plan; the next fix after leave rebuilds.
 */
fun skipLiveGraphWorkDuringForegroundPlan(foregroundPlanActive: Boolean): Boolean = foregroundPlanActive

/** Keep the plan progress bar from moving backwards during one plan. */
fun monotonicPlanPercent(
    previous: Int,
    incoming: Int?,
): Int {
    if (incoming == null || incoming < 0) return previous
    return incoming.coerceAtLeast(previous.coerceAtLeast(0))
}
