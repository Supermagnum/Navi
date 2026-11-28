package no.navi.app

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.navi.FfiNearbyServiceHit
import uniffi.navi.PlaceHit

/**
 * Owns Nearby-services state, IO search, GPS refresh, and the results sheet.
 * Kept out of [NaviMapScreen] to stay under the JVM method-size limit.
 */
@Composable
fun NearbyServicesHost(
    visible: Boolean,
    onVisibleChange: (Boolean) -> Unit,
    gpsLat: Double,
    gpsLon: Double,
    placeIndexDbPath: String,
    hideChrome: Boolean,
    onSelectPlace: (PlaceHit) -> Unit,
    onRouteHere: (PlaceHit) -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var busy by remember { mutableStateOf(false) }
    var hits by remember { mutableStateOf<List<FfiNearbyServiceHit>>(emptyList()) }
    var categories by remember { mutableStateOf(allNearbyServiceCategories()) }
    var gap by remember { mutableStateOf("") }
    var queryMs by remember { mutableStateOf(0L) }
    var searchLat by remember { mutableStateOf(Double.NaN) }
    var searchLon by remember { mutableStateOf(Double.NaN) }
    var searchJob by remember { mutableStateOf<Job?>(null) }

    fun runSearch(
        force: Boolean,
        label: String = "gps",
        overrideLat: Double? = null,
        overrideLon: Double? = null,
    ) {
        val held = NaviMapTestHooks.heldDebugGpsLatLon
        val lat = overrideLat ?: gpsLat.takeUnless { it == 0.0 && gpsLon == 0.0 } ?: held?.first ?: gpsLat
        val lon = overrideLon ?: gpsLon.takeUnless { gpsLat == 0.0 && it == 0.0 } ?: held?.second ?: gpsLon
        val hasGps = lat.isFinite() && lon.isFinite() && (lat != 0.0 || lon != 0.0)
        if (!hasGps) {
            android.util.Log.w("NearbyServices", "search skipped: no GPS label=$label")
            hits = emptyList()
            gap = ""
            queryMs = 0L
            busy = false
            return
        }
        if (!force && searchLat.isFinite() && searchLon.isFinite()) {
            val moved = haversineMApprox(searchLat, searchLon, lat, lon)
            if (moved < NEARBY_SERVICES_REFRESH_MOVE_M && hits.isNotEmpty()) {
                return
            }
        }
        searchJob?.cancel()
        busy = true
        val cats = categories.toList()
        val dbPath = placeIndexDbPath
        android.util.Log.i(
            "NearbyServices",
            "search start label=$label lat=$lat lon=$lon db=$dbPath",
        )
        searchJob =
            scope.launch {
                val outcome =
                    withContext(Dispatchers.IO) {
                        val installed =
                            PackRegionAvailability
                                .installedPackRegionIds(
                                    InstalledMaps
                                        .current()
                                        ?.regions
                                        ?.values
                                        .orEmpty(),
                                ).toList()
                        android.util.Log.i(
                            "NearbyServices",
                            "search installed=${installed.joinToString("|").ifEmpty { "(none)" }}",
                        )
                        runCatching {
                            queryNearbyServices(
                                indexDbPath = dbPath,
                                lat = lat,
                                lon = lon,
                                categories = cats,
                                installedRegions = installed,
                            )
                        }
                    }
                if (!isActive) return@launch
                outcome
                    .onSuccess { res ->
                        hits = res.hits
                        gap = nearbyServicesGapMessage(res.missingRegions, context.resources)
                        queryMs = res.queryTimeMs.toLong()
                        searchLat = lat
                        searchLon = lon
                        if (gap.isNotEmpty()) {
                            android.util.Log.i("NearbyServices", "GAP label=$label msg=$gap")
                        }
                        logNearbyServicesResult(label = label, lat = lat, lon = lon, result = res)
                    }.onFailure { err ->
                        android.util.Log.e(
                            "NearbyServices",
                            "search failed label=$label: ${err.message}",
                            err,
                        )
                        hits = emptyList()
                        gap = ""
                        queryMs = 0L
                    }
                busy = false
            }
    }

    LaunchedEffect(Unit) {
        while (true) {
            delay(250)
            if (NaviMapTestHooks.requestNearbyServicesTest) {
                NaviMapTestHooks.requestNearbyServicesTest = false
                onVisibleChange(true)
                var waited = 0
                while (waited < 60) {
                    val held = NaviMapTestHooks.heldDebugGpsLatLon
                    val ready =
                        (gpsLat.isFinite() && gpsLon.isFinite() && (gpsLat != 0.0 || gpsLon != 0.0)) ||
                            held != null
                    if (ready) break
                    delay(250)
                    waited++
                }
                val held = NaviMapTestHooks.heldDebugGpsLatLon
                runSearch(
                    force = true,
                    label = "harness",
                    overrideLat = held?.first,
                    overrideLon = held?.second,
                )
            }
        }
    }

    LaunchedEffect(visible, categories) {
        if (visible) runSearch(force = true)
    }

    LaunchedEffect(visible, gpsLat, gpsLon) {
        if (visible) runSearch(force = false)
    }

    if (!hideChrome && visible) {
        val hasGps =
            gpsLat.isFinite() && gpsLon.isFinite() && (gpsLat != 0.0 || gpsLon != 0.0)
        NearbyServicesSheet(
            hits = hits,
            selected = categories,
            onToggleCategory = { cat ->
                categories =
                    categories.toMutableSet().also { set ->
                        if (!set.add(cat)) {
                            set.remove(cat)
                        }
                        if (set.isEmpty()) {
                            set.addAll(allNearbyServiceCategories())
                        }
                    }
            },
            missingRegionsMessage = gap,
            noGpsMessage =
                if (hasGps) {
                    null
                } else {
                    context.getString(R.string.nearby_services_no_gps)
                },
            busy = busy,
            queryTimeMs = queryMs,
            onClose = { onVisibleChange(false) },
            onSelect = { hit ->
                onSelectPlace(nearbyHitToPlaceHit(hit))
                onVisibleChange(false)
            },
            onRouteHere = { hit ->
                onRouteHere(nearbyHitToPlaceHit(hit))
                onVisibleChange(false)
            },
            modifier = modifier,
        )
    }
}

fun nearbyHitToPlaceHit(hit: FfiNearbyServiceHit): PlaceHit =
    PlaceHit(
        osmId = hit.osmId,
        name = hit.name,
        kind = hit.kind,
        lat = hit.lat,
        lon = hit.lon,
        subArea = hit.subArea,
        municipality = hit.municipality,
        regionId = hit.regionId,
    )
