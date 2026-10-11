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
    indexProblem: String? = null,
): String? {
    if (indexProblem != null) {
        return if (onlineAvailable || !hitsEmpty) {
            "Offline place search unavailable (place index $indexProblem) — showing online results (Nominatim)."
        } else {
            "Offline place search unavailable (place index $indexProblem) — enter coordinates or tap the map"
        }
    }
    if (!hitsEmpty) return null
    return when {
        onlineAvailable && !indexHasEntries ->
            "No offline place index yet — showing online results (Nominatim)."
        onlineAvailable && indexHasEntries ->
            "No local match — showing online results (Nominatim)."
        indexHasEntries ->
            "No offline match. Turn on Online place search in Map settings, " +
                "or download the region that contains this place."
        indexRunning ->
            "Place index is still building — try coordinates, map tap, or wait for Wi‑Fi search"
        else ->
            "No place index on device — turn on Online place search in Map settings, " +
                "or enter coordinates / tap the map"
    }
}

/**
 * Offline-only: a local hit exists, but it is a minor place. Point at online
 * search or downloading another region (København farm vs the Danish city).
 */
fun placeSearchOfflineMoreHint(
    onlineEnabled: Boolean,
    topKind: String?,
): String? {
    if (onlineEnabled || topKind.isNullOrBlank()) return null
    if (!isMinorPlaceKind(topKind)) return null
    return "Turn on Online place search in Map settings, " +
        "or download the region that contains this place."
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
