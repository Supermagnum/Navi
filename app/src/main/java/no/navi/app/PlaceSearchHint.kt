package no.navi.app

/**
 * Place-search copy when the FTS index is empty or still building.
 * Distinct from a genuine zero-hit query on a populated index.
 */
@Suppress("UNUSED_PARAMETER")
fun placeSearchBuildingMessage(
    hitsEmpty: Boolean,
    indexHasEntries: Boolean,
    indexRunning: Boolean,
): String? {
    if (!hitsEmpty || indexHasEntries) return null
    return "Place index is still building — try coordinates or map tap for now"
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
