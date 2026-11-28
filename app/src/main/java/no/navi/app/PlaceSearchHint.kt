package no.navi.app

import android.content.Context
import uniffi.navi.mwmInstalled

/**
 * Offline place search on MWM builds uses CoMaps over installed `.mwm` leaves;
 * missing `place_index.db` is expected and must not block the search UI.
 */
fun offlineMwmPlaceSearchReady(
    mwmClientEnabled: Boolean,
    comapsSearchEnabled: Boolean,
    countryMapsExposed: Boolean,
    mwmLeafIds: Collection<String>,
): Boolean =
    mwmClientEnabled &&
        comapsSearchEnabled &&
        (countryMapsExposed || mwmLeafIds.isNotEmpty())

fun offlineMwmPlaceSearchReady(context: Context): Boolean {
    val dataDir = NaviAppData.resolve(context).absolutePath
    val leaves = runCatching { mwmInstalled(dataDir).leafIds }.getOrDefault(emptyList())
    return offlineMwmPlaceSearchReady(
        MwmClientBootstrap.isEnabled(),
        ComapsSearchBridge.enabled(),
        ComapsSessionBootstrap.countryMapsExposed(context),
        leaves,
    )
}

/** Strip FTS file problems from search hints when CoMaps/MWM search is the active backend. */
fun ftsPlaceIndexProblemForSearchUi(
    placeIndexProblem: String?,
    offlineMwmReady: Boolean,
): String? {
    if (placeIndexProblem == null || !offlineMwmReady) return placeIndexProblem
    return when (placeIndexProblem) {
        "pack volume unavailable" -> placeIndexProblem
        else -> null
    }
}

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
    offlineMwmReady: Boolean = false,
): String? {
    if (indexProblem != null) {
        return if (onlineAvailable || !hitsEmpty) {
            "Offline place search unavailable ($indexProblem) — showing online results (Nominatim)."
        } else {
            "Offline place search unavailable ($indexProblem) — enter coordinates or tap the map"
        }
    }
    if (!hitsEmpty) return null
    return when {
        onlineAvailable && !indexHasEntries && offlineMwmReady ->
            "No local name match in downloaded maps — showing online results (Nominatim)."
        onlineAvailable && !indexHasEntries ->
            "No offline search data yet — showing online results (Nominatim)."
        onlineAvailable && indexHasEntries ->
            "No local match — showing online results (Nominatim)."
        offlineMwmReady && indexHasEntries ->
            "No local name match in downloaded maps. Turn on Online place search in Map settings, " +
                "or download the region that contains this place."
        offlineMwmReady ->
            "No local name match in downloaded maps — turn on Online place search in Map settings, " +
                "or enter coordinates / tap the map"
        indexHasEntries ->
            "No offline match. Turn on Online place search in Map settings, " +
                "or download the region that contains this place."
        indexRunning ->
            "Offline search is still preparing — try coordinates, map tap, or wait for Wi‑Fi search"
        else ->
            "No offline search data on device — turn on Online place search in Map settings, " +
                "or enter coordinates / tap the map"
    }
}

/**
 * Local-only list where no hit matches the whole query after folding: the
 * prefix and partial hits stay listed below this line (`Köln` over
 * `Kolnesholmane`). While the query is still being typed ([submitted] false)
 * and a hit continues it (`Domb` and `Dombås`), there is no hint.
 */
fun placeSearchNoWholeMatchHint(
    query: String,
    hits: List<uniffi.navi.PlaceHit>,
    onlineEnabled: Boolean,
    usedOnline: Boolean,
    submitted: Boolean = true,
): String? {
    if (usedOnline || hits.isEmpty()) return null
    if (hits.any { placeHitMatchesWholeQuery(it, query) }) return null
    if (!submitted && hits.any { placeHitMatchesQueryPrefix(it, query) }) return null
    return if (onlineEnabled) {
        "No offline match."
    } else {
        "No offline match. Turn on Online place search in Map settings, " +
            "or download the region that contains this place."
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
