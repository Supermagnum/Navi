package no.navi.app

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.FilterChip
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.zIndex
import androidx.core.content.ContextCompat
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import org.maplibre.android.MapLibre
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.geometry.LatLngBounds
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.LineLayer
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.SymbolLayer
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.geojson.Feature
import org.maplibre.geojson.FeatureCollection
import org.maplibre.geojson.LineString
import org.maplibre.geojson.Point
import uniffi.navi.FfiCarRestSettings
import uniffi.navi.FfiSavedPlace
import uniffi.navi.FfiSavedRoute
import uniffi.navi.FfiTruckRestSettings
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.PlaceHit
import uniffi.navi.TravelProfile
import uniffi.navi.applyOsmUpdate
import uniffi.navi.cancelInFlightPlan
import uniffi.navi.checkOsmUpdates
import uniffi.navi.deleteSavedPlace
import uniffi.navi.deleteSavedRoute
import uniffi.navi.downloadProgressClear
import uniffi.navi.downloadProgressSnapshot
import uniffi.navi.ecoModeDefault
import uniffi.navi.ecoModeToggleable
import uniffi.navi.elevationAt
import uniffi.navi.ensurePlaceIndex
import uniffi.navi.foregroundPlanActive
import uniffi.navi.foregroundPlanEnter
import uniffi.navi.foregroundPlanLeave
import uniffi.navi.formatRouteAvoidanceReport
import uniffi.navi.geofabrikLatestPbfUrl
import uniffi.navi.indexedMapsStatus
import uniffi.navi.listSavedPlaces
import uniffi.navi.listSavedRoutes
import uniffi.navi.loadCarRestSettings
import uniffi.navi.loadTruckRestSettings
import uniffi.navi.loadVehicleLimits
import uniffi.navi.nearbyPlaces
import uniffi.navi.osmWeeklyReminderDue
import uniffi.navi.placeIndexHasEntries
import uniffi.navi.planProgressClear
import uniffi.navi.planProgressSnapshot
import uniffi.navi.pmtilesCancelJob
import uniffi.navi.pmtilesDefaultBaseUrl
import uniffi.navi.pmtilesGetJob
import uniffi.navi.pmtilesPauseJob
import uniffi.navi.pmtilesQueueRegion
import uniffi.navi.pmtilesResumeJob
import uniffi.navi.pmtilesRunJob
import uniffi.navi.renameSavedPlace
import uniffi.navi.resolveSpeedLimitKmh
import uniffi.navi.roadLabelNear
import uniffi.navi.roadNearInfo
import uniffi.navi.saveCarRestSettings
import uniffi.navi.saveNamedPlace
import uniffi.navi.saveNamedRoute
import uniffi.navi.saveTruckRestSettings
import uniffi.navi.saveVehicleLimits
import uniffi.navi.searchPlaces
import uniffi.navi.setOsmWeeklyReminder
import uniffi.navi.travelProfileMenuFocus
import uniffi.navi.updateGpsFix
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.roundToInt
import android.graphics.Paint as AndroidPaint

/** Max time the cold-start splash may stay up on a normal launch (not capture hold). */
private const val SPLASH_MAX_HOLD_MS = 2_000L

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        // Splash Screen API (androidx.core:core-splashscreen) for API 26–30 compat
        // and native API 31+. Activity theme is Theme.Navi.Splash; post-splash
        // switches to Theme.Navi. Distinct from launcher mipmaps and any notify icon.
        // adb hold for capture: --ez navi_keep_splash true
        // Normal launches: dismiss as soon as the first frame draws, and never hold
        // the splash longer than 2s (heavy map init is deferred past that frame).
        val keepSplashForCapture =
            AtomicBoolean(intent?.getBooleanExtra("navi_keep_splash", false) == true)
        val splashStartedAtMs = SystemClock.uptimeMillis()
        val firstFrameDrawn = AtomicBoolean(false)
        installSplashScreen().setKeepOnScreenCondition {
            if (keepSplashForCapture.get()) return@setKeepOnScreenCondition true
            if (firstFrameDrawn.get()) return@setKeepOnScreenCondition false
            SystemClock.uptimeMillis() - splashStartedAtMs < SPLASH_MAX_HOLD_MS
        }
        super.onCreate(savedInstanceState)
        applyNaviLaunchExtras(intent)
        runCatching { uniffi.navi.initNativeLogging() }
        setContent {
            var showMap by remember { mutableStateOf(false) }
            MaterialTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    // Cheap first frame so the splash can exit within SPLASH_MAX_HOLD_MS
                    // even while MapLibre / NaviMapScreen still warm up.
                    if (showMap) {
                        NaviMapScreen()
                    }
                }
            }
            SideEffect {
                firstFrameDrawn.set(true)
            }
            LaunchedEffect(Unit) {
                MapLibre.getInstance(this@MainActivity)
                showMap = true
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        // singleTask: am start with camera extras must apply without process restart.
        applyNaviLaunchExtras(intent)
    }

    /**
     * adb camera framing:
     *   am start -n no.navi.app/.MainActivity \
     *     --ed navi_camera_lat 62.1592913 --ed navi_camera_lon 11.3584086 --ed navi_camera_zoom 16
     * Also accepts --ef / --es for the same keys.
     */
    private fun applyNaviLaunchExtras(intent: Intent?) {
        if (intent == null) return
        if (intent.getBooleanExtra("navi_force_online_basemap", false)) {
            NaviMapTestHooks.forceOnlineBasemap = true
        }
        if (intent.getBooleanExtra("navi_hide_chrome", false)) {
            NaviMapTestHooks.hideUiChrome = true
            NaviMapTestHooks.hideSearchChrome = true
        }
        val camLat = intentDoubleExtra(intent, "navi_camera_lat")
        val camLon = intentDoubleExtra(intent, "navi_camera_lon")
        val camZoom = intentDoubleExtra(intent, "navi_camera_zoom")
        if (!camLat.isNaN() && !camLon.isNaN() && !camZoom.isNaN()) {
            android.util.Log.i(
                "NaviCamera",
                "pendingCamera lat=$camLat lon=$camLon zoom=$camZoom",
            )
            NaviMapTestHooks.disableGpsFollow = true
            NaviMapTestHooks.followGps = false
            NaviMapTestHooks.pendingCamera = Triple(camLat, camLon, camZoom)
        }
    }

    private fun intentDoubleExtra(
        intent: Intent,
        key: String,
    ): Double {
        val extras = intent.extras ?: return Double.NaN
        if (!extras.containsKey(key)) return Double.NaN
        when (val v = extras.get(key)) {
            is Double -> return v
            is Float -> return v.toDouble()
            is Int -> return v.toDouble()
            is Long -> return v.toDouble()
            is String -> return v.toDoubleOrNull() ?: Double.NaN
            is Number -> return v.toDouble()
        }
        val asDouble = extras.getDouble(key, Double.NaN)
        if (!asDouble.isNaN()) return asDouble
        val asFloat = extras.getFloat(key, Float.NaN)
        if (!asFloat.isNaN()) return asFloat.toDouble()
        return Double.NaN
    }
}

data class BreakPoiMark(
    val name: String,
    val lat: Double,
    val lon: Double,
    val kind: String = "hut",
)

data class MapRouteState(
    val polyline: String = "",
    /** Segment JSON from plan (`on_trail` / `off_trail` polylines); empty = solid only. */
    val routeSegmentsJson: String = "[]",
    val poiLat: Double = 0.0,
    val poiLon: Double = 0.0,
    val poiName: String = "",
    val poiIconPng: ByteArray = ByteArray(0),
    /** Route endpoint labels drawn on the map (start / via / end). */
    val startName: String = "",
    val startLat: Double = 0.0,
    val startLon: Double = 0.0,
    val viaName: String = "",
    val viaLat: Double = 0.0,
    val viaLon: Double = 0.0,
    /** Extra vias when more than one (Harlandshytta, Eldåbu, …). */
    val viaPoints: List<Waypoint> = emptyList(),
    val endName: String = "",
    val endLat: Double = 0.0,
    val endLon: Double = 0.0,
    /** Pause / overnight labels along the corridor (hut or tent fallback). */
    val breakPois: List<BreakPoiMark> = emptyList(),
    /** Multi-day day cards from plan `daysJson` (empty when single-day). */
    val multiDayCards: List<MultiDayCard> = emptyList(),
    /** Live GPS / last known position (0,0 = unknown). */
    val gpsLat: Double = 0.0,
    val gpsLon: Double = 0.0,
    val gpsAccuracyM: Float? = null,
    /**
     * When true (default), the camera centers on live GPS for zoom / GPS updates.
     * Manual pan sets this false until the user taps Recenter.
     */
    val followGps: Boolean = true,
    val cameraLat: Double? = null,
    val cameraLon: Double? = null,
    val cameraZoom: Double? = null,
    /** Map bearing degrees clockwise from north; used with rotation modes. */
    val cameraBearing: Double = 0.0,
    val tracks: List<TrackMarker> = emptyList(),
    val layerEpoch: Int = 0,
)

data class Waypoint(
    val name: String = "",
    val lat: Double = 0.0,
    val lon: Double = 0.0,
    /** Street name without house number (optional). */
    val street: String? = null,
    val houseNumber: String? = null,
    val postcode: String? = null,
)

private enum class SearchTarget { From, To, Via }

private enum class SearchMode { Place, Address }

/**
 * Pre-departure trip ETA in minutes for the HUD (before live GPS speed exists).
 *
 * Motor profiles use the FFI per-edge `maxspeed` / highway-fallback estimate.
 * Hiking uses the hiking planner's fixed 16 min/km. Cycling uses ~4 min/km
 * on the bicycle bbox graph from [planCarRoute].
 */
private fun preDepartureEtaMinutes(
    profile: TravelProfile,
    result: uniffi.navi.CorridorRouteResult,
): Double {
    if (result.distanceKm <= 0.0) return 0.0
    return when (profile) {
        TravelProfile.HIKING ->
            if (result.etaMinutes > 0.0) result.etaMinutes else result.distanceKm * 16.0
        TravelProfile.BICYCLE, TravelProfile.BICYCLE_ELECTRIC -> result.distanceKm * 4.0
        else ->
            if (result.etaMinutes > 0.0) result.etaMinutes else (result.distanceKm / 50.0) * 60.0
    }
}

/** Prefer battery / climb lines from the plan report when present. */
private fun formatEbikePlanStatus(
    report: String,
    distanceKm: Double,
    unitSystem: UnitSystem,
): String? {
    if (!report.contains("ebike_pct_of_capacity=") && !report.contains("ev_pct_of_capacity=")) {
        return null
    }
    val pct =
        Regex("""(?:ebike|ev)_pct_of_capacity=([0-9.]+)""")
            .find(report)
            ?.groupValues
            ?.getOrNull(1)
    val base =
        buildString {
            append(DisplayUnits.formatRoutePlanned(distanceKm, unitSystem))
            if (pct != null) {
                append(" · ~${pct.toDoubleOrNull()?.toInt() ?: pct}% of battery")
            }
        }
    val warn = report.lineSequence().firstOrNull { it.startsWith("WARNING:") }
    return if (warn != null) "$base\n${warn.take(160)}" else base
}

/**
 * When indexed packs are missing/stale, every plan rebuilds from the PBF
 * (~tens of seconds). Surface that so "extreme slowness" is actionable.
 */
private fun indexedPackMissHint(report: String): String? {
    if (!report.contains("pack_hit=false")) return null
    return "Slow plan: indexed maps not ready (PBF fallback). " +
        "Wait for background indexing, or Tools → Rebuild indexed maps."
}

private fun withIndexedPackMissHint(
    statusLine: String,
    report: String,
): String {
    val hint = indexedPackMissHint(report) ?: return statusLine
    return if (statusLine.isBlank()) hint else "$statusLine\n$hint"
}

private fun planReportIsCancelled(report: String): Boolean =
    report.contains("cancelled=true") ||
        report.lineSequence().any { it.trim() == "FAIL: cancelled" }

private fun cancelledCorridorResult(): uniffi.navi.CorridorRouteResult =
    uniffi.navi.CorridorRouteResult(
        report = "FAIL: cancelled\ncancelled=true\n",
        distanceKm = 0.0,
        etaMinutes = 0.0,
        cacheHit = false,
        coldBuildS = 0.0,
        warmLoadS = 0.0,
        routePolyline = "",
        poiLat = 0.0,
        poiLon = 0.0,
        poiName = "",
        poiIconKey = "",
        breakPoisJson = "[]",
        daysJson = "[]",
        simSamplesJson = "[]",
        maneuversJson = "[]",
        priorityPathSharePct = 0.0,
        routeSegmentsJson = "[]",
        offTrailAdvisory = "",
    )

private fun userFacingStatus(raw: String): String {
    val t = raw.trim()
    if (t.isEmpty()) return ""
    if (OsmUpdateUserCopy.looksTechnical(t)) {
        return OsmUpdateUserCopy.sanitize(t)
    }
    if (t.contains("TEST_KIND=") || t.contains("detected_cores=") || t.contains("DATA_SOURCE=")) {
        return when {
            planReportIsCancelled(t) -> "Planning cancelled"
            t.contains("PASS") && t.contains("distance_km=") -> {
                val km = Regex("""distance_km=([0-9.]+)""").find(t)?.groupValues?.getOrNull(1)
                val base =
                    if (km != null) "Route planned · $km km" else "Route planned"
                withIndexedPackMissHint(base, t)
            }
            t.contains("PASS") -> withIndexedPackMissHint("Done", t)
            t.lineSequence().any { it.startsWith("FAIL") } ->
                t
                    .lineSequence()
                    .first { it.startsWith("FAIL") }
                    .removePrefix("FAIL:")
                    .trim()
                    .take(100)
            else -> ""
        }
    }
    return t.take(120)
}

@Composable
private fun NaviMapScreen() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var status by remember { mutableStateOf("Ready") }
    var mapState by remember {
        mutableStateOf(
            NaviMapTestHooks.pendingCamera?.let { cam ->
                NaviMapTestHooks.pendingCamera = null
                NaviMapTestHooks.disableGpsFollow = true
                NaviMapTestHooks.followGps = false
                MapRouteState(
                    followGps = false,
                    cameraLat = cam.first,
                    cameraLon = cam.second,
                    cameraZoom = cam.third,
                    layerEpoch = 1,
                )
            } ?: MapRouteState(),
        )
    }
    var mapLayerCount by remember { mutableIntStateOf(0) }
    var profile by remember { mutableStateOf(TravelProfile.CAR) }
    var ecoEnabled by remember { mutableStateOf(ecoModeDefault(TravelProfile.CAR)) }
    var searchMode by remember { mutableStateOf(SearchMode.Place) }
    var searchTarget by remember { mutableStateOf(SearchTarget.To) }
    var query by remember { mutableStateOf("") }
    var hits by remember { mutableStateOf<List<PlaceHit>>(emptyList()) }
    var searchBusy by remember { mutableStateOf(false) }
    var searchIndexHint by remember { mutableStateOf("") }
    var showTools by remember { mutableStateOf(false) }
    var diagnosticLogging by remember {
        mutableStateOf(MapHudPrefs.loadDiagnosticLogging(context))
    }
    var downloadScopeCountry by remember { mutableStateOf(false) }
    var selectedGeofabrikPath by remember {
        mutableStateOf(MapHudPrefs.loadGeofabrikPath(context))
    }
    var downloadContinent by remember {
        mutableStateOf(GeofabrikDownloadCatalog.continentForPath(selectedGeofabrikPath))
    }
    LaunchedEffect(selectedGeofabrikPath) {
        NaviMapTestHooks.lastSelectedGeofabrikPath = selectedGeofabrikPath
    }
    LaunchedEffect(Unit) {
        DiagnosticLog.restoreFromPrefs(context)
        diagnosticLogging = DiagnosticLog.isEnabled()
        while (true) {
            val pendingPath = NaviMapTestHooks.pendingGeofabrikPath
            if (pendingPath != null) {
                NaviMapTestHooks.pendingGeofabrikPath = null
                selectedGeofabrikPath = pendingPath
                downloadContinent = GeofabrikDownloadCatalog.continentForPath(pendingPath)
                // Country-root paths keep Country scope; landsdels / freeform → Region.
                val country = GeofabrikDownloadCatalog.findByPath(pendingPath)
                downloadScopeCountry =
                    country != null &&
                    country.path == pendingPath.trim().trim('/')
            }
            kotlinx.coroutines.delay(200)
        }
    }
    var searchJob by remember { mutableStateOf<Job?>(null) }
    var toPoint by remember { mutableStateOf(Waypoint()) }
    var viaPoints by remember { mutableStateOf<List<Waypoint>>(emptyList()) }
    var fromPoint by remember { mutableStateOf<Waypoint?>(null) }
    var avoidMotorways by remember { mutableStateOf(false) }
    var avoidTolls by remember { mutableStateOf(false) }
    var avoidFerries by remember { mutableStateOf(false) }
    var preferOfficialNetworks by remember { mutableStateOf(false) }
    var preferPilgrimRoutes by remember { mutableStateOf(false) }
    var useNetworkedCabins by remember { mutableStateOf(false) }
    var bikeCapability by remember { mutableStateOf("trekking") }
    var networkHutMember by remember { mutableStateOf(false) }

    /** Sticky manual bearing when snap-back is off; cleared by mode chip. */
    var manualRotationSticky by remember { mutableStateOf(false) }

    /** Bumped to force MapLibre bearing re-apply even when degrees unchanged. */
    var bearingApplyEpoch by remember { mutableIntStateOf(0) }
    val rotationSnapHandler = remember { android.os.Handler(android.os.Looper.getMainLooper()) }
    var pendingRotationSnap by remember { mutableStateOf<Runnable?>(null) }
    var prioritySharePct by remember { mutableDoubleStateOf(0.0) }
    var savedRoutes by remember { mutableStateOf<List<FfiSavedRoute>>(emptyList()) }
    var axleKg by remember { mutableStateOf("") }
    var bogieKg by remember { mutableStateOf("") }
    var heightM by remember { mutableStateOf("") }
    var widthM by remember { mutableStateOf("") }
    var lengthM by remember { mutableStateOf("") }
    var showProfilePanel by remember { mutableStateOf(true) }
    var showVehiclePanel by remember { mutableStateOf(true) }
    var showRoutesPanel by remember { mutableStateOf(true) }
    var showPlacesPanel by remember { mutableStateOf(true) }
    var savedPlaces by remember { mutableStateOf<List<FfiSavedPlace>>(emptyList()) }
    var mapMarkPending by remember { mutableStateOf<MapMarkPending?>(null) }
    var savePlaceDraftName by remember { mutableStateOf("") }
    var showSavePlaceDialog by remember { mutableStateOf(false) }
    var renamePlaceId by remember { mutableStateOf<String?>(null) }
    var renamePlaceDraft by remember { mutableStateOf("") }
    var approachGuidance by remember { mutableStateOf(ApproachGuidanceState()) }
    var speedCameraOptIn by remember { mutableStateOf(MapHudPrefs.loadSpeedCameraOptIn(context)) }
    var showSpeedCameraPrompt by remember { mutableStateOf(false) }
    var speedCamerasJson by remember { mutableStateOf("[]") }
    var speedCameraWarning by remember { mutableStateOf(SpeedCameraWarningState()) }
    var roadSignsJson by remember { mutableStateOf("[]") }
    var schoolPoisJson by remember { mutableStateOf("[]") }
    var routeSchoolPoisJson by remember { mutableStateOf("[]") }
    var roadSignWarning by remember { mutableStateOf(RoadSignWarningState()) }
    var hideChrome by remember { mutableStateOf(false) }
    var hideSearch by remember { mutableStateOf(false) }
    var regionDownloadProgress by remember { mutableStateOf("") }
    var downloadPolling by remember { mutableStateOf(false) }
    var indexedMapsUiLine by remember { mutableStateOf("") }
    var placeIndexUiLine by remember { mutableStateOf("") }
    var planningRoute by remember { mutableStateOf(false) }
    var routePlanProgress by remember { mutableStateOf("") }

    /** True while planning when indexed packs are not ready (slow PBF path). */
    var planIndexingHintVisible by remember { mutableStateOf(false) }
    var recalculatingRoute by remember { mutableStateOf(false) }
    var showHikingReroutePrompt by remember { mutableStateOf(false) }
    var missingCoveragePrompt by remember { mutableStateOf<MissingRegionCoverage?>(null) }
    var rerouteJob by remember { mutableStateOf<Job?>(null) }
    val planAbort =
        remember {
            java.util.concurrent.atomic
                .AtomicBoolean(false)
        }
    val offRouteCoordinator = remember { OffRouteCoordinator() }
    var routePlanPct by remember { mutableIntStateOf(-1) }
    var weeklyReminder by remember { mutableStateOf(false) }
    var pendingUpdatePlan by remember { mutableStateOf<String?>(null) }
    var updateReminderDue by remember { mutableStateOf(false) }
    var routeSamples by remember { mutableStateOf<List<RouteSimSample>>(emptyList()) }
    var routeManeuvers by remember { mutableStateOf<List<RouteManeuver>>(emptyList()) }
    var simulating by remember { mutableStateOf(false) }
    var routeSimulator by remember { mutableStateOf<RouteSimulator?>(null) }
    val progressTrackerRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference<RouteProgressTracker?>(null)
        }
    val simulatingRef =
        remember {
            java.util.concurrent.atomic
                .AtomicBoolean(false)
        }
    val roadSignsJsonRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference("[]")
        }
    roadSignsJsonRef.set(roadSignsJson)
    val routeSchoolPoisJsonRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference("[]")
        }
    routeSchoolPoisJsonRef.set(routeSchoolPoisJson)
    val applyFixRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference<(android.location.Location) -> Unit>({})
        }
    val isDebuggable =
        remember {
            (context.applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE) != 0
        }
    var lastViaToastIndex by remember { mutableIntStateOf(-1) }
    var lastNearbyStreetAtMs by remember { mutableLongStateOf(0L) }
    var lastNearbyStreetLat by remember { mutableDoubleStateOf(Double.NaN) }
    var lastNearbyStreetLon by remember { mutableDoubleStateOf(Double.NaN) }
    val nearbyStreetInFlight = remember { AtomicBoolean(false) }

    /** Cold PBF bbox build for [liveSpeedLimitConeJson] — must not run on main. */
    val speedLimitConeInFlight = remember { AtomicBoolean(false) }
    var driveHud by remember {
        mutableStateOf(
            DriveHudState(
                autoZoomLevel = MapHudPrefs.loadAutoZoomLevel(context),
                autoZoomWhileMoving = MapHudPrefs.loadAutoZoomOn(context),
                breakAsDistance = MapHudPrefs.loadBreakAsDistance(context),
                unitSystem = MapHudPrefs.loadUnitSystem(context),
                optIn3d = MapHudPrefs.loadOptIn3d(context),
                contoursEnabled = MapHudPrefs.loadContoursEnabled(context),
                cameraTiltDeg = MapHudPrefs.loadCameraTiltDeg(context),
                snapRotationBackToMode = MapHudPrefs.loadSnapRotationBack(context),
                vulkanAvailable = MapHudPrefs.vulkanRendererAvailable(),
                // Never seed a break countdown until a route is planned.
                minutesToBreak = null,
            ),
        )
    }

    fun clearActiveRoute(message: String = "Route deleted") {
        routeSimulator?.stop()
        routeSimulator = null
        simulating = false
        simulatingRef.set(false)
        NaviMapTestHooks.simulatingActive = false
        progressTrackerRef.set(null)
        routeSamples = emptyList()
        routeManeuvers = emptyList()
        lastViaToastIndex = -1
        mapState =
            mapState.copy(
                polyline = "",
                // Hiking corridors live in routeSegmentsJson; clearing polyline alone
                // leaves applyRouteToStyle redrawing the stale on/off-trail layers.
                routeSegmentsJson = "[]",
                poiLat = 0.0,
                poiLon = 0.0,
                poiName = "",
                poiIconPng = ByteArray(0),
                startName = "",
                startLat = 0.0,
                startLon = 0.0,
                viaName = "",
                viaLat = 0.0,
                viaLon = 0.0,
                viaPoints = emptyList(),
                endName = "",
                endLat = 0.0,
                endLon = 0.0,
                breakPois = emptyList(),
                multiDayCards = emptyList(),
                layerEpoch = mapState.layerEpoch + 1,
            )
        driveHud =
            driveHud.copy(
                minutesToBreak = null,
                tripEtaMinutes = null,
                currentStreet = null,
                currentSpeedKmh = null,
                currentSpeedLimitKmh = null,
                overspeed = false,
            )
        approachGuidance = ApproachGuidanceState()
        NaviMapTestHooks.lastApproachPhase = ApproachUiPhase.Hidden
        NaviMapTestHooks.lastRoutePolylineChars = 0
        NaviMapTestHooks.lastPlanReport = ""
        NaviMapTestHooks.lastPlanDistanceKm = 0.0
        NaviMapTestHooks.lastRoutePolyline = ""
        NaviMapTestHooks.lastBreakPoiCount = 0
        NaviMapTestHooks.lastArrivedAtEnd = false
        NaviMapTestHooks.lastCurrentStreet = null
        status = message
    }

    fun stopRouteSimulation(message: String = "Simulation stopped") {
        routeSimulator?.stop()
        routeSimulator = null
        simulating = false
        simulatingRef.set(false)
        NaviMapTestHooks.simulatingActive = false
        status = message
    }

    fun startRouteSimulation() {
        if (routeSamples.size < 2) {
            status = "No simulation samples on route"
            return
        }
        routeSimulator?.stop()
        progressTrackerRef.get()?.reset()
        lastViaToastIndex = -1
        NaviMapTestHooks.lastArrivedAtEnd = false
        simulating = true
        simulatingRef.set(true)
        NaviMapTestHooks.simulatingActive = true
        val scale = NaviMapTestHooks.simulationTimeScale.coerceAtLeast(0.01)
        val sim =
            RouteSimulator(
                scope = scope,
                samples = routeSamples,
                onFix = { loc -> applyFixRef.get().invoke(loc) },
                onSample = { s ->
                    NaviMapTestHooks.lastSimSpeedKmh = s.speedKmh
                    NaviMapTestHooks.lastSimHighway = s.highway
                    NaviMapTestHooks.lastSimMaxspeedPosted = s.maxspeedPosted
                },
                onFinished = {
                    simulating = false
                    simulatingRef.set(false)
                    NaviMapTestHooks.simulatingActive = false
                    status = "Simulation finished"
                },
            )
        routeSimulator = sim
        status = "SIMULATING"
        sim.start(timeScale = scale)
    }

    fun prepareRouteSimulation() {
        if (routeSamples.size < 2) return
        routeSimulator?.stop()
        simulating = false
        simulatingRef.set(false)
        NaviMapTestHooks.simulatingActive = false
        progressTrackerRef.set(
            RouteProgressTracker(
                samples = routeSamples,
                maneuvers = routeManeuvers,
                viaPoints = viaPoints,
                endPoint =
                    Waypoint(
                        mapState.endName.ifBlank { "End" },
                        mapState.endLat,
                        mapState.endLon,
                    ),
                offRouteThresholdM =
                    if (profile == TravelProfile.HIKING) {
                        RouteProgressTracker.OFF_ROUTE_CROSS_TRACK_HIKING_M
                    } else {
                        RouteProgressTracker.OFF_ROUTE_CROSS_TRACK_MOTOR_M
                    },
            ),
        )
        lastViaToastIndex = -1
        NaviMapTestHooks.lastArrivedAtEnd = false
        routeSimulator =
            RouteSimulator(
                scope = scope,
                samples = routeSamples,
                onFix = { loc -> applyFixRef.get().invoke(loc) },
                onSample = { s ->
                    NaviMapTestHooks.lastSimSpeedKmh = s.speedKmh
                    NaviMapTestHooks.lastSimHighway = s.highway
                    NaviMapTestHooks.lastSimMaxspeedPosted = s.maxspeedPosted
                },
                onFinished = {},
            )
    }
    var pmtilesBaseUrl by remember {
        mutableStateOf(
            MapHudPrefs.loadPmtilesBaseUrl(context).ifBlank {
                runCatching { pmtilesDefaultBaseUrl() }.getOrDefault("")
            },
        )
    }
    var pmtilesJobId by remember { mutableStateOf<String?>(null) }
    var pmtilesProgress by remember { mutableStateOf("") }
    var styleEpoch by remember { mutableIntStateOf(0) }
    var showDriveSettings by remember { mutableStateOf(false) }
    var showMapSettings by remember { mutableStateOf(false) }
    var drivingHoursSinceBreak by remember { mutableDoubleStateOf(0.0) }
    var locationPermGranted by remember {
        mutableStateOf(
            ContextCompat.checkSelfPermission(
                context,
                Manifest.permission.ACCESS_FINE_LOCATION,
            ) == PackageManager.PERMISSION_GRANTED ||
                ContextCompat.checkSelfPermission(
                    context,
                    Manifest.permission.ACCESS_COARSE_LOCATION,
                ) == PackageManager.PERMISSION_GRANTED,
        )
    }
    val locationPermissionLauncher =
        rememberLauncherForActivityResult(
            ActivityResultContracts.RequestMultiplePermissions(),
        ) { result ->
            locationPermGranted = result.values.any { it }
        }

    val dataDir =
        remember {
            NaviAppData.resolve(context)
        }

    /** GeoTIFF DEM decode can take tens of seconds — never on the UI thread. */
    val demSampleInFlight = remember { AtomicBoolean(false) }
    val demAltitudeReady = remember { AtomicBoolean(false) }

    fun enqueueDemAltitude(
        lat: Double,
        lon: Double,
    ) {
        if (NaviMapTestHooks.gpsAltitudeM != null) return
        if (lat == 0.0 && lon == 0.0) return
        if (!demSampleInFlight.compareAndSet(false, true)) return
        val elevPath = File(dataDir, "elevation").absolutePath
        scope.launch {
            try {
                val dem =
                    withContext(Dispatchers.IO) {
                        runCatching { elevationAt(elevPath, lat, lon) }.getOrNull()
                    }
                if (dem != null) {
                    demAltitudeReady.set(true)
                    driveHud = driveHud.copy(altitudeM = dem)
                    NaviMapTestHooks.lastHudAltitudeM = dem
                }
            } finally {
                demSampleInFlight.set(false)
            }
        }
    }

    var offlineIntegrity by remember {
        mutableStateOf(OfflineDataIntegrity.inspect(context, dataDir))
    }
    LaunchedEffect(Unit) {
        val report =
            withContext(Dispatchers.IO) {
                OfflineDataIntegrity.inspect(context, dataDir)
            }
        offlineIntegrity = report
        report.userMessage()?.let { msg ->
            if (status.isBlank() ||
                status.startsWith("Liberty") ||
                status.startsWith("Offline")
            ) {
                status = msg
            }
        }
    }

    /**
     * Apply a planned corridor onto the live map. Used by the Plan button and by
     * [NaviMapTestHooks.pendingRoute] (instrumented tests). Prefer calling this
     * directly from the UI success path — do not rely only on the hook poll, or a
     * stopped duplicate MainActivity can consume [pendingRoute] and leave the
     * visible map without a polyline.
     */
    fun applyPlannedRoute(pending: uniffi.navi.CorridorRouteResult) {
        NaviMapTestHooks.pendingFromPoint?.let {
            fromPoint = it
            NaviMapTestHooks.pendingFromPoint = null
        }
        NaviMapTestHooks.pendingViaPoints?.let {
            viaPoints = it
            NaviMapTestHooks.pendingViaPoints = null
        }
        NaviMapTestHooks.pendingToPoint?.let {
            toPoint = it
            NaviMapTestHooks.pendingToPoint = null
        }
        val iconPng = NaviMapTestHooks.pendingIconPng
        val pts = parsePolyline(pending.routePolyline)
        val startPt = pts.firstOrNull()
        val endPt = pts.lastOrNull()
        // Prefer live waypoint names. Test hooks are fallbacks for instrumented
        // pushes that set labels without a From/To waypoint (never override a
        // real fromPoint after reroute / plan).
        val startLabel =
            fromPoint
                ?.name
                .orEmpty()
                .ifBlank { NaviMapTestHooks.routeStartLabel }
                .ifBlank { "Start" }
        val endLabel =
            toPoint.name
                .ifBlank { NaviMapTestHooks.routeEndLabel }
                .ifBlank { pending.poiName }
                .ifBlank { "End" }
        val viaLabel =
            viaPoints
                .firstOrNull()
                ?.name
                .orEmpty()
                .ifBlank { NaviMapTestHooks.routeViaLabel }
        val breaks =
            parseBreakPoisJson(
                runCatching { pending.breakPoisJson }.getOrDefault("[]"),
            )
        val dayCards =
            parseDaysJson(
                runCatching { pending.daysJson }.getOrDefault("[]"),
            )
        // Pin start / end on the corridor tips (route From / To geometry). Pin vias
        // on the nearest densified corridor point. Pins stay on the red line;
        // WAYPOINT_ROUTE_PIN_MAX_M is the preferred budget vs the chosen place.
        val polyPts = pts.map { it.latitude to it.longitude }

        fun pinOnRoute(
            label: String,
            lat: Double,
            lon: Double,
            tip: LatLng?,
        ): Pair<Double, Double> {
            if (lat == 0.0 && lon == 0.0) {
                return (tip?.latitude ?: 0.0) to (tip?.longitude ?: 0.0)
            }
            val snap =
                if (tip != null) {
                    RoutePinSnap(
                        tip.latitude,
                        tip.longitude,
                        haversineMApprox(lat, lon, tip.latitude, tip.longitude),
                    )
                } else {
                    snapWaypointToRoutePolyline(polyPts, lat, lon)
                }
            if (snap == null) return lat to lon
            if (snap.distM > WAYPOINT_ROUTE_PIN_MAX_M) {
                android.util.Log.w(
                    "NaviRoute",
                    "waypoint pin '$label' ${"%.0f".format(snap.distM)} m from place " +
                        "(prefer <= ${WAYPOINT_ROUTE_PIN_MAX_M.toInt()} m); using corridor point",
                )
            }
            return snap.lat to snap.lon
        }
        val (startLatFix, startLonFix) =
            pinOnRoute(
                startLabel,
                fromPoint?.lat?.takeIf { it != 0.0 } ?: 0.0,
                fromPoint?.lon?.takeIf { it != 0.0 } ?: 0.0,
                startPt,
            )
        val (endLatFix, endLonFix) =
            pinOnRoute(
                endLabel,
                toPoint.lat.takeIf { it != 0.0 } ?: 0.0,
                toPoint.lon.takeIf { it != 0.0 } ?: 0.0,
                endPt,
            )
        val snappedVias =
            viaPoints.map { v ->
                val (plat, plon) = pinOnRoute(v.name, v.lat, v.lon, null)
                v.copy(lat = plat, lon = plon)
            }
        val viaLatFix = snappedVias.firstOrNull()?.lat ?: 0.0
        val viaLonFix = snappedVias.firstOrNull()?.lon ?: 0.0
        mapState =
            MapRouteState(
                polyline = pending.routePolyline,
                routeSegmentsJson = pending.routeSegmentsJson,
                // Keep destination POI marker on the same corridor pin as End
                // (pending.poi* is often the place centroid off the road).
                poiLat = endLatFix,
                poiLon = endLonFix,
                poiName = endLabel.ifBlank { pending.poiName },
                poiIconPng = iconPng,
                startName = startLabel,
                startLat = startLatFix,
                startLon = startLonFix,
                viaName = viaLabel,
                viaLat = viaLatFix,
                viaLon = viaLonFix,
                viaPoints = snappedVias,
                endName = endLabel,
                endLat = endLatFix,
                endLon = endLonFix,
                breakPois = breaks,
                multiDayCards = dayCards,
                gpsLat = mapState.gpsLat,
                gpsLon = mapState.gpsLon,
                gpsAccuracyM = mapState.gpsAccuracyM,
                tracks = mapState.tracks,
                // Clear Compose camera target so CorridorMapView fits the corridor
                // instead of staying locked on the last search pin. Leave follow off
                // until the user taps Recenter (or pans then recenters).
                followGps = false,
                cameraLat = null,
                cameraLon = null,
                cameraZoom = null,
                cameraBearing = mapState.cameraBearing,
                layerEpoch = mapState.layerEpoch + 1,
            )
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.lastRoutePolylineChars = pending.routePolyline.length
        NaviMapTestHooks.lastPlanReport = pending.report
        NaviMapTestHooks.lastPlanDistanceKm = pending.distanceKm
        NaviMapTestHooks.lastRoutePolyline = pending.routePolyline
        NaviMapTestHooks.lastAppliedRouteStartLabel = startLabel
        NaviMapTestHooks.lastBreakPoiCount = breaks.size
        NaviMapTestHooks.lastManeuversJson =
            runCatching { pending.maneuversJson }.getOrDefault("[]")
        NaviMapTestHooks.lastSimSamplesJson =
            runCatching { pending.simSamplesJson }.getOrDefault("[]")
        routeSamples =
            parseRouteSimSamples(
                runCatching { pending.simSamplesJson }.getOrDefault("[]"),
            )
        routeSchoolPoisJson =
            if (schoolPoisJson != "[]" && routeSamples.size >= 2) {
                runCatching {
                    uniffi.navi.schoolsNearRouteCorridorJson(
                        schoolPoisJson,
                        runCatching { pending.simSamplesJson }.getOrDefault("[]"),
                        200.0,
                    )
                }.getOrDefault("[]")
            } else {
                "[]"
            }
        routeManeuvers =
            parseRouteManeuvers(
                runCatching { pending.maneuversJson }.getOrDefault("[]"),
            )
        progressTrackerRef.set(
            RouteProgressTracker(
                samples = routeSamples,
                maneuvers = routeManeuvers,
                viaPoints = snappedVias,
                endPoint = Waypoint(endLabel, endLatFix, endLonFix),
                offRouteThresholdM =
                    if (profile == TravelProfile.HIKING) {
                        RouteProgressTracker.OFF_ROUTE_CROSS_TRACK_HIKING_M
                    } else {
                        RouteProgressTracker.OFF_ROUTE_CROSS_TRACK_MOTOR_M
                    },
            ),
        )
        offRouteCoordinator.reset()
        showHikingReroutePrompt = false
        recalculatingRoute = false
        NaviMapTestHooks.reroutingActive = false
        NaviMapTestHooks.hikingReroutePromptVisible = false
        NaviMapTestHooks.lastOffRoute = false
        lastViaToastIndex = -1
        status =
            userFacingStatus(
                if (pending.distanceKm > 0.0) {
                    DisplayUnits.formatRoutePlanned(pending.distanceKm, driveHud.unitSystem)
                } else {
                    pending.report
                },
            )
        if (pending.routePolyline.isNotBlank()) {
            runCatching {
                val intervalH =
                    breakIntervalHoursForProfile(
                        dataDir.absolutePath,
                        profile,
                    )
                val minsLeft =
                    ((intervalH - drivingHoursSinceBreak) * 60.0)
                        .coerceAtLeast(0.0)
                val etaMin = preDepartureEtaMinutes(profile, pending)
                driveHud =
                    driveHud.copy(
                        minutesToBreak = minsLeft,
                        tripEtaMinutes = etaMin,
                    )
            }
        } else {
            driveHud = driveHud.copy(minutesToBreak = null, tripEtaMinutes = null)
        }
    }

    LaunchedEffect(dataDir) {
        preferOfficialNetworks = uniffi.navi.loadPreferOfficialNetworks(dataDir.absolutePath)
        preferPilgrimRoutes = uniffi.navi.loadPreferPilgrimRoutes(dataDir.absolutePath)
        useNetworkedCabins = uniffi.navi.loadUseNetworkedCabins(dataDir.absolutePath)
        bikeCapability = uniffi.navi.loadBikeCapability(dataDir.absolutePath)
        networkHutMember = uniffi.navi.loadNetworkHutMember(dataDir.absolutePath)
        NaviMapTestHooks.lastSnapRotationBack = driveHud.snapRotationBackToMode
    }
    val iconsDir =
        remember {
            File(context.filesDir, "icons").also { ensureIconsCopied(context, it) }
        }

    fun placeIndexDbForWrite(): File = File(dataDir, "place_index.db")

    fun resolvePlaceIndexDb(): File {
        val preferred = placeIndexDbForWrite()
        // Prefer the app-local copy. /data/local/tmp may look readable (canRead)
        // on the AVD but SQLite open still fails under the app sandbox.
        if (preferred.isFile && preferred.length() > 10_000L) {
            return preferred
        }
        val staged = File("/data/local/tmp/navi_fixtures/place_index_search_check.db")
        if (staged.isFile && staged.canRead() && staged.length() > 10_000L) {
            return staged
        }
        return preferred
    }

    fun resolveRegionPbf(): File? = RouteReplan.resolvePbf(dataDir)

    LaunchedEffect(Unit) {
        val lat = mapState.gpsLat
        val lon = mapState.gpsLon
        if (
            !MapHudPrefs.loadSpeedCameraPromptShown(context) &&
            uniffi.navi.speedCameraJurisdictionAllows(lat, lon)
        ) {
            showSpeedCameraPrompt = true
        }
    }
    // Single effect for live-hazard layers + speed cameras: at most one PBF camera
    // scan per (dataDir, opt-in) need. Cameras are never scanned when opt-in is false.
    LaunchedEffect(speedCameraOptIn, dataDir) {
        if (!speedCameraOptIn) {
            speedCamerasJson = "[]"
            speedCameraWarning = SpeedCameraWarningState()
        }
        val pbf = resolveRegionPbf() ?: return@LaunchedEffect
        withContext(Dispatchers.IO) {
            fun readCache(name: String): String? {
                val candidates =
                    listOf(
                        File(dataDir, "live_hazards_cache/${pbf.nameWithoutExtension}/$name"),
                        File("/data/local/tmp/navi_fixtures/live_hazards_cache/${pbf.nameWithoutExtension}/$name"),
                    )
                return candidates.firstOrNull { it.isFile && it.length() > 2 }?.readText()
            }
            val cachedSigns = readCache("signs.json")
            val cachedCams = readCache("cameras.json")
            val cachedChildren = readCache("children.json")
            val cachedBumps = readCache("bumps.json")
            val signsRaw: String
            val schoolRaw: String
            val camsRaw: String
            val bumpsRaw: String
            val signsChildrenBumpsCached =
                cachedSigns != null && cachedChildren != null && cachedBumps != null
            if (signsChildrenBumpsCached && (!speedCameraOptIn || cachedCams != null)) {
                signsRaw = cachedSigns!!
                schoolRaw = cachedChildren!!
                bumpsRaw = cachedBumps!!
                camsRaw =
                    when {
                        speedCameraOptIn -> cachedCams!!
                        else -> cachedCams ?: "[]"
                    }
            } else {
                signsRaw =
                    cachedSigns
                        ?: runCatching { uniffi.navi.loadRoadSignsJson(pbf.absolutePath) }
                            .getOrElse {
                                NaviMapTestHooks.lastRoadSignsIndexed = -2
                                return@withContext
                            }
                schoolRaw =
                    cachedChildren
                        ?: runCatching { uniffi.navi.loadSchoolPoisJson(pbf.absolutePath) }
                            .getOrDefault("[]")
                bumpsRaw =
                    cachedBumps
                        ?: runCatching { uniffi.navi.loadSpeedBumpsJson(pbf.absolutePath) }
                            .getOrDefault("[]")
                camsRaw =
                    when {
                        !speedCameraOptIn -> cachedCams ?: "[]"
                        cachedCams != null -> cachedCams
                        else ->
                            runCatching { uniffi.navi.loadSpeedCamerasJson(pbf.absolutePath) }
                                .getOrDefault("[]")
                    }
                // Persist compact layers. Never write cameras.json as a poisoned
                // empty array when opt-in is false (that would skip a real scan later).
                runCatching {
                    val cacheDir =
                        File(dataDir, "live_hazards_cache/${pbf.nameWithoutExtension}").also {
                            it.mkdirs()
                        }
                    File(cacheDir, "signs.json").writeText(signsRaw)
                    File(cacheDir, "children.json").writeText(schoolRaw)
                    File(cacheDir, "bumps.json").writeText(bumpsRaw)
                    if (speedCameraOptIn && !camsRaw.contains("\"error\"")) {
                        File(cacheDir, "cameras.json").writeText(camsRaw)
                    }
                }
            }
            roadSignsJson = signsRaw
            NaviMapTestHooks.lastRoadSignsIndexed =
                if (signsRaw.contains("\"error\"")) {
                    -3
                } else {
                    runCatching { org.json.JSONArray(signsRaw).length() }.getOrDefault(0)
                }
            NaviMapTestHooks.lastSchoolPoisIndexed =
                if (schoolRaw.contains("\"error\"")) {
                    -2
                } else {
                    runCatching { org.json.JSONArray(schoolRaw).length() }.getOrDefault(0)
                }
            schoolPoisJson = schoolRaw
            if (speedCameraOptIn) {
                speedCamerasJson = camsRaw
            }
            val liveStats =
                runCatching {
                    uniffi.navi.liveHazardsIngestFromJson(
                        pbf.absolutePath,
                        signsRaw,
                        if (speedCameraOptIn) camsRaw else "[]",
                        schoolRaw,
                        bumpsRaw,
                    )
                }.getOrNull()
            if (liveStats != null) {
                NaviMapTestHooks.lastLiveHazardSigns = liveStats.signs.toInt()
                NaviMapTestHooks.lastLiveHazardChildren = liveStats.children.toInt()
                NaviMapTestHooks.lastLiveHazardCameras = liveStats.cameras.toInt()
                NaviMapTestHooks.lastLiveHazardBumps = liveStats.bumps.toInt()
                NaviMapTestHooks.lastLiveHazardCompactUtf8 = liveStats.compactJsonUtf8.toLong()
                NaviMapTestHooks.lastLiveHazardConeM = liveStats.coneM
            }
        }
    }

    fun graphCacheDirForPbf(pbf: File): File {
        val graphTag =
            when (profile) {
                TravelProfile.BICYCLE, TravelProfile.BICYCLE_ELECTRIC -> "bicycle"
                TravelProfile.HIKING -> "foot"
                TravelProfile.TRUCK,
                TravelProfile.TRUCK_ELECTRIC,
                TravelProfile.MOBILE_HOME,
                -> "truck"
                else -> "car"
            }
        return File(dataDir, "graph-cache-${pbf.nameWithoutExtension}-$graphTag")
    }

    fun refreshRoutes() {
        savedRoutes = listSavedRoutes(dataDir.absolutePath)
    }

    fun refreshPlaces() {
        savedPlaces = listSavedPlaces(dataDir.absolutePath)
    }

    suspend fun resolveLabelAt(
        lat: Double,
        lon: Double,
    ): Pair<String, String> {
        val resolved =
            withContext(Dispatchers.IO) {
                val hits =
                    try {
                        nearbyPlaces(
                            resolvePlaceIndexDb().absolutePath,
                            lat,
                            lon,
                            GPS_WAYPOINT_RESOLVE_RADIUS_M,
                            16u,
                        )
                    } catch (e: kotlinx.coroutines.CancellationException) {
                        throw e
                    } catch (_: Exception) {
                        emptyList()
                    }
                pickNearbyPlaceNameForGpsWaypoint(hits)
                    ?: try {
                        val pbf = resolveRegionPbf() ?: return@withContext null
                        roadLabelNear(
                            pbf.absolutePath,
                            graphCacheDirForPbf(pbf).absolutePath,
                            File(dataDir, "elevation").absolutePath,
                            lat,
                            lon,
                            profile,
                            GPS_WAYPOINT_RESOLVE_RADIUS_M,
                        )?.trim()?.takeIf { it.isNotEmpty() }
                    } catch (e: kotlinx.coroutines.CancellationException) {
                        throw e
                    } catch (_: Exception) {
                        null
                    }
            }
        return if (resolved != null) {
            resolved to "map-resolved"
        } else {
            formatMapMarkFallback(lat, lon) to "map-mark"
        }
    }

    /**
     * Fast start label for reroute: place-index only (no road-graph build).
     * Falls back to GPS coordinate text within a short timeout.
     */
    suspend fun resolveRerouteStartLabel(
        lat: Double,
        lon: Double,
    ): String {
        val fromIndex =
            withTimeoutOrNull(2_000L) {
                withContext(Dispatchers.IO) {
                    try {
                        val hits =
                            nearbyPlaces(
                                resolvePlaceIndexDb().absolutePath,
                                lat,
                                lon,
                                GPS_WAYPOINT_RESOLVE_RADIUS_M,
                                16u,
                            )
                        pickNearbyPlaceNameForGpsWaypoint(hits)
                    } catch (e: kotlinx.coroutines.CancellationException) {
                        throw e
                    } catch (_: Exception) {
                        null
                    }
                }
            }
        return fromIndex?.takeIf { it.isNotBlank() }
            ?: formatGpsWaypointFallback(lat, lon)
    }

    /**
     * Recompute from current GPS to remaining vias + destination after a
     * confirmed off-route. Uses [RouteReplan] (same UniFFI pipeline as Plan).
     * Start label is resolved via [resolveLabelAt] (~12 m), matching Use GPS.
     */
    fun startRerouteFromCurrent(
        lat: Double,
        lon: Double,
    ) {
        if (recalculatingRoute || planningRoute) return
        if (toPoint.name.isBlank() && mapState.endName.isBlank()) return
        val dest =
            if (toPoint.lat != 0.0 || toPoint.lon != 0.0) {
                toPoint
            } else {
                Waypoint(mapState.endName.ifBlank { "End" }, mapState.endLat, mapState.endLon)
            }
        val remainingVias =
            viaPoints.filterIndexed { idx, _ ->
                idx > NaviMapTestHooks.lastViaIndex
            }
        rerouteJob?.cancel()
        cancelInFlightPlan()
        planAbort.set(false)
        rerouteJob =
            scope.launch {
                recalculatingRoute = true
                planningRoute = true
                planProgressClear()
                try {
                    foregroundPlanEnter()
                    NaviMapTestHooks.reroutingActive = true
                    NaviMapTestHooks.autoRerouteTriggeredCount += 1
                    routePlanProgress = "Recalculating route…"
                    routePlanPct = 0
                    status = "Recalculating route… (may take several seconds)"
                    planIndexingHintVisible =
                        withContext(Dispatchers.IO) {
                            val planPbf = resolveRegionPbf()
                            if (planPbf == null || !planPbf.isFile) {
                                true
                            } else {
                                runCatching {
                                    indexedMapsStatus(planPbf.absolutePath, dataDir.absolutePath).trim()
                                }.getOrDefault("missing") != "ready"
                            }
                        }
                    val startWp =
                        Waypoint(resolveRerouteStartLabel(lat, lon), lat, lon)
                    val pts =
                        buildList {
                            add(startWp)
                            addAll(remainingVias)
                            add(dest)
                        }
                    val ecoForPlan = if (ecoModeToggleable(profile)) ecoEnabled else true
                    val vehicle =
                        runCatching { loadVehicleLimits(dataDir.absolutePath) }
                            .getOrElse {
                                FfiVehicleLimits(null, null, null, null, null, null)
                            }
                    val result =
                        runCatching {
                            RouteReplan.plan(
                                dataDir = dataDir,
                                profile = profile,
                                waypoints = pts,
                                useEco = ecoForPlan,
                                avoidMotorways = avoidMotorways,
                                avoidTolls = avoidTolls,
                                avoidFerries = avoidFerries,
                                vehicle = vehicle,
                                preferOfficialNetworks = preferOfficialNetworks,
                                preferPilgrimRoutes = preferPilgrimRoutes,
                                onProgress = { pct, detail ->
                                    routePlanPct = pct
                                    routePlanProgress = "Recalculating route… $detail"
                                },
                            )
                        }.getOrElse { e ->
                            if (e is kotlinx.coroutines.CancellationException) throw e
                            status = "Reroute failed: ${e.message}"
                            recalculatingRoute = false
                            planningRoute = false
                            planIndexingHintVisible = false
                            NaviMapTestHooks.reroutingActive = false
                            routePlanProgress = ""
                            offRouteCoordinator.suppressUntilOnRoute()
                            return@launch
                        }
                    if (!isActive) return@launch
                    recalculatingRoute = false
                    planningRoute = false
                    planIndexingHintVisible = false
                    NaviMapTestHooks.reroutingActive = false
                    routePlanProgress = ""
                    if (planAbort.get() || planReportIsCancelled(result.report)) {
                        NaviMapTestHooks.lastPlanReport = result.report
                        RoutingPlanLog.cancelled(
                            ecoEnabled = ecoForPlan,
                            durationMs = 0,
                            reason = "cancelled",
                            report = result.report,
                        )
                        offRouteCoordinator.suppressUntilOnRoute()
                        if (status != "Kept original route") {
                            status = "Kept original route"
                        }
                        return@launch
                    }
                    if (!result.report.contains("PASS") || result.routePolyline.isBlank()) {
                        status = userFacingStatus(result.report).ifBlank { "Reroute failed" }
                        offRouteCoordinator.suppressUntilOnRoute()
                        return@launch
                    }
                    // Drop stale Plan-time hook labels so applyPlannedRoute uses
                    // the live fromPoint name from resolveLabelAt.
                    NaviMapTestHooks.routeStartLabel = ""
                    fromPoint = startWp
                    applyPlannedRoute(result)
                    status =
                        "Route updated · ${"%.1f".format(result.distanceKm)} km " +
                        "(recalculated after detour)"
                } finally {
                    foregroundPlanLeave()
                    planProgressClear()
                }
            }
    }

    val showVehicleClearance =
        profile == TravelProfile.TRUCK ||
            profile == TravelProfile.TRUCK_ELECTRIC ||
            profile == TravelProfile.MOBILE_HOME
    val showVehicleHeightOnly =
        profile == TravelProfile.CAR ||
            profile == TravelProfile.CAR_ELECTRIC
    val showVehicleLimitsPanel = showVehicleClearance || showVehicleHeightOnly

    fun persistVehicle() {
        val limits =
            FfiVehicleLimits(
                axleWeightKg = if (showVehicleClearance) axleKg.toDoubleOrNull() else null,
                bogieWeightKg = if (showVehicleClearance) bogieKg.toDoubleOrNull() else null,
                heightM = if (showVehicleLimitsPanel) heightM.toDoubleOrNull() else null,
                widthM = if (showVehicleClearance) widthM.toDoubleOrNull() else null,
                lengthM = if (showVehicleClearance) lengthM.toDoubleOrNull() else null,
                totalWeightKg = null,
            )
        val ok = saveVehicleLimits(dataDir.absolutePath, limits)
        if (ok) {
            limits.axleWeightKg?.let { DiagnosticLog.logSettingSaved("vehicle_axle_weight_kg", it) }
            limits.bogieWeightKg?.let { DiagnosticLog.logSettingSaved("vehicle_bogie_weight_kg", it) }
            limits.heightM?.let { DiagnosticLog.logSettingSaved("vehicle_height_m", it) }
            limits.widthM?.let { DiagnosticLog.logSettingSaved("vehicle_width_m", it) }
            limits.lengthM?.let { DiagnosticLog.logSettingSaved("vehicle_length_m", it) }
        }
        status = if (ok) "Vehicle limits saved" else "Failed to save vehicle limits"
    }

    fun formatProgressPct(
        done: ULong,
        total: ULong?,
        label: String,
    ): String {
        val pct =
            if (total != null && total > 0uL) {
                ((done.toDouble() * 100.0) / total.toDouble()).roundToInt().coerceIn(0, 100)
            } else {
                null
            }
        return when {
            pct != null && total != null -> "$label $pct% ($done / $total)"
            pct != null -> "$label $pct%"
            total == null -> label
            else -> "$label $done / ?"
        }
    }

    fun startRegionDownload(path: String) {
        val leaf = path.substringAfterLast('/')
        val filename = "$leaf-latest.osm.pbf"
        val url = geofabrikLatestPbfUrl(path)
        val already = RegionDownloadBackground.partialBytes(dataDir, filename)
        regionDownloadProgress =
            if (already > 0L) "Resuming download…" else "Downloading region… 0%"
        downloadPolling = true
        status =
            if (already > 0L) {
                "Resuming download of $path…"
            } else {
                "Downloading $path..."
            }
        MapHudPrefs.saveGeofabrikPath(context, path)
        RegionDownloadBackground.ensureStarted(
            context,
            dataDir,
            url,
            filename,
            path,
        )
    }

    LaunchedEffect(downloadPolling, pmtilesJobId) {
        while (isActive) {
            val regionRunning = RegionDownloadBackground.isRunning()
            if (!downloadPolling && !regionRunning && pmtilesJobId == null) {
                delay(400)
                continue
            }
            if (regionRunning) {
                downloadPolling = true
            }
            val snap = runCatching { downloadProgressSnapshot() }.getOrNull()
            if (snap != null && snap.label.isNotBlank()) {
                val line = formatProgressPct(snap.unitsDone, snap.unitsTotal, snap.label)
                if (snap.label.contains("map tiles", ignoreCase = true) ||
                    snap.label.contains("basemap", ignoreCase = true) ||
                    snap.label.contains("DEM", ignoreCase = true) ||
                    snap.label.contains("Planning extract", ignoreCase = true) ||
                    snap.label.contains("Writing map archive", ignoreCase = true)
                ) {
                    pmtilesProgress = line
                } else {
                    regionDownloadProgress = line
                    if (regionRunning && !planningRoute) {
                        status = line
                    }
                }
            } else if (regionRunning) {
                val line = RegionDownloadBackground.uiLine()
                if (line.isNotBlank()) {
                    regionDownloadProgress = line
                    if (!planningRoute) status = line
                }
            }
            pmtilesJobId?.let { id ->
                runCatching { pmtilesGetJob(dataDir.absolutePath, id) }.getOrNull()?.let { job ->
                    val label =
                        when {
                            job.regionKey.endsWith("_dem") -> "Downloading terrain DEM…"
                            else -> "Downloading map tiles for region…"
                        }
                    pmtilesProgress = formatProgressPct(job.bytesReceived, job.totalBytes, label)
                }
            }
            if (!regionRunning && downloadPolling && pmtilesJobId == null) {
                val leftover = RegionDownloadBackground.statusLine()
                if (leftover == "done" || leftover.startsWith("failed")) {
                    downloadPolling = false
                }
            }
            delay(400)
        }
    }

    LaunchedEffect(dataDir) {
        RegionDownloadBackground.ensureStartedFromPending(context, dataDir)
        if (RegionDownloadBackground.isRunning() || RegionDownloadBackground.discoverPending(dataDir) != null) {
            downloadPolling = true
            showTools = true
            val pending = RegionDownloadBackground.discoverPending(dataDir)
            if (pending != null && pending.geofabrikPath.isNotBlank()) {
                selectedGeofabrikPath = pending.geofabrikPath
            }
        }
        val pbf = resolveRegionPbf()
        if (pbf != null && pbf.isFile) {
            PlaceIndexBackground.ensureStarted(pbf, placeIndexDbForWrite())
        }
    }

    // Passive indexed-maps status; auto-start background rebuild when packs are stale.
    LaunchedEffect(Unit) {
        while (isActive) {
            val pbf =
                dataDir.listFiles()?.firstOrNull {
                    it.isFile && it.name.endsWith(".osm.pbf")
                }
            if (pbf != null) {
                val elev = File(dataDir, "elevation").takeIf { it.isDirectory }
                IndexedMapsBackground.ensureStarted(scope, pbf, dataDir, elev)
                indexedMapsUiLine = IndexedMapsBackground.uiLine(pbf, dataDir)
                if (IndexedMapsBackground.isRunning() &&
                    !planningRoute &&
                    !RegionDownloadBackground.isRunning()
                ) {
                    val line = indexedMapsUiLine
                    if (line.isNotBlank()) status = line
                }
                placeIndexUiLine =
                    if (PlaceIndexBackground.isRunning()) {
                        "Place index: building (background)"
                    } else {
                        val st = PlaceIndexBackground.statusLine()
                        if (st == "idle") "" else "Place index: $st"
                    }
            } else {
                indexedMapsUiLine = ""
            }
            delay(2_500)
        }
    }

    LaunchedEffect(planningRoute) {
        if (!planningRoute) return@LaunchedEffect
        while (isActive && planningRoute) {
            val snap = runCatching { planProgressSnapshot() }.getOrNull()
            if (snap != null && snap.label.isNotBlank()) {
                val line = formatProgressPct(snap.unitsDone, snap.unitsTotal, snap.label)
                routePlanProgress = line
                status = line
                routePlanPct =
                    monotonicPlanPercent(routePlanPct, snap.percent?.toInt())
            }
            delay(250)
        }
    }

    LaunchedEffect(Unit) {
        val limits = loadVehicleLimits(dataDir.absolutePath)
        axleKg = limits.axleWeightKg?.toString().orEmpty()
        bogieKg = limits.bogieWeightKg?.toString().orEmpty()
        heightM = limits.heightM?.toString().orEmpty()
        widthM = limits.widthM?.toString().orEmpty()
        lengthM = limits.lengthM?.toString().orEmpty()
        refreshRoutes()
        refreshPlaces()
        updateReminderDue = osmWeeklyReminderDue(dataDir.absolutePath)
        runCatching {
            ecoEnabled = if (usesTruckRestSettings(profile)) {
                loadTruckRestSettings(dataDir.absolutePath).ecoModeEnabled
            } else {
                loadCarRestSettings(dataDir.absolutePath).ecoModeEnabled
            } ||
                ecoModeDefault(profile)
            // Break minutes are only meaningful with an active route — do not seed on launch.
            driveHud =
                driveHud.copy(
                    ecoActive = ecoEnabled,
                    minutesToBreak = null,
                    distanceToTurnKm = null,
                    breakAsDistance = MapHudPrefs.loadBreakAsDistance(context),
                    unitSystem = MapHudPrefs.loadUnitSystem(context),
                )
        }
        if (!locationPermGranted) {
            locationPermissionLauncher.launch(
                arrayOf(
                    Manifest.permission.ACCESS_FINE_LOCATION,
                    Manifest.permission.ACCESS_COARSE_LOCATION,
                ),
            )
        }

        // Continuous hook poll via Handler so it survives Compose LaunchedEffect
        // cancellation that was observed mid-instrumented-test (after several screenshots).
    }

    fun modeTargetBearing(): Double? =
        when (driveHud.rotationMode) {
            MapRotationMode.NorthUp -> 0.0
            MapRotationMode.Compass -> NaviMapTestHooks.magneticHeadingDeg
            MapRotationMode.DirectionOfTravel -> NaviMapTestHooks.gpsBearingDeg
        }

    fun cancelPendingRotationSnap() {
        pendingRotationSnap?.let { rotationSnapHandler.removeCallbacks(it) }
        pendingRotationSnap = null
    }

    fun reassertModeBearing(force: Boolean = false) {
        if (manualRotationSticky && !driveHud.snapRotationBackToMode) {
            NaviMapTestHooks.manualRotationOverrideActive = true
            return
        }
        val target = modeTargetBearing() ?: return
        val cur = mapState.cameraBearing
        val mapBr = NaviMapTestHooks.lastCameraBearing
        val diverged =
            kotlin.math.abs(cur - target) > 0.05 || kotlin.math.abs(mapBr - target) > 0.05
        if (!force && !diverged) {
            NaviMapTestHooks.manualRotationOverrideActive = false
            return
        }
        manualRotationSticky = false
        NaviMapTestHooks.manualRotationOverrideActive = false
        if (NaviMapTestHooks.applyBearingToMap) {
            mapState = mapState.copy(cameraBearing = target)
            bearingApplyEpoch += 1
        }
        NaviMapTestHooks.lastCameraBearing = target
    }

    fun onManualRotateEnded(bearingDeg: Double) {
        // Keep Compose cameraBearing aligned with the MapLibre gesture result so
        // the HUD poll cannot overwrite lastCameraBearing back to the old mode
        // target while the override / snap-back window is active.
        mapState = mapState.copy(cameraBearing = bearingDeg)
        NaviMapTestHooks.lastCameraBearing = bearingDeg
        cancelPendingRotationSnap()
        if (!driveHud.snapRotationBackToMode) {
            manualRotationSticky = true
            NaviMapTestHooks.manualRotationOverrideActive = true
            return
        }
        manualRotationSticky = false
        NaviMapTestHooks.manualRotationOverrideActive = true
        val snap =
            Runnable {
                pendingRotationSnap = null
                reassertModeBearing(force = true)
            }
        pendingRotationSnap = snap
        rotationSnapHandler.postDelayed(snap, 1_000L)
    }

    DisposableEffect(locationPermGranted) {
        val lm = context.getSystemService(LocationManager::class.java)

        fun applyFix(loc: Location?) {
            if (loc == null) return
            val provider = loc.provider.orEmpty()
            val testOrSim =
                provider == "navi-route-sim" || provider == "navi-test-inject"
            // While simulating, ignore real LocationManager fixes.
            if (simulatingRef.get() && !testOrSim) return
            // Instrumented off-route injects: keep live GPS from clobbering.
            if (NaviMapTestHooks.ignoreLiveGpsFixes && !testOrSim) return
            // Always update map GPS mark from a valid fix.
            if (loc.latitude != 0.0 || loc.longitude != 0.0) {
                NaviMapTestHooks.lastGpsProvider = provider
                // Mirror into native so lastGpsFix() / currentSpeedKmh() stay live.
                val speedKmh =
                    if (loc.hasSpeed() && loc.speed.isFinite() && loc.speed >= 0f) {
                        loc.speed * 3.6
                    } else {
                        null
                    }
                val speedAccKmh =
                    if (loc.hasSpeedAccuracy() &&
                        loc.speedAccuracyMetersPerSecond.isFinite() &&
                        loc.speedAccuracyMetersPerSecond > 0f
                    ) {
                        loc.speedAccuracyMetersPerSecond * 3.6
                    } else {
                        null
                    }
                runCatching {
                    updateGpsFix(
                        loc.latitude,
                        loc.longitude,
                        available = true,
                        speedKmh = speedKmh,
                    )
                }
                if (speedKmh != null) {
                    NaviMapTestHooks.lastGpsSpeedKmh = speedKmh
                }
                val acc = if (loc.hasAccuracy()) loc.accuracy else null
                val moved =
                    kotlin.math.hypot(
                        loc.latitude - mapState.gpsLat,
                        loc.longitude - mapState.gpsLon,
                    ) > 1e-6
                if (moved || mapState.gpsLat == 0.0) {
                    NaviMapTestHooks.lastGpsLat = loc.latitude
                    NaviMapTestHooks.lastGpsLon = loc.longitude
                    mapState =
                        if (NaviMapTestHooks.disableGpsFollow) {
                            mapState.copy(
                                gpsLat = loc.latitude,
                                gpsLon = loc.longitude,
                                gpsAccuracyM = acc,
                                followGps = false,
                                layerEpoch = mapState.layerEpoch + 1,
                            )
                        } else if (mapState.followGps) {
                            mapState.copy(
                                gpsLat = loc.latitude,
                                gpsLon = loc.longitude,
                                gpsAccuracyM = acc,
                                cameraLat = loc.latitude,
                                cameraLon = loc.longitude,
                                layerEpoch = mapState.layerEpoch + 1,
                            )
                        } else {
                            mapState.copy(
                                gpsLat = loc.latitude,
                                gpsLon = loc.longitude,
                                gpsAccuracyM = acc,
                                layerEpoch = mapState.layerEpoch + 1,
                            )
                        }
                    if (NaviMapTestHooks.disableGpsFollow) {
                        NaviMapTestHooks.followGps = false
                    }
                } else if (acc != mapState.gpsAccuracyM) {
                    mapState = mapState.copy(gpsAccuracyM = acc)
                }
                // Live guidance from GPS / simulator along the planned route.
                val tracker = progressTrackerRef.get()
                var streetFromRoute = false
                if (tracker != null && mapState.polyline.isNotBlank()) {
                    val snap = tracker.update(loc.latitude, loc.longitude)
                    NaviMapTestHooks.lastSimAlongM = snap.alongM
                    NaviMapTestHooks.lastDistanceToManeuverM = snap.distanceToManeuverM
                    NaviMapTestHooks.lastViaIndex = snap.viaIndexReached
                    if (snap.arrivedAtEnd) {
                        NaviMapTestHooks.lastArrivedAtEnd = true
                    }
                    NaviMapTestHooks.lastManeuverKind = snap.maneuver?.kind
                    snap.sample?.let { s ->
                        if (!simulatingRef.get()) {
                            NaviMapTestHooks.lastSimSpeedKmh = s.speedKmh
                            NaviMapTestHooks.lastSimHighway = s.highway
                            NaviMapTestHooks.lastSimMaxspeedPosted = s.maxspeedPosted
                        }
                        val road = formatCurrentRoadLabel(s.street, s.highway)
                        streetFromRoute = true
                        val limit =
                            resolveSpeedLimitKmh(
                                postedKmh = s.maxspeedKmh,
                                maxspeedConditional = s.maxspeedConditional,
                                highway = s.highway,
                            )
                        val over =
                            OverspeedHud.isOverspeed(speedKmh, limit, speedAccKmh)
                        NaviMapTestHooks.lastCurrentStreet = road
                        NaviMapTestHooks.lastCurrentSpeedLimitKmh = limit
                        NaviMapTestHooks.lastOverspeed = over
                        if (
                            driveHud.currentStreet != road ||
                            driveHud.currentSpeedKmh != speedKmh ||
                            driveHud.currentSpeedLimitKmh != limit ||
                            driveHud.overspeed != over
                        ) {
                            driveHud =
                                driveHud.copy(
                                    currentStreet = road,
                                    currentSpeedKmh = speedKmh,
                                    currentSpeedLimitKmh = limit,
                                    overspeed = over,
                                )
                        }
                    }
                    if (loc.hasBearing()) {
                        NaviMapTestHooks.gpsBearingDeg = loc.bearing.toDouble()
                    } else if (snap.bearingDeg > 0.0) {
                        NaviMapTestHooks.gpsBearingDeg = snap.bearingDeg
                    }
                    val man = snap.maneuver
                    NaviMapTestHooks.lastOffRoute = snap.offRoute
                    NaviMapTestHooks.lastCrossTrackM = snap.crossTrackM
                    if (snap.offRoute) {
                        approachGuidance =
                            ApproachGuidanceState(
                                active = true,
                                offRoute = true,
                                unitSystem = driveHud.unitSystem,
                            )
                        NaviMapTestHooks.lastApproachPhase = approachUiPhase(approachGuidance)
                        NaviMapTestHooks.lastApproachIconKey = null
                    } else if (man != null && snap.distanceToManeuverM.isFinite()) {
                        val endWp = toPoint
                        val useEndAddr = man.kind == "destination"
                        val (street, house, post) =
                            parseAddressDisplayLines(
                                street = if (useEndAddr) endWp.street else man.street,
                                houseNumber = if (useEndAddr) endWp.houseNumber else man.houseNumber,
                                postcode = if (useEndAddr) endWp.postcode else man.postcode,
                                combined = if (useEndAddr && endWp.street == null) endWp.name else null,
                            )
                        val icon = man.iconKey()
                        approachGuidance =
                            ApproachGuidanceState(
                                active = true,
                                distanceM = snap.distanceToManeuverM,
                                iconKey = icon,
                                nextStreet = street ?: man.street,
                                houseNumber = house,
                                postcode = post,
                                roundaboutExit = man.roundaboutExit,
                                unitSystem = driveHud.unitSystem,
                                offRoute = false,
                            )
                        NaviMapTestHooks.lastApproachPhase = approachUiPhase(approachGuidance)
                        NaviMapTestHooks.lastApproachIconKey = icon
                    } else {
                        approachGuidance = ApproachGuidanceState()
                        NaviMapTestHooks.lastApproachPhase = ApproachUiPhase.Hidden
                        NaviMapTestHooks.lastApproachIconKey = null
                    }
                    if (speedCameraOptIn && speedCamerasJson != "[]") {
                        val warnJson =
                            uniffi.navi.nearestSpeedCameraWarningJson(
                                speedCamerasJson,
                                loc.latitude,
                                loc.longitude,
                                true,
                            )
                        speedCameraWarning =
                            speedCameraWarningFromJson(warnJson).copy(
                                unitSystem = driveHud.unitSystem,
                            )
                    } else {
                        speedCameraWarning = SpeedCameraWarningState()
                    }
                    val signJson =
                        if (roadSignsJsonRef.get() != "[]" &&
                            uniffi.navi.roadSignJurisdictionAllows(loc.latitude, loc.longitude)
                        ) {
                            uniffi.navi.nearestRoadSignWarningJson(
                                roadSignsJsonRef.get(),
                                loc.latitude,
                                loc.longitude,
                            )
                        } else {
                            "{}"
                        }
                    val schoolFallbackJson =
                        if (routeSchoolPoisJsonRef.get() != "[]") {
                            uniffi.navi.nearestSchoolProximityWarningJson(
                                routeSchoolPoisJsonRef.get(),
                                loc.latitude,
                                loc.longitude,
                            )
                        } else {
                            "{}"
                        }
                    val signState = roadSignWarningFromJson(signJson)
                    val schoolState = roadSignWarningFromJson(schoolFallbackJson)
                    val finalRoadSignJson =
                        when {
                            // Real mapped children warning (142) has priority over proximity fallback.
                            signState.active && signState.code == "142" -> signJson
                            // Proximity fallback when no explicit children-sign tag is active.
                            schoolState.active -> schoolFallbackJson
                            else -> signJson
                        }
                    roadSignWarning =
                        roadSignWarningFromJson(finalRoadSignJson).copy(
                            unitSystem = driveHud.unitSystem,
                        )
                    NaviMapTestHooks.lastRoadSignWarningJson =
                        if (roadSignWarning.active) finalRoadSignJson else "{}"
                    NaviMapTestHooks.lastSchoolProximityWarningJson = schoolFallbackJson
                    NaviMapTestHooks.lastRouteSchoolPoiCount =
                        runCatching { org.json.JSONArray(routeSchoolPoisJsonRef.get()).length() }
                            .getOrDefault(0)
                    val offAction =
                        offRouteCoordinator.onFix(
                            offRoute = snap.offRoute,
                            hiking = profile == TravelProfile.HIKING,
                            busy = recalculatingRoute || planningRoute || showHikingReroutePrompt,
                            confirmMs =
                                NaviMapTestHooks.offRouteConfirmMsOverride
                                    ?: RouteProgressTracker.OFF_ROUTE_CONFIRM_MS,
                        )
                    when (offAction) {
                        OffRouteCoordinator.Action.AutoReroute -> {
                            startRerouteFromCurrent(loc.latitude, loc.longitude)
                        }
                        OffRouteCoordinator.Action.PromptHikingReroute -> {
                            showHikingReroutePrompt = true
                            NaviMapTestHooks.hikingReroutePromptVisible = true
                            status = "Off trail — recalculate route?"
                        }
                        else -> Unit
                    }
                    if (snap.remainingEtaMinutes.isFinite() && !snap.offRoute) {
                        val intervalH =
                            runCatching {
                                breakIntervalHoursForProfile(dataDir.absolutePath, profile)
                            }.getOrDefault(2.0)
                        // Integrate planned segment times — not alongM / instantaneous speed
                        // (that under-counts elapsed hours when current speed is above the
                        // average so far, which inflates minutes-to-break).
                        val drivenH = snap.elapsedDrivingHours.coerceAtLeast(0.0)
                        drivingHoursSinceBreak = drivenH
                        val minsLeft = ((intervalH - drivenH) * 60.0).coerceAtLeast(0.0)
                        driveHud =
                            driveHud.copy(
                                tripEtaMinutes = snap.remainingEtaMinutes,
                                minutesToBreak = if (driveHud.breakRemindersEnabled) minsLeft else null,
                            )
                        NaviMapTestHooks.lastMinutesToBreak = driveHud.minutesToBreak
                        NaviMapTestHooks.lastElapsedDrivingHours = drivenH
                    }
                    if (snap.viaIndexReached > lastViaToastIndex) {
                        lastViaToastIndex = snap.viaIndexReached
                        val viaName =
                            viaPoints.getOrNull(snap.viaIndexReached)?.name
                                ?: "Via ${snap.viaIndexReached + 1}"
                        status = "Passed $viaName — continuing"
                    }
                    if (snap.arrivedAtEnd) {
                        if (simulatingRef.get()) {
                            stopRouteSimulation("Arrived at destination")
                        } else {
                            status = "Arrived at destination"
                        }
                    }
                    DiagnosticLog.onManeuverProgress(
                        index = snap.maneuverIndex,
                        of = tracker.maneuverCount(),
                        kind = snap.maneuver?.kind,
                        street = snap.maneuver?.street,
                        distanceM =
                            snap.distanceToManeuverM.takeIf {
                                it.isFinite() && it < Double.POSITIVE_INFINITY / 2
                            },
                    )
                    if (!(manualRotationSticky && !driveHud.snapRotationBackToMode) &&
                        pendingRotationSnap == null
                    ) {
                        when (driveHud.rotationMode) {
                            MapRotationMode.DirectionOfTravel -> {
                                val br = NaviMapTestHooks.gpsBearingDeg
                                if (br != null) {
                                    mapState = mapState.copy(cameraBearing = br)
                                    NaviMapTestHooks.lastCameraBearing = br
                                }
                            }
                            MapRotationMode.Compass -> {
                                val br = NaviMapTestHooks.magneticHeadingDeg
                                if (br != null) {
                                    mapState = mapState.copy(cameraBearing = br)
                                    NaviMapTestHooks.lastCameraBearing = br
                                }
                            }
                            MapRotationMode.NorthUp -> {
                                if (kotlin.math.abs(mapState.cameraBearing) > 0.05 ||
                                    kotlin.math.abs(NaviMapTestHooks.lastCameraBearing) > 0.05
                                ) {
                                    reassertModeBearing(force = true)
                                }
                            }
                        }
                    }
                } else if (
                    NaviMapTestHooks.liveHazardConeEnabled &&
                    progressTrackerRef.get() == null
                ) {
                    // Route-independent 300 m heading cone (no planned route / progress tracker).
                    if (loc.hasBearing() && loc.bearing.isFinite()) {
                        NaviMapTestHooks.gpsBearingDeg = loc.bearing.toDouble()
                    }
                    val headingDeg =
                        if (loc.hasBearing() && loc.bearing.isFinite()) {
                            loc.bearing.toDouble()
                        } else {
                            null
                        }
                    val camJson =
                        if (speedCameraOptIn) {
                            uniffi.navi.liveHazardConeSpeedCameraWarningJson(
                                loc.latitude,
                                loc.longitude,
                                headingDeg,
                                true,
                            )
                        } else {
                            "{}"
                        }
                    speedCameraWarning =
                        speedCameraWarningFromJson(camJson).copy(
                            unitSystem = driveHud.unitSystem,
                        )
                    val signJson =
                        uniffi.navi.liveHazardConeRoadSignWarningJson(
                            loc.latitude,
                            loc.longitude,
                            headingDeg,
                        )
                    val schoolFallbackJson =
                        uniffi.navi.liveHazardConeChildrenWarningJson(
                            loc.latitude,
                            loc.longitude,
                            headingDeg,
                        )
                    val signState = roadSignWarningFromJson(signJson)
                    val schoolState = roadSignWarningFromJson(schoolFallbackJson)
                    var finalRoadSignJson =
                        when {
                            signState.active && signState.code == "142" -> signJson
                            schoolState.active -> schoolFallbackJson
                            else -> signJson
                        }
                    // Apply hazard/school plate immediately. Speed-limit cone may
                    // cold-build a PBF bbox graph (tens of seconds) — never on main.
                    roadSignWarning =
                        roadSignWarningFromJson(finalRoadSignJson).copy(
                            unitSystem = driveHud.unitSystem,
                        )
                    NaviMapTestHooks.lastRoadSignWarningJson =
                        if (roadSignWarning.active) finalRoadSignJson else "{}"
                    NaviMapTestHooks.lastSchoolProximityWarningJson = schoolFallbackJson
                    NaviMapTestHooks.lastLiveHazardConeM =
                        runCatching { uniffi.navi.liveHazardConeM() }.getOrDefault(300.0)
                    if (!roadSignWarning.active) {
                        val pbf = resolveRegionPbf()
                        val skipGraph =
                            skipLiveGraphWorkDuringForegroundPlan(
                                runCatching { foregroundPlanActive() }
                                    .getOrDefault(planningRoute),
                            )
                        if (pbf != null &&
                            !skipGraph &&
                            speedLimitConeInFlight.compareAndSet(false, true)
                        ) {
                            val fixLat = loc.latitude
                            val fixLon = loc.longitude
                            val heading = headingDeg
                            val prof = profile
                            val currentLimit = driveHud.currentSpeedLimitKmh
                            val unit = driveHud.unitSystem
                            val pbfPath = pbf.absolutePath
                            val cachePath = graphCacheDirForPbf(pbf).absolutePath
                            val elevPath = File(dataDir, "elevation").absolutePath
                            scope.launch {
                                try {
                                    val limitJson =
                                        withContext(Dispatchers.IO) {
                                            runCatching {
                                                uniffi.navi.liveSpeedLimitConeJson(
                                                    pbfPath,
                                                    cachePath,
                                                    elevPath,
                                                    fixLat,
                                                    fixLon,
                                                    heading,
                                                    prof,
                                                    currentLimit,
                                                )
                                            }.getOrDefault("{}")
                                        }
                                    val plate =
                                        runCatching {
                                            org.json.JSONObject(limitJson).optJSONObject("road_sign")
                                        }.getOrNull()
                                    if (plate != null && plate.has("icon_key")) {
                                        val plateJson = plate.toString()
                                        roadSignWarning =
                                            roadSignWarningFromJson(plateJson).copy(
                                                unitSystem = unit,
                                            )
                                        NaviMapTestHooks.lastRoadSignWarningJson = plateJson
                                    }
                                } finally {
                                    speedLimitConeInFlight.set(false)
                                }
                            }
                        }
                    }
                }
                if (!streetFromRoute) {
                    if (speedKmh != null && driveHud.currentSpeedKmh != speedKmh) {
                        val over =
                            OverspeedHud.isOverspeed(
                                speedKmh,
                                driveHud.currentSpeedLimitKmh,
                                speedAccKmh,
                            )
                        driveHud =
                            driveHud.copy(currentSpeedKmh = speedKmh, overspeed = over)
                        NaviMapTestHooks.lastOverspeed = over
                    }
                    // Idle GPS: nearest OSM way (bbox graph), place-index only as fallback.
                    val now = android.os.SystemClock.elapsedRealtime()
                    val movedM =
                        if (
                            lastNearbyStreetLat.isFinite() && lastNearbyStreetLon.isFinite()
                        ) {
                            haversineMApprox(
                                lastNearbyStreetLat,
                                lastNearbyStreetLon,
                                loc.latitude,
                                loc.longitude,
                            )
                        } else {
                            Double.POSITIVE_INFINITY
                        }
                    val due = now - lastNearbyStreetAtMs >= 3_000L || movedM >= 30.0
                    if (due && nearbyStreetInFlight.compareAndSet(false, true)) {
                        lastNearbyStreetAtMs = now
                        lastNearbyStreetLat = loc.latitude
                        lastNearbyStreetLon = loc.longitude
                        val fixLat = loc.latitude
                        val fixLon = loc.longitude
                        val prof = profile
                        val clearIfFar = movedM > 150.0
                        scope.launch {
                            val interim =
                                withContext(Dispatchers.IO) {
                                    val hits =
                                        runCatching {
                                            nearbyPlaces(
                                                resolvePlaceIndexDb().absolutePath,
                                                fixLat,
                                                fixLon,
                                                200.0,
                                                24u,
                                            )
                                        }.getOrDefault(emptyList())
                                    streetLabelFromNearbyPlaces(hits)
                                }
                            if (interim != null && driveHud.currentStreet != interim) {
                                driveHud = driveHud.copy(currentStreet = interim)
                                NaviMapTestHooks.lastCurrentStreet = interim
                            }
                            val nearInfo =
                                withContext(Dispatchers.IO) {
                                    if (skipLiveGraphWorkDuringForegroundPlan(
                                            runCatching { foregroundPlanActive() }
                                                .getOrDefault(false),
                                        )
                                    ) {
                                        return@withContext null
                                    }
                                    val pbf = resolveRegionPbf() ?: return@withContext null
                                    runCatching {
                                        roadNearInfo(
                                            pbf.absolutePath,
                                            graphCacheDirForPbf(pbf).absolutePath,
                                            File(dataDir, "elevation").absolutePath,
                                            fixLat,
                                            fixLon,
                                            prof,
                                            80.0,
                                        )
                                    }.getOrNull()?.takeIf { it.label.isNotBlank() }
                                }
                            nearbyStreetInFlight.set(false)
                            when {
                                nearInfo != null -> {
                                    val over =
                                        OverspeedHud.isOverspeed(
                                            driveHud.currentSpeedKmh ?: speedKmh,
                                            nearInfo.speedLimitKmh,
                                            speedAccKmh,
                                        )
                                    NaviMapTestHooks.lastCurrentStreet = nearInfo.label
                                    NaviMapTestHooks.lastCurrentSpeedLimitKmh =
                                        nearInfo.speedLimitKmh
                                    driveHud =
                                        driveHud.copy(
                                            currentStreet = nearInfo.label,
                                            currentSpeedKmh =
                                                driveHud.currentSpeedKmh ?: speedKmh,
                                            currentSpeedLimitKmh = nearInfo.speedLimitKmh,
                                            overspeed = over,
                                        )
                                    NaviMapTestHooks.lastOverspeed = over
                                }
                                interim == null && clearIfFar && driveHud.currentStreet != null -> {
                                    driveHud =
                                        driveHud.copy(
                                            currentStreet = null,
                                            currentSpeedLimitKmh = null,
                                            overspeed = false,
                                        )
                                    NaviMapTestHooks.lastCurrentStreet = null
                                    NaviMapTestHooks.lastCurrentSpeedLimitKmh = null
                                    NaviMapTestHooks.lastOverspeed = false
                                }
                            }
                        }
                    }
                }
            }
            // Diagnostic GPS (rate-limited in DiagnosticLog). Log before altitude
            // early-returns so a DEM/GPS alt skip does not skip the GPS line.
            if (loc.latitude != 0.0 || loc.longitude != 0.0) {
                val sats =
                    loc.extras
                        ?.getInt("satellites", -1)
                        ?.takeIf { it >= 0 }
                val fixType =
                    when {
                        loc.provider == "navi-route-sim" -> "sim"
                        loc.hasAccuracy() && loc.accuracy <= 20f -> "3D"
                        loc.hasAccuracy() -> "2D"
                        else -> "fix"
                    }
                val altAsl =
                    driveHud.altitudeM
                        ?: NaviMapTestHooks.gpsAltitudeM
                        ?: loc.takeIf { it.hasAltitude() }?.altitude
                DiagnosticLog.logGps(
                    lat = loc.latitude,
                    lon = loc.longitude,
                    altAslM = altAsl,
                    accuracyM = if (loc.hasAccuracy()) loc.accuracy else null,
                    satellites = sats,
                    fixType = fixType,
                    zoom =
                        NaviMapTestHooks.lastCameraZoom
                            .takeIf { it.isFinite() && it > 0.0 }
                            ?: mapState.cameraZoom,
                    pitchDeg =
                        NaviMapTestHooks.lastCameraPitch
                            .takeIf { it.isFinite() },
                    bearingDeg =
                        NaviMapTestHooks.lastCameraBearing
                            .takeIf { it.isFinite() },
                )
                DiagnosticLog.maybeLogSystem(context.filesDir)
            }
            // Test hook overrides live sensor in the poll loop.
            if (NaviMapTestHooks.gpsAltitudeM != null) return
            // Prefer DEM terrain height whenever a tile covers this fix.
            // Decode off-main; GPS altitude is only a stand-in until DEM returns.
            enqueueDemAltitude(loc.latitude, loc.longitude)
            if (demAltitudeReady.get()) {
                return
            }
            if (!loc.hasAltitude()) {
                return
            }
            val alt = loc.altitude
            // AVD / first fixes often report altitude 0.0 with no usable vertical
            // accuracy — leave unset rather than showing a 0 sentinel.
            val usable =
                if (android.os.Build.VERSION.SDK_INT >= 26 && loc.hasVerticalAccuracy()) {
                    loc.verticalAccuracyMeters <= 50f && kotlin.math.abs(alt) > 0.5
                } else {
                    kotlin.math.abs(alt) > 0.5
                }
            if (!usable) {
                return
            }
            driveHud = driveHud.copy(altitudeM = alt)
            NaviMapTestHooks.lastHudAltitudeM = alt
        }
        applyFixRef.set { loc -> applyFix(loc) }
        if (!locationPermGranted) {
            return@DisposableEffect onDispose { }
        }
        val listener = LocationListener { loc -> applyFix(loc) }
        try {
            applyFix(
                lm.getLastKnownLocation(LocationManager.GPS_PROVIDER)
                    ?: lm.getLastKnownLocation(LocationManager.NETWORK_PROVIDER)
                    ?: lm.getLastKnownLocation(LocationManager.PASSIVE_PROVIDER),
            )
            val providers =
                listOf(
                    LocationManager.GPS_PROVIDER,
                    LocationManager.NETWORK_PROVIDER,
                    LocationManager.PASSIVE_PROVIDER,
                )
            for (p in providers) {
                if (lm.isProviderEnabled(p)) {
                    lm.requestLocationUpdates(p, 1_000L, 1f, listener)
                }
            }
        } catch (_: SecurityException) {
            // Permission revoked mid-session; keep stub / last mark.
        }
        onDispose {
            runCatching { lm.removeUpdates(listener) }
        }
    }
    // Refresh DEM altitude when tiles arrive later or the GPS point moves.
    // Do not key on lat/lon: cancelling mid-decode would restart a tens-of-seconds
    // GeoTIFF inflate on every fix.
    LaunchedEffect(locationPermGranted) {
        if (!locationPermGranted) return@LaunchedEffect
        while (isActive) {
            enqueueDemAltitude(mapState.gpsLat, mapState.gpsLon)
            delay(5_000)
        }
    }
    DisposableEffect(Unit) {
        val handler = Handler(Looper.getMainLooper())
        // Edge-trigger test-hook chrome flags so a continuous poll does not
        // overwrite the user's Close (hideSearch=true) every tick.
        val lastHookHideChrome =
            java.util.concurrent.atomic.AtomicBoolean(
                NaviMapTestHooks.hideUiChrome,
            )
        val lastHookHideSearch =
            java.util.concurrent.atomic.AtomicBoolean(
                NaviMapTestHooks.hideSearchChrome,
            )
        hideChrome = NaviMapTestHooks.hideUiChrome
        hideSearch = NaviMapTestHooks.hideSearchChrome
        val routeApplier: (uniffi.navi.CorridorRouteResult) -> Unit = { pending ->
            applyPlannedRoute(pending)
        }
        NaviMapTestHooks.applyRouteHandler = routeApplier
        val runnable =
            object : Runnable {
                override fun run() {
                    try {
                        val pending = NaviMapTestHooks.pendingRoute
                        // Only the resumed activity may consume test-hook routes. A stopped
                        // duplicate MainActivity (pre-singleTask launches) would otherwise
                        // steal pendingRoute and leave the visible map without a polyline.
                        if (pending != null) {
                            val resumed =
                                (context as? androidx.lifecycle.LifecycleOwner)
                                    ?.lifecycle
                                    ?.currentState
                                    ?.isAtLeast(androidx.lifecycle.Lifecycle.State.RESUMED)
                                    ?: false
                            if (resumed) {
                                NaviMapTestHooks.pendingRoute = null
                                applyPlannedRoute(pending)
                            }
                        }
                        if (NaviMapTestHooks.requestClearRoute) {
                            NaviMapTestHooks.requestClearRoute = false
                            clearActiveRoute("Route deleted")
                        }
                        val breakDistReq = NaviMapTestHooks.requestBreakAsDistance
                        if (breakDistReq != null) {
                            NaviMapTestHooks.requestBreakAsDistance = null
                            MapHudPrefs.saveBreakAsDistance(context, breakDistReq)
                            driveHud = driveHud.copy(breakAsDistance = breakDistReq)
                        }
                        val cam = NaviMapTestHooks.pendingCamera
                        if (cam != null) {
                            NaviMapTestHooks.pendingCamera = null
                            mapState =
                                mapState.copy(
                                    followGps = false,
                                    cameraLat = cam.first,
                                    cameraLon = cam.second,
                                    cameraZoom = cam.third,
                                    layerEpoch = mapState.layerEpoch + 1,
                                )
                            NaviMapTestHooks.followGps = false
                        }
                        if (NaviMapTestHooks.requestRecenterGps && !NaviMapTestHooks.disableGpsFollow) {
                            NaviMapTestHooks.requestRecenterGps = false
                            if (mapState.gpsLat != 0.0 || mapState.gpsLon != 0.0) {
                                mapState =
                                    mapState.copy(
                                        followGps = true,
                                        cameraLat = mapState.gpsLat,
                                        cameraLon = mapState.gpsLon,
                                        layerEpoch = mapState.layerEpoch + 1,
                                    )
                                NaviMapTestHooks.followGps = true
                            }
                        }
                        val rotReq = NaviMapTestHooks.requestRotationMode
                        if (rotReq != null) {
                            NaviMapTestHooks.requestRotationMode = null
                            cancelPendingRotationSnap()
                            manualRotationSticky = false
                            NaviMapTestHooks.manualRotationOverrideActive = false
                            driveHud = driveHud.copy(rotationMode = rotReq)
                            NaviMapTestHooks.lastRotationMode = rotReq
                            reassertModeBearing(force = true)
                        }
                        val pendingBearing = NaviMapTestHooks.pendingBearing
                        if (pendingBearing != null) {
                            NaviMapTestHooks.pendingBearing = null
                            if (NaviMapTestHooks.applyBearingToMap) {
                                mapState = mapState.copy(cameraBearing = pendingBearing)
                                bearingApplyEpoch += 1
                            }
                            NaviMapTestHooks.lastCameraBearing = pendingBearing
                        }
                        val tracks = NaviMapTestHooks.pendingTracks
                        if (tracks != null) {
                            NaviMapTestHooks.pendingTracks = null
                            mapState =
                                mapState.copy(
                                    tracks = tracks,
                                    layerEpoch = mapState.layerEpoch + 1,
                                )
                            NaviMapTestHooks.lastTrackIds = tracks.map { it.id }
                            NaviMapTestHooks.tracksEpoch += 1
                        }
                        val approach = NaviMapTestHooks.pendingApproachGuidance
                        if (approach != null) {
                            NaviMapTestHooks.pendingApproachGuidance = null
                            approachGuidance = approach
                            NaviMapTestHooks.lastApproachPhase = approachUiPhase(approach)
                            NaviMapTestHooks.lastApproachIconKey = approach.iconKey
                        }
                        if (NaviMapTestHooks.requestStartRouteSimulation) {
                            if (routeSamples.size >= 2) {
                                NaviMapTestHooks.requestStartRouteSimulation = false
                                startRouteSimulation()
                            }
                        }
                        if (NaviMapTestHooks.requestStartLiveConeSimulation) {
                            val coords = NaviMapTestHooks.liveConeSimCoordsJson
                            if (!coords.isNullOrBlank()) {
                                NaviMapTestHooks.requestStartLiveConeSimulation = false
                                // No planned route: clear progress tracker so the live cone path runs.
                                progressTrackerRef.set(null)
                                NaviMapTestHooks.lastRoutePolyline = ""
                                routeSchoolPoisJson = "[]"
                                val samplesJson =
                                    runCatching {
                                        uniffi.navi.simSamplesJsonFromLatLon(
                                            coords,
                                            NaviMapTestHooks.liveConeSimSpeedKmh,
                                        )
                                    }.getOrDefault("[]")
                                routeSamples = parseRouteSimSamples(samplesJson)
                                NaviMapTestHooks.lastSimSamplesJson = samplesJson
                                if (routeSamples.size >= 2) {
                                    startRouteSimulation()
                                }
                            }
                        }
                        if (NaviMapTestHooks.requestStopRouteSimulation) {
                            NaviMapTestHooks.requestStopRouteSimulation = false
                            stopRouteSimulation()
                        }
                        if (NaviMapTestHooks.requestPrepareRouteSimulation) {
                            NaviMapTestHooks.requestPrepareRouteSimulation = false
                            prepareRouteSimulation()
                        }
                        val inject = NaviMapTestHooks.pendingInjectFixLatLon
                        if (inject != null) {
                            NaviMapTestHooks.pendingInjectFixLatLon = null
                            // Stop corridor playback so the off-route fix is not
                            // overwritten by the next on-route sample tick.
                            if (simulatingRef.get()) {
                                stopRouteSimulation()
                            }
                            // Keep device GPS from immediately undoing the inject.
                            NaviMapTestHooks.ignoreLiveGpsFixes = true
                            val loc =
                                android.location.Location("navi-test-inject").apply {
                                    latitude = inject.first
                                    longitude = inject.second
                                    time = System.currentTimeMillis()
                                    accuracy = 5f
                                    NaviMapTestHooks.pendingInjectFixSpeedKmh?.let { kmh ->
                                        speed = (kmh / 3.6).toFloat()
                                    }
                                }
                            NaviMapTestHooks.pendingInjectFixSpeedKmh = null
                            applyFixRef.get().invoke(loc)
                        }
                        val hikeAns = NaviMapTestHooks.requestHikingRerouteAnswer
                        if (hikeAns != null && showHikingReroutePrompt) {
                            NaviMapTestHooks.requestHikingRerouteAnswer = null
                            showHikingReroutePrompt = false
                            NaviMapTestHooks.hikingReroutePromptVisible = false
                            if (hikeAns) {
                                startRerouteFromCurrent(mapState.gpsLat, mapState.gpsLon)
                            } else {
                                offRouteCoordinator.suppressUntilOnRoute()
                                status = "Kept original route"
                            }
                        }
                        // Always mirror sim flag into Compose state (hook/ref updates
                        // alone do not invalidate composition).
                        simulating = simulatingRef.get()
                        NaviMapTestHooks.reroutingActive = recalculatingRoute
                        NaviMapTestHooks.hikingReroutePromptVisible = showHikingReroutePrompt
                        val seekCum = NaviMapTestHooks.requestSimSeekCumM
                        if (seekCum != null) {
                            NaviMapTestHooks.requestSimSeekCumM = null
                            if (routeSimulator == null && routeSamples.size >= 2) {
                                if (progressTrackerRef.get() != null) {
                                    prepareRouteSimulation()
                                } else {
                                    // Route-independent live-cone playback: no progress tracker.
                                    routeSimulator =
                                        RouteSimulator(
                                            scope = scope,
                                            samples = routeSamples,
                                            onFix = { loc -> applyFixRef.get().invoke(loc) },
                                            onSample = { s ->
                                                NaviMapTestHooks.lastSimSpeedKmh = s.speedKmh
                                                NaviMapTestHooks.lastSimHighway = s.highway
                                                NaviMapTestHooks.lastSimMaxspeedPosted =
                                                    s.maxspeedPosted
                                            },
                                            onFinished = {},
                                        )
                                }
                            }
                            // Mark as simulating so LM fixes stay suppressed during seeks.
                            simulating = true
                            simulatingRef.set(true)
                            NaviMapTestHooks.simulatingActive = true
                            routeSimulator?.seekToCumM(seekCum)
                        }
                        val wantHideChrome = NaviMapTestHooks.hideUiChrome
                        if (wantHideChrome != lastHookHideChrome.get()) {
                            hideChrome = wantHideChrome
                            lastHookHideChrome.set(wantHideChrome)
                        }
                        val wantHideSearch = NaviMapTestHooks.hideSearchChrome
                        if (wantHideSearch != lastHookHideSearch.get()) {
                            hideSearch = wantHideSearch
                            lastHookHideSearch.set(wantHideSearch)
                            if (wantHideSearch) {
                                showTools = false
                            }
                        }
                        // Continuous enforce: chip typing can reopen search after an edge hide.
                        if (wantHideSearch && !hideSearch) {
                            hideSearch = true
                            showTools = false
                        }
                        if (NaviMapTestHooks.requestCloseTools) {
                            NaviMapTestHooks.requestCloseTools = false
                            showTools = false
                        }
                        if (NaviMapTestHooks.requestOpenTools) {
                            NaviMapTestHooks.requestOpenTools = false
                            showTools = true
                            hideSearch = false
                            hideChrome = false
                        }
                        NaviMapTestHooks.toolsOpen = showTools
                        if (NaviMapTestHooks.requestOpenDriveSettings) {
                            NaviMapTestHooks.requestOpenDriveSettings = false
                            showDriveSettings = true
                            showMapSettings = false
                            NaviMapTestHooks.driveSettingsOpen = true
                        }
                        if (NaviMapTestHooks.requestOpenMapSettings) {
                            NaviMapTestHooks.requestOpenMapSettings = false
                            showMapSettings = true
                            showDriveSettings = false
                            NaviMapTestHooks.mapSettingsOpen = true
                        }
                        val tripReq = NaviMapTestHooks.requestShowTripEta
                        if (tripReq != null) {
                            NaviMapTestHooks.requestShowTripEta = null
                            driveHud =
                                driveHud.copy(
                                    showTripEta = tripReq,
                                    tripEtaMinutes =
                                        when {
                                            tripReq && driveHud.tripEtaMinutes == null -> 95.0
                                            else -> driveHud.tripEtaMinutes
                                        },
                                )
                            NaviMapTestHooks.lastShowTripEta = tripReq
                        }
                        val opt3dReq = NaviMapTestHooks.requestOptIn3d
                        if (opt3dReq != null) {
                            NaviMapTestHooks.requestOptIn3d = null
                            driveHud = driveHud.copy(optIn3d = opt3dReq)
                            MapHudPrefs.saveOptIn3d(context, opt3dReq)
                            styleEpoch += 1
                        }
                        val tiltReq = NaviMapTestHooks.requestCameraTiltDeg
                        if (tiltReq != null) {
                            NaviMapTestHooks.requestCameraTiltDeg = null
                            val next =
                                if (driveHud.vulkanAvailable) {
                                    MapHudPrefs.snapTilt(tiltReq)
                                } else {
                                    0.0
                                }
                            driveHud = driveHud.copy(cameraTiltDeg = next)
                            MapHudPrefs.saveCameraTiltDeg(context, next)
                            styleEpoch += 1
                        }
                        if (NaviMapTestHooks.requestClearSearch) {
                            NaviMapTestHooks.requestClearSearch = false
                            query = ""
                            hits = emptyList()
                            NaviMapTestHooks.lastSearchHitCount = 0
                            NaviMapTestHooks.lastSearchQuery = ""
                            NaviMapTestHooks.lastSearchHitNames = emptyList()
                        }
                        val pendingHit = NaviMapTestHooks.pendingApplyHit
                        if (pendingHit != null) {
                            NaviMapTestHooks.pendingApplyHit = null
                            val wp =
                                Waypoint(
                                    name = pendingHit.name,
                                    lat = pendingHit.lat,
                                    lon = pendingHit.lon,
                                )
                            when (searchTarget) {
                                SearchTarget.From -> fromPoint = wp
                                SearchTarget.To -> toPoint = wp
                                SearchTarget.Via -> viaPoints = viaPoints + wp
                            }
                            mapState =
                                mapState.copy(
                                    followGps = false,
                                    cameraLat = pendingHit.lat,
                                    cameraLon = pendingHit.lon,
                                    cameraZoom = 12.0,
                                    poiLat = pendingHit.lat,
                                    poiLon = pendingHit.lon,
                                    poiName = pendingHit.name,
                                    layerEpoch = mapState.layerEpoch + 1,
                                )
                            NaviMapTestHooks.followGps = false
                            query = pendingHit.name
                            hits = emptyList()
                            status =
                                userFacingStatus(
                                    "Set ${searchTarget.name.lowercase()}: ${pendingHit.name}",
                                )
                        }
                        val breakReq = NaviMapTestHooks.requestBreakReminders
                        if (breakReq != null) {
                            NaviMapTestHooks.requestBreakReminders = null
                            driveHud = driveHud.copy(breakRemindersEnabled = breakReq)
                            NaviMapTestHooks.lastBreakRemindersEnabled = breakReq
                        }
                        val profileReq = NaviMapTestHooks.requestTravelProfile
                        if (profileReq != null) {
                            NaviMapTestHooks.requestTravelProfile = null
                            profile = profileReq
                            ecoEnabled = ecoModeDefault(profileReq)
                            driveHud = driveHud.copy(ecoActive = ecoEnabled)
                            if (mapState.polyline.isNotBlank() && driveHud.breakRemindersEnabled) {
                                val intervalH =
                                    breakIntervalHoursForProfile(
                                        dataDir.absolutePath,
                                        profileReq,
                                    )
                                driveHud =
                                    driveHud.copy(
                                        minutesToBreak =
                                            ((intervalH - drivingHoursSinceBreak) * 60.0)
                                                .coerceAtLeast(0.0),
                                    )
                            }
                            status = "Profile: ${profileReq.name.lowercase()}"
                        }
                        val hookAlt = NaviMapTestHooks.gpsAltitudeM
                        if (hookAlt != null && driveHud.altitudeM != hookAlt) {
                            driveHud = driveHud.copy(altitudeM = hookAlt)
                        }
                        NaviMapTestHooks.pendingCurrentStreet?.let { street ->
                            NaviMapTestHooks.pendingCurrentStreet = null
                            driveHud = driveHud.copy(currentStreet = street)
                            NaviMapTestHooks.lastCurrentStreet = street
                        }
                        NaviMapTestHooks.lastReportedLayerCount = mapLayerCount
                        NaviMapTestHooks.driveSettingsOpen = showDriveSettings
                        NaviMapTestHooks.mapSettingsOpen = showMapSettings
                        NaviMapTestHooks.lastRotationMode = driveHud.rotationMode
                        // Do not overwrite lastCameraZoom / lat / lon from Compose state here —
                        // MapLibre camera-idle is the source of truth (user pan/pinch/double-tap).
                        // Same for bearing while a manual-rotate override/snap window is active:
                        // camera-idle (and onManualRotateEnded) own lastCameraBearing then.
                        if (!(manualRotationSticky || pendingRotationSnap != null)) {
                            NaviMapTestHooks.lastCameraBearing = mapState.cameraBearing
                        }
                        NaviMapTestHooks.lastBreakRemindersEnabled = driveHud.breakRemindersEnabled
                        NaviMapTestHooks.lastShowTripEta = driveHud.showTripEta
                        NaviMapTestHooks.lastMinutesToBreak = driveHud.minutesToBreak
                        NaviMapTestHooks.lastCurrentStreet = driveHud.currentStreet
                        NaviMapTestHooks.lastBreakHudVisible = formatBreakHudLine(
                            routePlanned = mapState.polyline.isNotBlank(),
                            breakRemindersEnabled = driveHud.breakRemindersEnabled,
                            minutesToBreak = driveHud.minutesToBreak,
                            breakAsDistance = driveHud.breakAsDistance,
                            unitSystem = driveHud.unitSystem,
                        ) != null
                        NaviMapTestHooks.lastHudAltitudeM = driveHud.altitudeM

                        val snapReq = NaviMapTestHooks.requestSnapRotationBack
                        if (snapReq != null) {
                            NaviMapTestHooks.requestSnapRotationBack = null
                            driveHud = driveHud.copy(snapRotationBackToMode = snapReq)
                            MapHudPrefs.saveSnapRotationBack(context, snapReq)
                            NaviMapTestHooks.lastSnapRotationBack = snapReq
                            if (snapReq) {
                                manualRotationSticky = false
                                reassertModeBearing(force = true)
                            }
                        }
                        val simRot = NaviMapTestHooks.requestSimulateManualRotateDeg
                        if (simRot != null) {
                            NaviMapTestHooks.requestSimulateManualRotateDeg = null
                            // Apply via the same path as a real rotate-gesture end so the
                            // pending-snap / sticky flags are set before the next poll can
                            // reassert the mode bearing and erase the simulated override.
                            if (NaviMapTestHooks.applyBearingToMap) {
                                mapState = mapState.copy(cameraBearing = simRot)
                                bearingApplyEpoch += 1
                            }
                            onManualRotateEnded(simRot)
                        }
                        if (!(manualRotationSticky && !driveHud.snapRotationBackToMode) &&
                            pendingRotationSnap == null
                        ) {
                            val targetBearing = modeTargetBearing()
                            if (targetBearing != null) {
                                val cur = mapState.cameraBearing
                                val mapBr = NaviMapTestHooks.lastCameraBearing
                                if (kotlin.math.abs(cur - targetBearing) > 0.05 ||
                                    kotlin.math.abs(mapBr - targetBearing) > 0.05
                                ) {
                                    reassertModeBearing(force = true)
                                }
                            }
                        }
                        NaviMapTestHooks.lastSnapRotationBack = driveHud.snapRotationBackToMode
                        NaviMapTestHooks.manualRotationOverrideActive =
                            manualRotationSticky ||
                            pendingRotationSnap != null
                    } catch (e: Exception) {
                        android.util.Log.e("HudVerification", "hook poll error", e)
                    }
                    handler.postDelayed(this, 250)
                }
            }
        handler.post(runnable)
        onDispose {
            handler.removeCallbacks(runnable)
            if (NaviMapTestHooks.applyRouteHandler === routeApplier) {
                NaviMapTestHooks.applyRouteHandler = null
            }
        }
    }

    fun applyHit(
        hit: PlaceHit,
        target: SearchTarget = searchTarget,
    ) {
        val label = placeHitDisplayLabel(hit)
        val (street, house, post) = parseAddressDisplayLines(combined = hit.name)
        val wp =
            Waypoint(
                name = label,
                lat = hit.lat,
                lon = hit.lon,
                street = street,
                houseNumber = house,
                postcode = post,
            )
        when (target) {
            SearchTarget.From -> fromPoint = wp
            SearchTarget.To -> toPoint = wp
            SearchTarget.Via -> viaPoints = viaPoints + wp
        }
        mapState =
            mapState.copy(
                followGps = false,
                cameraLat = hit.lat,
                cameraLon = hit.lon,
                cameraZoom = 12.0,
                poiLat = hit.lat,
                poiLon = hit.lon,
                poiName = label,
                layerEpoch = mapState.layerEpoch + 1,
            )
        NaviMapTestHooks.followGps = false
        query = label
        hits = emptyList()
        status = userFacingStatus("Set ${target.name.lowercase()}: $label")
    }

    fun applyMarkAs(
        target: SearchTarget,
        pending: MapMarkPending,
    ) {
        searchTarget = target
        applyHit(
            PlaceHit(
                osmId = 0L,
                name = pending.suggestedName,
                kind = pending.kind,
                lat = pending.lat,
                lon = pending.lon,
                subArea = "",
                municipality = "",
            ),
            target = target,
        )
        mapMarkPending = null
    }

    fun runSearch(q: String) {
        searchJob?.cancel()
        val trimmed = q.trim()
        if (trimmed.length < 2) {
            hits = emptyList()
            searchIndexHint = ""
            return
        }
        // Accept WGS84 "lat, lon" for From / Via / To without place FTS.
        parseLatLonQuery(trimmed)?.let { (lat, lon) ->
            val name = formatCoordWaypointName(lat, lon)
            hits =
                listOf(
                    PlaceHit(
                        osmId = 0L,
                        name = name,
                        kind = "coordinate",
                        lat = lat,
                        lon = lon,
                        subArea = "",
                        municipality = "",
                    ),
                )
            NaviMapTestHooks.lastSearchHitCount = 1
            NaviMapTestHooks.lastSearchQuery = trimmed
            NaviMapTestHooks.lastSearchHitNames = listOf(name)
            searchBusy = false
            searchIndexHint = ""
            return
        }
        searchBusy = true
        searchJob =
            scope.launch {
                delay(200)
                val dbPath = resolvePlaceIndexDb().absolutePath
                val list =
                    withContext(Dispatchers.IO) {
                        searchPlaces(dbPath, trimmed, 20u)
                    }
                hits =
                    when (searchMode) {
                        SearchMode.Place ->
                            list
                                .filter {
                                    val k = it.kind.lowercase()
                                    k.contains("place") ||
                                        k.contains("amenity") ||
                                        k.contains("tourism") ||
                                        k.contains("peak") ||
                                        k.contains("hut") ||
                                        k.contains("natural")
                                }.ifEmpty { list }
                        SearchMode.Address ->
                            list
                                .filter {
                                    val k = it.kind.lowercase()
                                    k.contains("highway") || k.contains("place") || k.contains("addr")
                                }.ifEmpty { list }
                    }
                val hasEntries =
                    withContext(Dispatchers.IO) {
                        runCatching { placeIndexHasEntries(dbPath) }.getOrDefault(false)
                    }
                searchIndexHint =
                    placeSearchBuildingMessage(
                        hits.isEmpty(),
                        hasEntries,
                        PlaceIndexBackground.isRunning(),
                    ).orEmpty()
                NaviMapTestHooks.lastSearchHitCount = hits.size
                NaviMapTestHooks.lastSearchQuery = trimmed
                NaviMapTestHooks.lastSearchHitNames = hits.map { placeHitDisplayLabel(it) }
                NaviMapTestHooks.lastSearchIndexBuildingHint = searchIndexHint
                searchBusy = false
            }
    }

    // No planned corridor => clear approach + break countdown.
    // Current street may still update from GPS + place index (see docs/current-street.md).
    LaunchedEffect(mapState.polyline) {
        if (mapState.polyline.isBlank()) {
            if (approachGuidance.active) {
                approachGuidance = ApproachGuidanceState()
                NaviMapTestHooks.lastApproachPhase = ApproachUiPhase.Hidden
            }
            if (driveHud.minutesToBreak != null) {
                driveHud = driveHud.copy(minutesToBreak = null)
            }
        } else if (driveHud.minutesToBreak == null && driveHud.breakRemindersEnabled) {
            runCatching {
                val intervalH = breakIntervalHoursForProfile(dataDir.absolutePath, profile)
                val minsLeft =
                    ((intervalH - drivingHoursSinceBreak) * 60.0).coerceAtLeast(0.0)
                driveHud = driveHud.copy(minutesToBreak = minsLeft)
            }
        }
    }

    LaunchedEffect(Unit) {
        while (isActive) {
            val want = simulatingRef.get()
            if (want != simulating) {
                simulating = want
            }
            delay(100)
        }
    }

    Box(modifier = Modifier.fillMaxSize()) {
        CorridorMapView(
            state = mapState,
            dataDir = dataDir,
            prefer3d = driveHud.optIn3d,
            contoursEnabled = driveHud.contoursEnabled,
            cameraTiltDeg = driveHud.cameraTiltDeg,
            vulkanAvailable = driveHud.vulkanAvailable,
            unitSystem = driveHud.unitSystem,
            styleEpoch = styleEpoch,
            bearingEpoch = bearingApplyEpoch,
            modifier = Modifier.fillMaxSize(),
            onLayerCount = { mapLayerCount = it },
            onUserPan = {
                if (mapState.followGps) {
                    mapState = mapState.copy(followGps = false)
                    NaviMapTestHooks.followGps = false
                    DiagnosticLog.logToggle("follow_gps", false)
                }
            },
            onMapLongPress = { lat, lon ->
                scope.launch {
                    val (name, kind) = resolveLabelAt(lat, lon)
                    mapMarkPending =
                        MapMarkPending(
                            lat = lat,
                            lon = lon,
                            suggestedName = name,
                            kind = kind,
                        )
                    mapState =
                        mapState.copy(
                            followGps = false,
                            cameraLat = lat,
                            cameraLon = lon,
                            poiLat = lat,
                            poiLon = lon,
                            poiName = name,
                            layerEpoch = mapState.layerEpoch + 1,
                        )
                    NaviMapTestHooks.followGps = false
                    NaviMapTestHooks.lastMapLongPressLat = lat
                    NaviMapTestHooks.lastMapLongPressLon = lon
                    NaviMapTestHooks.mapLongPressCount += 1
                    status = "Marked: $name"
                }
            },
            onUserRotate = { bearing -> onManualRotateEnded(bearing) },
            onCameraIdleTarget = { lat, lon, zoom ->
                NaviMapTestHooks.lastCameraLat = lat
                NaviMapTestHooks.lastCameraLon = lon
                NaviMapTestHooks.lastCameraZoom = zoom
                NaviMapTestHooks.followGps = mapState.followGps
                // Keep Compose camera in sync with the real MapLibre view after
                // gestures so HUD zoom does not invent a stale GPS center.
                if (!mapState.followGps) {
                    val cur = mapState
                    if (cur.cameraLat != lat || cur.cameraLon != lon || cur.cameraZoom != zoom) {
                        mapState =
                            cur.copy(
                                cameraLat = lat,
                                cameraLon = lon,
                                cameraZoom = zoom,
                            )
                    }
                }
            },
            onStyleNote = { note ->
                if (!note.isNullOrBlank()) status = note
            },
            on3dFailed = {
                driveHud = driveHud.copy(optIn3d = false)
                MapHudPrefs.saveOptIn3d(context, false)
                status = "3D unavailable on this device; using 2D Liberty"
                styleEpoch += 1
            },
        )

        mapMarkPending?.let { pending ->
            if (!showSavePlaceDialog) {
                MapMarkActionSheet(
                    pending = pending,
                    onSetFrom = { applyMarkAs(SearchTarget.From, pending) },
                    onSetVia = { applyMarkAs(SearchTarget.Via, pending) },
                    onSetTo = { applyMarkAs(SearchTarget.To, pending) },
                    onSavePlace = {
                        savePlaceDraftName = pending.suggestedName
                        showSavePlaceDialog = true
                    },
                    onCancel = { mapMarkPending = null },
                )
            }
        }
        if (showSavePlaceDialog) {
            val pending = mapMarkPending
            SavePlaceNameDialog(
                name = savePlaceDraftName,
                onNameChange = { savePlaceDraftName = it },
                onConfirm = {
                    val p = pending
                    if (p == null) {
                        showSavePlaceDialog = false
                        return@SavePlaceNameDialog
                    }
                    val report =
                        saveNamedPlace(
                            dataDir.absolutePath,
                            savePlaceDraftName.trim(),
                            p.lat,
                            p.lon,
                            p.kind,
                        )
                    refreshPlaces()
                    showSavePlaceDialog = false
                    mapMarkPending = null
                    status =
                        if (report.startsWith("PASS")) {
                            "Saved place: ${savePlaceDraftName.trim()}"
                        } else {
                            report
                        }
                    showPlacesPanel = true
                    hideSearch = false
                },
                onCancel = { showSavePlaceDialog = false },
            )
        }
        if (renamePlaceId != null) {
            SavePlaceNameDialog(
                name = renamePlaceDraft,
                onNameChange = { renamePlaceDraft = it },
                onConfirm = {
                    val id = renamePlaceId ?: return@SavePlaceNameDialog
                    val ok = renameSavedPlace(dataDir.absolutePath, id, renamePlaceDraft.trim())
                    refreshPlaces()
                    renamePlaceId = null
                    status = if (ok) "Renamed saved place" else "Could not rename place"
                },
                onCancel = { renamePlaceId = null },
            )
        }

        SimulatingBannerOverlay(
            active = simulating,
            modifier =
                Modifier
                    .align(Alignment.TopCenter)
                    .zIndex(6f)
                    .padding(top = 64.dp),
        )
        RecalculatingRouteBanner(
            active = planningRoute || recalculatingRoute,
            title = if (recalculatingRoute) "Recalculating route…" else "Planning route…",
            onCancel = {
                val wasReroute = recalculatingRoute
                planAbort.set(true)
                cancelInFlightPlan()
                recalculatingRoute = false
                planningRoute = false
                NaviMapTestHooks.reroutingActive = false
                routePlanProgress = ""
                if (wasReroute) {
                    offRouteCoordinator.suppressUntilOnRoute()
                    status = "Kept original route"
                } else {
                    status = "Planning cancelled"
                }
            },
            modifier =
                Modifier
                    .align(Alignment.TopCenter)
                    .zIndex(7f)
                    .padding(top = if (simulating) 108.dp else 64.dp),
        )
        if (showSpeedCameraPrompt) {
            AlertDialog(
                onDismissRequest = {
                    showSpeedCameraPrompt = false
                    MapHudPrefs.saveSpeedCameraPromptShown(context, true)
                    MapHudPrefs.saveSpeedCameraOptIn(context, false)
                    speedCameraOptIn = false
                },
                title = { Text("Speed camera warnings") },
                text = {
                    Text(
                        "Show nearby speed-camera warnings while driving? " +
                            "Display only — cameras are never used to change routes. " +
                            "Data comes from OpenStreetMap and may be incomplete. " +
                            "Not available in all countries.",
                    )
                },
                confirmButton = {
                    TextButton(
                        onClick = {
                            showSpeedCameraPrompt = false
                            MapHudPrefs.saveSpeedCameraPromptShown(context, true)
                            MapHudPrefs.saveSpeedCameraOptIn(context, true)
                            speedCameraOptIn = true
                        },
                        modifier = Modifier.testTag("btn_speed_camera_opt_in_yes"),
                    ) { Text("Enable") }
                },
                dismissButton = {
                    TextButton(
                        onClick = {
                            showSpeedCameraPrompt = false
                            MapHudPrefs.saveSpeedCameraPromptShown(context, true)
                            MapHudPrefs.saveSpeedCameraOptIn(context, false)
                            speedCameraOptIn = false
                        },
                        modifier = Modifier.testTag("btn_speed_camera_opt_in_no"),
                    ) { Text("Not now") }
                },
            )
        }
        if (showHikingReroutePrompt) {
            AlertDialog(
                onDismissRequest = {
                    showHikingReroutePrompt = false
                    NaviMapTestHooks.hikingReroutePromptVisible = false
                    offRouteCoordinator.suppressUntilOnRoute()
                    status = "Kept original route"
                },
                title = { Text("Off trail") },
                text = {
                    Text(
                        "You left the planned path. Recalculate from here? " +
                            "This can take several seconds on this device.",
                    )
                },
                confirmButton = {
                    TextButton(
                        onClick = {
                            showHikingReroutePrompt = false
                            NaviMapTestHooks.hikingReroutePromptVisible = false
                            startRerouteFromCurrent(mapState.gpsLat, mapState.gpsLon)
                        },
                        modifier = Modifier.testTag("btn_hiking_reroute_yes"),
                    ) { Text("Recalculate") }
                },
                dismissButton = {
                    TextButton(
                        onClick = {
                            showHikingReroutePrompt = false
                            NaviMapTestHooks.hikingReroutePromptVisible = false
                            offRouteCoordinator.suppressUntilOnRoute()
                            status = "Kept original route"
                        },
                        modifier = Modifier.testTag("btn_hiking_reroute_no"),
                    ) { Text("Keep route") }
                },
            )
        }
        missingCoveragePrompt?.let { missing ->
            AlertDialog(
                onDismissRequest = {
                    missingCoveragePrompt = null
                    NaviMapTestHooks.missingCoveragePromptVisible = false
                    status = "Download cancelled — set another destination or download a region in Tools"
                },
                title = { Text("Map data needed") },
                text = { Text(missing.message) },
                confirmButton = {
                    TextButton(
                        onClick = {
                            val path = missing.suggestedGeofabrikPath
                            missingCoveragePrompt = null
                            NaviMapTestHooks.missingCoveragePromptVisible = false
                            selectedGeofabrikPath = path
                            downloadContinent = GeofabrikDownloadCatalog.continentForPath(path)
                            val countryHit = GeofabrikDownloadCatalog.findByPath(path)
                            downloadScopeCountry =
                                countryHit != null &&
                                countryHit.path == path.trim().trim('/')
                            showTools = true
                            startRegionDownload(path)
                        },
                        modifier = Modifier.testTag("btn_missing_coverage_download"),
                    ) { Text("Download ${RegionCoverage.displayName(missing.suggestedGeofabrikPath)}") }
                },
                dismissButton = {
                    TextButton(
                        onClick = {
                            missingCoveragePrompt = null
                            NaviMapTestHooks.missingCoveragePromptVisible = false
                            status = "Download cancelled — no route planned"
                        },
                        modifier = Modifier.testTag("btn_missing_coverage_dismiss"),
                    ) { Text("Not now") }
                },
            )
        }

        Column(
            modifier =
                Modifier
                    .align(Alignment.TopCenter)
                    .fillMaxWidth()
                    .zIndex(1f)
                    .windowInsetsPadding(
                        WindowInsets.safeDrawing.only(
                            WindowInsetsSides.Top + WindowInsetsSides.Horizontal,
                        ),
                    ).padding(10.dp)
                    .heightIn(max = 520.dp)
                    .verticalScroll(rememberScrollState()),
        ) {
            if (!hideChrome) {
                TopDriveHud(
                    state =
                        driveHud.copy(
                            ecoActive = ecoEnabled,
                        ),
                    expanded = showMapSettings,
                    onToggleExpanded = {
                        showMapSettings = !showMapSettings
                        if (showMapSettings) showDriveSettings = false
                    },
                    modifier = Modifier.padding(bottom = 8.dp),
                )
                ApproachInstructionBox(
                    state = approachGuidance,
                    iconsDir = iconsDir.absolutePath,
                    routePlanned = mapState.polyline.isNotBlank(),
                    modifier =
                        Modifier
                            .align(Alignment.Start)
                            .padding(bottom = 8.dp),
                )
                SpeedCameraWarningBox(
                    state = speedCameraWarning,
                    iconsDir = iconsDir.absolutePath,
                    modifier =
                        Modifier
                            .align(Alignment.Start)
                            .padding(bottom = 8.dp),
                )
                RoadSignWarningBox(
                    state = roadSignWarning,
                    iconsDir = iconsDir.absolutePath,
                    modifier =
                        Modifier
                            .align(Alignment.Start)
                            .padding(bottom = 8.dp),
                )
                if (!hideSearch) {
                    Surface(
                        shape = RoundedCornerShape(12.dp),
                        tonalElevation = 4.dp,
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .testTag("search_chrome"),
                    ) {
                        Column(modifier = Modifier.padding(10.dp)) {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.SpaceBetween,
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                    // Shared query/hits are not per-chip drafts — clear when
                                    // switching so a resolved GPS/place label from From does
                                    // not linger visually on To/Via (and vice versa).
                                    fun selectSearchTarget(next: SearchTarget) {
                                        if (searchTarget == next) return
                                        searchTarget = next
                                        query = ""
                                        hits = emptyList()
                                    }
                                    FilterChip(
                                        selected = searchTarget == SearchTarget.From,
                                        onClick = { selectSearchTarget(SearchTarget.From) },
                                        label = { Text("From") },
                                        modifier = Modifier.testTag("chip_from"),
                                    )
                                    FilterChip(
                                        selected = searchTarget == SearchTarget.To,
                                        onClick = { selectSearchTarget(SearchTarget.To) },
                                        label = { Text("To") },
                                        modifier = Modifier.testTag("chip_to"),
                                    )
                                    FilterChip(
                                        selected = searchTarget == SearchTarget.Via,
                                        onClick = { selectSearchTarget(SearchTarget.Via) },
                                        label = { Text("Via") },
                                        modifier = Modifier.testTag("chip_via"),
                                    )
                                    FilterChip(
                                        selected = searchMode == SearchMode.Place,
                                        onClick = {
                                            searchMode = SearchMode.Place
                                            runSearch(query)
                                        },
                                        label = { Text("Place") },
                                    )
                                    FilterChip(
                                        selected = searchMode == SearchMode.Address,
                                        onClick = {
                                            searchMode = SearchMode.Address
                                            runSearch(query)
                                        },
                                        label = { Text("Address") },
                                    )
                                }
                                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                                    TextButton(
                                        onClick = { showTools = !showTools },
                                        modifier = Modifier.testTag("btn_tools"),
                                    ) {
                                        Text(if (showTools) "Hide tools" else "Tools")
                                    }
                                    TextButton(
                                        onClick = {
                                            hideSearch = true
                                            showTools = false
                                            status = "Route planning closed"
                                        },
                                        modifier = Modifier.testTag("btn_close_search"),
                                    ) {
                                        Text("Close")
                                    }
                                }
                            }
                            Text(
                                "From: ${fromPoint?.name ?: "(unset)"}  |  To: ${toPoint.name.ifBlank { "(unset)" }}  |  Via: ${
                                    if (viaPoints.isEmpty()) "(none)" else viaPoints.joinToString(" → ") { it.name }
                                }",
                                style = MaterialTheme.typography.bodySmall,
                                modifier = Modifier.testTag("search_waypoints_summary"),
                            )
                            OutlinedTextField(
                                value = query,
                                onValueChange = {
                                    query = it
                                    runSearch(it)
                                },
                                modifier =
                                    Modifier
                                        .fillMaxWidth()
                                        .testTag("field_search"),
                                singleLine = true,
                                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                                keyboardActions =
                                    KeyboardActions(
                                        onDone = {
                                            hits.firstOrNull()?.let { applyHit(it) }
                                        },
                                    ),
                                placeholder = {
                                    Text(
                                        when (searchMode) {
                                            SearchMode.Place -> "Place, hut, or lat, lon"
                                            SearchMode.Address -> "Road, settlement, or lat, lon"
                                        },
                                    )
                                },
                            )
                            if (searchBusy) {
                                Text("Searching...", style = MaterialTheme.typography.bodySmall)
                            }
                            if (searchIndexHint.isNotBlank() && hits.isEmpty() && !searchBusy) {
                                Text(
                                    searchIndexHint,
                                    style = MaterialTheme.typography.bodySmall,
                                    modifier =
                                        Modifier
                                            .fillMaxWidth()
                                            .testTag("search_index_building_hint"),
                                )
                            }
                            if (hits.isNotEmpty()) {
                                hits.take(8).forEachIndexed { idx, hit ->
                                    Column(
                                        modifier =
                                            Modifier
                                                .fillMaxWidth()
                                                .testTag("search_hit_$idx")
                                                .clickable { applyHit(hit) }
                                                .padding(vertical = 6.dp, horizontal = 4.dp),
                                    ) {
                                        Text(
                                            placeHitDisplayLabel(hit),
                                            style = MaterialTheme.typography.bodyLarge,
                                        )
                                        Text(hit.kind, style = MaterialTheme.typography.bodySmall)
                                    }
                                }
                            }
                            if (viaPoints.isNotEmpty()) {
                                TextButton(
                                    onClick = { viaPoints = emptyList() },
                                    modifier = Modifier.testTag("btn_clear_vias"),
                                ) { Text("Clear vias (${viaPoints.size})") }
                            }
                            Button(
                                onClick = {
                                    scope.launch {
                                        val start = fromPoint
                                        if (start == null || toPoint.name.isBlank()) {
                                            status = "Set From and To first"
                                            return@launch
                                        }
                                        val coverageWaypoints =
                                            buildList {
                                                add(
                                                    RegionCoverage.Waypoint(
                                                        "From",
                                                        start.name,
                                                        start.lat,
                                                        start.lon,
                                                    ),
                                                )
                                                viaPoints.forEach { v ->
                                                    add(
                                                        RegionCoverage.Waypoint(
                                                            "Via",
                                                            v.name,
                                                            v.lat,
                                                            v.lon,
                                                        ),
                                                    )
                                                }
                                                add(
                                                    RegionCoverage.Waypoint(
                                                        "To",
                                                        toPoint.name,
                                                        toPoint.lat,
                                                        toPoint.lon,
                                                    ),
                                                )
                                            }
                                        val missing =
                                            RegionCoverage.missingCoverage(coverageWaypoints, dataDir)
                                        if (missing != null) {
                                            missingCoveragePrompt = missing
                                            NaviMapTestHooks.missingCoveragePromptVisible = true
                                            NaviMapTestHooks.lastMissingCoveragePath =
                                                missing.suggestedGeofabrikPath
                                            NaviMapTestHooks.lastMissingCoverageMessage = missing.message
                                            status = missing.message
                                            return@launch
                                        }
                                        val pts =
                                            buildList {
                                                add(start)
                                                addAll(viaPoints)
                                                add(toPoint)
                                            }
                                        // Prefer a single downloaded extract that covers the trip.
                                        val pbf =
                                            RegionCoverage.resolvePlanPbf(dataDir, coverageWaypoints)
                                        val stagedOk =
                                            profile == TravelProfile.HIKING &&
                                                NaviMapTestHooks.preferStagedHikingRoute &&
                                                File("/data/local/tmp/navi_fixtures/skolla_rondvassbu.polyline.txt").isFile
                                        if (pbf == null && !stagedOk) {
                                            status = "No region PBF — download a region in Tools first"
                                            return@launch
                                        }
                                        val wpsJson =
                                            pts.joinToString(",", "[", "]") {
                                                """{"name":${org.json.JSONObject.quote(it.name)},"lat":${it.lat},"lon":${it.lon}}"""
                                            }
                                        val ecoForPlan =
                                            if (ecoModeToggleable(profile)) ecoEnabled else true
                                        planAbort.set(false)
                                        val planStarted = System.currentTimeMillis()
                                        RoutingPlanLog.start(
                                            profile = profile.name.lowercase(),
                                            ecoEnabled = ecoForPlan,
                                            legCount = (pts.size - 1).coerceAtLeast(1),
                                            waypointNames = pts.map { it.name },
                                            startLat = pts.first().lat,
                                            startLon = pts.first().lon,
                                            endLat = pts.last().lat,
                                            endLon = pts.last().lon,
                                        )
                                        downloadProgressClear()
                                        planProgressClear()
                                        planningRoute = true
                                        routePlanPct = 0
                                        routePlanProgress = "Planning route: starting…"
                                        status = routePlanProgress
                                        val result =
                                            try {
                                                foregroundPlanEnter()
                                                planIndexingHintVisible =
                                                    withContext(Dispatchers.IO) {
                                                        val planPbf = pbf ?: resolveRegionPbf()
                                                        if (planPbf == null || !planPbf.isFile) {
                                                            true
                                                        } else {
                                                            runCatching {
                                                                indexedMapsStatus(
                                                                    planPbf.absolutePath,
                                                                    dataDir.absolutePath,
                                                                ).trim()
                                                            }.getOrDefault("missing") != "ready"
                                                        }
                                                    }
                                                withContext(Dispatchers.IO) {
                                                    runCatching {
                                                        val stagedPoly =
                                                            File(
                                                                "/data/local/tmp/navi_fixtures/skolla_rondvassbu.polyline.txt",
                                                            )
                                                        val stagedBreaks =
                                                            File(
                                                                "/data/local/tmp/navi_fixtures/skolla_rondvassbu.breaks.json",
                                                            )
                                                        if (profile == TravelProfile.HIKING &&
                                                            NaviMapTestHooks.preferStagedHikingRoute &&
                                                            stagedPoly.isFile
                                                        ) {
                                                            RoutingPlanLog.progress(50, ecoForPlan, detail = "staged")
                                                            val poly = stagedPoly.readText().trim()
                                                            val breaks =
                                                                if (stagedBreaks.isFile) {
                                                                    stagedBreaks.readText().trim()
                                                                } else {
                                                                    "[]"
                                                                }
                                                            val stagedSamples =
                                                                File(
                                                                    "/data/local/tmp/navi_fixtures/skolla_rondvassbu.sim_samples.json",
                                                                )
                                                            val samplesJson =
                                                                if (stagedSamples.isFile) {
                                                                    stagedSamples.readText().trim()
                                                                } else {
                                                                    "[]"
                                                                }
                                                            uniffi.navi.CorridorRouteResult(
                                                                report = "TEST_KIND=STAGED_HIKE\nPASS\ndistance_km=112.5\n",
                                                                distanceKm = 112.5,
                                                                etaMinutes = 112.5 * 16.0,
                                                                cacheHit = true,
                                                                coldBuildS = 0.0,
                                                                warmLoadS = 0.0,
                                                                routePolyline = poly,
                                                                poiLat = toPoint.lat,
                                                                poiLon = toPoint.lon,
                                                                poiName = toPoint.name,
                                                                poiIconKey = "cabin",
                                                                breakPoisJson = breaks,
                                                                daysJson = "[]",
                                                                simSamplesJson = samplesJson,
                                                                maneuversJson = "[]",
                                                                priorityPathSharePct = 0.0,
                                                                routeSegmentsJson = "[]",
                                                                offTrailAdvisory = "",
                                                            )
                                                        } else {
                                                            when (profile) {
                                                                TravelProfile.HIKING -> {
                                                                    if (planAbort.get()) {
                                                                        return@runCatching cancelledCorridorResult()
                                                                    }
                                                                    RoutingPlanLog.progress(
                                                                        10,
                                                                        ecoForPlan,
                                                                        detail = "hiking_graph",
                                                                    )
                                                                    val hike =
                                                                        uniffi.navi.planHikingRoute(
                                                                            pbf!!.absolutePath,
                                                                            File(dataDir, "elevation").absolutePath,
                                                                            File(dataDir, "graph-cache-foot").absolutePath,
                                                                            wpsJson,
                                                                            preferOfficialNetworks,
                                                                            preferPilgrimRoutes,
                                                                            dataDir.absolutePath,
                                                                        )
                                                                    RoutingPlanLog.progress(
                                                                        90,
                                                                        ecoForPlan,
                                                                        detail = "hiking_path",
                                                                    )
                                                                    hike
                                                                }
                                                                else -> {
                                                                    // Multi-leg motor/bike: bbox-clipped graph per profile.
                                                                    var poly = ""
                                                                    var dist = 0.0
                                                                    var etaSum = 0.0
                                                                    var shareWeighted = 0.0
                                                                    var last: uniffi.navi.CorridorRouteResult? = null
                                                                    val legSamples = mutableListOf<List<RouteSimSample>>()
                                                                    val legManeuvers = mutableListOf<List<RouteManeuver>>()
                                                                    val vehicleAvoidanceLines = linkedSetOf<String>()
                                                                    val legTotal = pts.size - 1
                                                                    val graphTag =
                                                                        when (profile) {
                                                                            TravelProfile.BICYCLE,
                                                                            TravelProfile.BICYCLE_ELECTRIC,
                                                                            -> "bicycle"
                                                                            TravelProfile.TRUCK,
                                                                            TravelProfile.TRUCK_ELECTRIC,
                                                                            TravelProfile.MOBILE_HOME,
                                                                            -> "truck"
                                                                            else -> "car"
                                                                        }
                                                                    val cacheDir =
                                                                        File(
                                                                            dataDir,
                                                                            "graph-cache-${pbf!!.nameWithoutExtension}-$graphTag",
                                                                        )
                                                                    for (i in 0 until legTotal) {
                                                                        if (planAbort.get()) {
                                                                            return@runCatching last
                                                                                ?: cancelledCorridorResult()
                                                                        }
                                                                        val a = pts[i]
                                                                        val b = pts[i + 1]
                                                                        val pct = ((i * 100) / legTotal).coerceIn(0, 99)
                                                                        RoutingPlanLog.progress(
                                                                            pct,
                                                                            ecoForPlan,
                                                                            detail = "leg_${i + 1}_of_$legTotal",
                                                                        )
                                                                        val legRes =
                                                                            uniffi.navi.planCarRoute(
                                                                                pbf.absolutePath,
                                                                                File(dataDir, "elevation").absolutePath,
                                                                                cacheDir.absolutePath,
                                                                                a.lat,
                                                                                a.lon,
                                                                                b.lat,
                                                                                b.lon,
                                                                                ecoForPlan,
                                                                                profile,
                                                                                avoidMotorways,
                                                                                avoidTolls,
                                                                                avoidFerries,
                                                                                loadVehicleLimits(dataDir.absolutePath),
                                                                                preferOfficialNetworks,
                                                                                dataDir.absolutePath,
                                                                            )
                                                                        if (!legRes.report.contains("PASS")) {
                                                                            return@runCatching legRes
                                                                        }
                                                                        legRes.report.lineSequence().forEach { line ->
                                                                            if (line.contains(
                                                                                    "weight/height/width/length-restricted",
                                                                                    ignoreCase = true,
                                                                                )
                                                                            ) {
                                                                                vehicleAvoidanceLines += line.trim()
                                                                            }
                                                                        }
                                                                        dist += legRes.distanceKm
                                                                        etaSum += legRes.etaMinutes
                                                                        shareWeighted += legRes.priorityPathSharePct * legRes.distanceKm
                                                                        poly =
                                                                            if (poly.isEmpty()) {
                                                                                legRes.routePolyline
                                                                            } else {
                                                                                poly + ";" +
                                                                                    legRes.routePolyline
                                                                                        .substringAfter(';')
                                                                            }
                                                                        legSamples.add(parseRouteSimSamples(legRes.simSamplesJson))
                                                                        legManeuvers.add(parseRouteManeuvers(legRes.maneuversJson))
                                                                        last = legRes
                                                                    }
                                                                    val base = last!!
                                                                    val mergedSamples = mergeSimSamples(legSamples)
                                                                    val mergedManeuvers = mergeManeuvers(legManeuvers)
                                                                    val mergedShare =
                                                                        if (dist > 0.0) {
                                                                            shareWeighted / dist
                                                                        } else {
                                                                            base.priorityPathSharePct
                                                                        }
                                                                    val mergedReport =
                                                                        buildString {
                                                                            append(base.report)
                                                                            if (!base.report.endsWith("\n") &&
                                                                                vehicleAvoidanceLines.isNotEmpty()
                                                                            ) {
                                                                                append('\n')
                                                                            }
                                                                            vehicleAvoidanceLines.forEach { appendLine(it) }
                                                                        }
                                                                    uniffi.navi.CorridorRouteResult(
                                                                        report = mergedReport,
                                                                        distanceKm = dist,
                                                                        etaMinutes = etaSum,
                                                                        cacheHit = base.cacheHit,
                                                                        coldBuildS = base.coldBuildS,
                                                                        warmLoadS = base.warmLoadS,
                                                                        routePolyline = poly,
                                                                        poiLat = toPoint.lat,
                                                                        poiLon = toPoint.lon,
                                                                        poiName = toPoint.name,
                                                                        poiIconKey = base.poiIconKey,
                                                                        breakPoisJson = base.breakPoisJson,
                                                                        daysJson = base.daysJson,
                                                                        simSamplesJson =
                                                                            org.json
                                                                                .JSONArray(
                                                                                    mergedSamples.map { s ->
                                                                                        org.json
                                                                                            .JSONObject()
                                                                                            .put("lat", s.lat)
                                                                                            .put("lon", s.lon)
                                                                                            .put("cum_m", s.cumM)
                                                                                            .put("speed_kmh", s.speedKmh)
                                                                                            .put("highway", s.highway)
                                                                                            .put("maxspeed_posted", s.maxspeedPosted)
                                                                                    },
                                                                                ).toString(),
                                                                        maneuversJson =
                                                                            org.json
                                                                                .JSONArray(
                                                                                    mergedManeuvers.map { m ->
                                                                                        org.json
                                                                                            .JSONObject()
                                                                                            .put("lat", m.lat)
                                                                                            .put("lon", m.lon)
                                                                                            .put("cum_m", m.cumM)
                                                                                            .put("kind", m.kind)
                                                                                            .put("street", m.street)
                                                                                            .put("roundabout_exit", m.roundaboutExit)
                                                                                            .also { jo ->
                                                                                                if (m.icon != null) {
                                                                                                    jo.put("icon", m.icon)
                                                                                                }
                                                                                            }
                                                                                    },
                                                                                ).toString(),
                                                                        priorityPathSharePct = mergedShare,
                                                                        routeSegmentsJson = "[]",
                                                                        offTrailAdvisory = "",
                                                                    )
                                                                }
                                                            }
                                                        }
                                                    }.getOrElse { e ->
                                                        if (e is CancellationException) throw e
                                                        android.util.Log.e("NaviRoute", "plan failed", e)
                                                        uniffi.navi.CorridorRouteResult(
                                                            report = "FAIL: ${e.message ?: e.javaClass.simpleName}\n",
                                                            distanceKm = 0.0,
                                                            etaMinutes = 0.0,
                                                            cacheHit = false,
                                                            coldBuildS = 0.0,
                                                            warmLoadS = 0.0,
                                                            routePolyline = "",
                                                            poiLat = 0.0,
                                                            poiLon = 0.0,
                                                            poiName = "",
                                                            poiIconKey = "",
                                                            breakPoisJson = "[]",
                                                            daysJson = "[]",
                                                            simSamplesJson = "[]",
                                                            maneuversJson = "[]",
                                                            priorityPathSharePct = 0.0,
                                                            routeSegmentsJson = "[]",
                                                            offTrailAdvisory = "",
                                                        )
                                                    }
                                                }
                                            } catch (e: CancellationException) {
                                                RoutingPlanLog.cancelled(
                                                    ecoForPlan,
                                                    System.currentTimeMillis() - planStarted,
                                                    reason = "cancelled",
                                                    report = "",
                                                )
                                                if (status != "Planning cancelled") {
                                                    status = "Planning cancelled"
                                                }
                                                throw e
                                            } finally {
                                                planningRoute = false
                                                routePlanPct = -1
                                                routePlanProgress = ""
                                                planIndexingHintVisible = false
                                                planProgressClear()
                                                foregroundPlanLeave()
                                                downloadProgressClear()
                                            }
                                        val durationMs = System.currentTimeMillis() - planStarted
                                        if (planAbort.get() || planReportIsCancelled(result.report)) {
                                            NaviMapTestHooks.lastPlanReport = result.report
                                            NaviMapTestHooks.lastRoutePolylineChars = 0
                                            NaviMapTestHooks.lastRoutePolyline = ""
                                            RoutingPlanLog.cancelled(
                                                ecoForPlan,
                                                durationMs,
                                                reason = "cancelled",
                                                report = result.report,
                                            )
                                            if (status != "Planning cancelled") {
                                                status = "Planning cancelled"
                                            }
                                            return@launch
                                        }
                                        if (!result.report.contains("PASS") || result.routePolyline.isBlank()) {
                                            NaviMapTestHooks.lastPlanReport = result.report
                                            NaviMapTestHooks.lastRoutePolylineChars = 0
                                            NaviMapTestHooks.lastRoutePolyline = ""
                                            RoutingPlanLog.failed(
                                                ecoForPlan,
                                                durationMs,
                                                userFacingStatus(result.report).ifBlank { "Routing failed" },
                                            )
                                            status = userFacingStatus(result.report).ifBlank { "Routing failed" }
                                            return@launch
                                        }
                                        RoutingPlanLog.complete(result, ecoForPlan, durationMs)
                                        NaviMapTestHooks.routeStartLabel = start.name
                                        NaviMapTestHooks.routeEndLabel = toPoint.name
                                        NaviMapTestHooks.routeViaLabel =
                                            viaPoints.joinToString(", ") { it.name }
                                        // Apply on this composition immediately. Do not only stash
                                        // into pendingRoute — a non-resumed sibling activity can
                                        // consume the hook and the visible map stays empty.
                                        applyPlannedRoute(result)
                                        prioritySharePct = result.priorityPathSharePct
                                        val planStatus =
                                            formatEbikePlanStatus(
                                                result.report,
                                                result.distanceKm,
                                                driveHud.unitSystem,
                                            )
                                                ?: (
                                                    formatRouteAvoidanceReport(
                                                        avoidMotorways,
                                                        avoidTolls,
                                                        avoidFerries,
                                                        prioritySharePct,
                                                    ) + "\n" +
                                                        DisplayUnits.formatRoutePlanned(
                                                            result.distanceKm,
                                                            driveHud.unitSystem,
                                                        )
                                                )
                                        status =
                                            withIndexedPackMissHint(
                                                if (result.offTrailAdvisory.isNotBlank()) {
                                                    "$planStatus · Off-trail: use judgment (terrain advisory)"
                                                } else {
                                                    planStatus
                                                },
                                                result.report,
                                            )
                                    }
                                },
                                enabled = !planningRoute,
                                modifier =
                                    Modifier
                                        .fillMaxWidth()
                                        .testTag("btn_plan_route"),
                            ) {
                                Text(if (planningRoute) "Planning…" else "Plan route")
                            }
                            if (mapState.polyline.isNotBlank()) {
                                TextButton(
                                    onClick = { clearActiveRoute("Planned route deleted") },
                                    enabled = !planningRoute,
                                    modifier =
                                        Modifier
                                            .fillMaxWidth()
                                            .testTag("btn_delete_planned_route"),
                                ) {
                                    Text("Delete route")
                                }
                            }
                            if (routePlanProgress.isNotBlank() || planningRoute) {
                                Text(
                                    text = routePlanProgress.ifBlank { "Planning route…" },
                                    style = MaterialTheme.typography.bodySmall,
                                    modifier =
                                        Modifier
                                            .fillMaxWidth()
                                            .testTag("route_plan_progress"),
                                )
                                if (routePlanPct in 0..100) {
                                    LinearProgressIndicator(
                                        progress = { routePlanPct / 100f },
                                        modifier =
                                            Modifier
                                                .fillMaxWidth()
                                                .height(6.dp)
                                                .testTag("route_plan_bar"),
                                    )
                                } else {
                                    LinearProgressIndicator(
                                        modifier =
                                            Modifier
                                                .fillMaxWidth()
                                                .height(6.dp)
                                                .testTag("route_plan_bar"),
                                    )
                                }
                                if (planIndexingHintVisible) {
                                    TextButton(
                                        onClick = { showTools = true },
                                        modifier =
                                            Modifier
                                                .fillMaxWidth()
                                                .testTag("route_plan_indexing_hint"),
                                    ) {
                                        Text(
                                            "Planning is faster once background indexing finishes " +
                                                "(Tools → Indexed maps). This plan may take longer " +
                                                "if indexing isn't done yet.",
                                            style = MaterialTheme.typography.bodySmall,
                                        )
                                    }
                                }
                            }
                            MultiDayPlanCards(
                                days = mapState.multiDayCards,
                                unitSystem = driveHud.unitSystem,
                                modifier =
                                    Modifier
                                        .fillMaxWidth()
                                        .padding(top = 4.dp),
                            )
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                TextButton(
                                    onClick = {
                                        val hasDeviceFix =
                                            mapState.gpsLat != 0.0 || mapState.gpsLon != 0.0
                                        if (!hasDeviceFix) {
                                            status = "GPS unavailable"
                                            return@TextButton
                                        }
                                        val fixLat = mapState.gpsLat
                                        val fixLon = mapState.gpsLon
                                        val targetAtClick = searchTarget
                                        val immediate = gpsImmediateCoordHit(fixLat, fixLon)
                                        applyHit(immediate, target = targetAtClick)
                                        NaviMapTestHooks.lastGpsImmediateCoord =
                                            formatCoordWaypointName(fixLat, fixLon)
                                        scope.launch {
                                            val (name, kind) = resolveLabelAt(fixLat, fixLon)
                                            val current =
                                                when (targetAtClick) {
                                                    SearchTarget.From -> fromPoint
                                                    SearchTarget.To -> toPoint
                                                    SearchTarget.Via -> viaPoints.lastOrNull()
                                                }
                                            if (!gpsWaypointShouldUpgrade(
                                                    current?.lat,
                                                    current?.lon,
                                                    current?.name,
                                                    fixLat,
                                                    fixLon,
                                                    name,
                                                    kind,
                                                )
                                            ) {
                                                return@launch
                                            }
                                            val hitKind =
                                                when (kind) {
                                                    "map-resolved" -> "gps-resolved"
                                                    "map-mark" -> "gps"
                                                    else -> kind
                                                }
                                            applyHit(
                                                PlaceHit(
                                                    osmId = 0L,
                                                    name = name,
                                                    kind = hitKind,
                                                    lat = fixLat,
                                                    lon = fixLon,
                                                    subArea = "",
                                                    municipality = "",
                                                ),
                                                target = targetAtClick,
                                            )
                                        }
                                    },
                                    modifier = Modifier.testTag("btn_use_gps"),
                                ) {
                                    Text(
                                        when (searchTarget) {
                                            SearchTarget.From -> "Use GPS as from"
                                            SearchTarget.To -> "Use GPS as to"
                                            SearchTarget.Via -> "Use GPS as via"
                                        },
                                    )
                                }
                                TextButton(
                                    onClick = {
                                        val last =
                                            savedRoutes.firstOrNull {
                                                it.lastBreakLat != null && it.lastBreakLon != null
                                            }
                                        if (last?.lastBreakLat != null && last.lastBreakLon != null) {
                                            fromPoint =
                                                Waypoint(
                                                    "Last stop",
                                                    last.lastBreakLat!!,
                                                    last.lastBreakLon!!,
                                                )
                                            status = "Continue from last stop on ${last.endName}"
                                        } else if (viaPoints.isNotEmpty()) {
                                            fromPoint = viaPoints.last()
                                            status = "Continue from via (${viaPoints.last().name})"
                                        } else {
                                            status = "No last stop saved yet"
                                        }
                                    },
                                ) { Text("Continue from last stop") }
                            }
                            if (isDebuggable && mapState.polyline.isNotBlank() && routeSamples.size >= 2) {
                                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                    TextButton(
                                        onClick = {
                                            if (simulating) stopRouteSimulation() else startRouteSimulation()
                                        },
                                        modifier = Modifier.testTag("btn_simulate_route"),
                                    ) {
                                        Text(if (simulating) "Stop simulation" else "Simulate route")
                                    }
                                }
                            }
                        }
                    }

                    Spacer(modifier = Modifier.height(8.dp))

                    Surface(
                        shape = RoundedCornerShape(12.dp),
                        tonalElevation = 3.dp,
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .testTag("profile_menu"),
                    ) {
                        Column(modifier = Modifier.padding(10.dp)) {
                            if (!showProfilePanel) {
                                TextButton(
                                    onClick = { showProfilePanel = true },
                                    modifier = Modifier.testTag("btn_open_profile"),
                                ) { Text("Profile") }
                            } else {
                                Text("Profile", style = MaterialTheme.typography.labelLarge)
                                Row(
                                    modifier =
                                        Modifier
                                            .fillMaxWidth()
                                            .horizontalScroll(rememberScrollState()),
                                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                                ) {
                                    TravelProfile.entries.filter { travelProfileMenuFocus(it) }.forEach { p ->
                                        FilterChip(
                                            selected = profile == p,
                                            onClick = {
                                                profile = p
                                                ecoEnabled = ecoModeDefault(p)
                                                status = "Profile: ${p.name.lowercase()}"
                                            },
                                            label = {
                                                Text(travelProfileChipLabel(p))
                                            },
                                            modifier =
                                                Modifier.testTag(
                                                    when (p) {
                                                        TravelProfile.HIKING -> "chip_profile_hiking"
                                                        TravelProfile.CAR -> "chip_profile_car"
                                                        TravelProfile.BICYCLE_ELECTRIC ->
                                                            "chip_profile_bicycle_electric"
                                                        else -> "chip_profile_${p.name.lowercase()}"
                                                    },
                                                ),
                                        )
                                    }
                                }
                                Row(
                                    modifier = Modifier.fillMaxWidth(),
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.SpaceBetween,
                                ) {
                                    Text(
                                        if (ecoModeToggleable(profile)) {
                                            "Eco routing"
                                        } else {
                                            "Eco routing (locked on for this profile)"
                                        },
                                    )
                                    Switch(
                                        checked = if (ecoModeToggleable(profile)) ecoEnabled else true,
                                        onCheckedChange = { enabled ->
                                            if (ecoModeToggleable(profile)) {
                                                ecoEnabled = enabled
                                                DiagnosticLog.logToggle(
                                                    "eco_mode",
                                                    enabled,
                                                    mapOf("profile" to profile.name),
                                                )
                                            }
                                        },
                                        enabled = ecoModeToggleable(profile),
                                    )
                                }
                                if (profile == TravelProfile.HIKING ||
                                    profile == TravelProfile.BICYCLE ||
                                    profile == TravelProfile.BICYCLE_ELECTRIC
                                ) {
                                    Row(
                                        modifier = Modifier.fillMaxWidth(),
                                        verticalAlignment = Alignment.CenterVertically,
                                        horizontalArrangement = Arrangement.SpaceBetween,
                                    ) {
                                        Text("Follow official hiking/cycling networks")
                                        Switch(
                                            checked = preferOfficialNetworks,
                                            onCheckedChange = { on ->
                                                preferOfficialNetworks = on
                                                uniffi.navi.savePreferOfficialNetworks(
                                                    dataDir.absolutePath,
                                                    on,
                                                )
                                                DiagnosticLog.logToggle(
                                                    "prefer_official_networks",
                                                    on,
                                                    mapOf("profile" to profile.name),
                                                )
                                                DiagnosticLog.logSettingSaved(
                                                    "prefer_official_networks",
                                                    on,
                                                )
                                            },
                                        )
                                    }
                                    Row(
                                        modifier = Modifier.fillMaxWidth(),
                                        verticalAlignment = Alignment.CenterVertically,
                                        horizontalArrangement = Arrangement.SpaceBetween,
                                    ) {
                                        Text("Use networked cabins")
                                        Switch(
                                            checked = useNetworkedCabins,
                                            onCheckedChange = { on ->
                                                useNetworkedCabins = on
                                                uniffi.navi.saveUseNetworkedCabins(
                                                    dataDir.absolutePath,
                                                    on,
                                                )
                                                DiagnosticLog.logToggle(
                                                    "use_networked_cabins",
                                                    on,
                                                    mapOf("profile" to profile.name),
                                                )
                                                DiagnosticLog.logSettingSaved(
                                                    "use_networked_cabins",
                                                    on,
                                                )
                                            },
                                            modifier = Modifier.testTag("toggle_use_networked_cabins"),
                                        )
                                    }
                                }
                                if (profile == TravelProfile.BICYCLE ||
                                    profile == TravelProfile.BICYCLE_ELECTRIC
                                ) {
                                    Column(modifier = Modifier.fillMaxWidth()) {
                                        Text("Bike type (surface suitability)")
                                        Row(
                                            modifier = Modifier.fillMaxWidth(),
                                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                                        ) {
                                            listOf(
                                                "road" to "Road",
                                                "trekking" to "Gravel",
                                                "mountain" to "MTB",
                                            ).forEach { (id, label) ->
                                                TextButton(
                                                    onClick = {
                                                        bikeCapability = id
                                                        uniffi.navi.saveBikeCapability(
                                                            dataDir.absolutePath,
                                                            id,
                                                        )
                                                        DiagnosticLog.logSettingSaved(
                                                            "bike_capability",
                                                            id,
                                                        )
                                                    },
                                                ) {
                                                    Text(
                                                        label,
                                                        fontWeight =
                                                            if (bikeCapability == id) {
                                                                FontWeight.Bold
                                                            } else {
                                                                FontWeight.Normal
                                                            },
                                                    )
                                                }
                                            }
                                        }
                                    }
                                }
                                if (profile == TravelProfile.HIKING) {
                                    Row(
                                        modifier = Modifier.fillMaxWidth(),
                                        verticalAlignment = Alignment.CenterVertically,
                                        horizontalArrangement = Arrangement.SpaceBetween,
                                    ) {
                                        Text("Network hut member (DNT/STF/…)")
                                        Switch(
                                            checked = networkHutMember,
                                            onCheckedChange = { on ->
                                                networkHutMember = on
                                                uniffi.navi.saveNetworkHutMember(
                                                    dataDir.absolutePath,
                                                    on,
                                                )
                                                DiagnosticLog.logToggle(
                                                    "network_hut_member",
                                                    on,
                                                    mapOf("profile" to profile.name),
                                                )
                                                DiagnosticLog.logSettingSaved(
                                                    "network_hut_member",
                                                    on,
                                                )
                                            },
                                            modifier = Modifier.testTag("toggle_network_hut_member"),
                                        )
                                    }
                                    Row(
                                        modifier = Modifier.fillMaxWidth(),
                                        verticalAlignment = Alignment.CenterVertically,
                                        horizontalArrangement = Arrangement.SpaceBetween,
                                    ) {
                                        Text("Follow pilgrim routes")
                                        Switch(
                                            checked = preferPilgrimRoutes,
                                            onCheckedChange = { on ->
                                                preferPilgrimRoutes = on
                                                uniffi.navi.savePreferPilgrimRoutes(
                                                    dataDir.absolutePath,
                                                    on,
                                                )
                                                DiagnosticLog.logToggle(
                                                    "prefer_pilgrim_routes",
                                                    on,
                                                    mapOf("profile" to profile.name),
                                                )
                                                DiagnosticLog.logSettingSaved(
                                                    "prefer_pilgrim_routes",
                                                    on,
                                                )
                                            },
                                            modifier = Modifier.testTag("toggle_prefer_pilgrim_routes"),
                                        )
                                    }
                                }
                                Row(
                                    modifier = Modifier.fillMaxWidth(),
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.SpaceBetween,
                                ) {
                                    Text("Avoid motorways")
                                    Switch(
                                        checked = avoidMotorways,
                                        onCheckedChange = { on ->
                                            avoidMotorways = on
                                            DiagnosticLog.logToggle("avoid_motorways", on)
                                            status =
                                                formatRouteAvoidanceReport(
                                                    avoidMotorways,
                                                    avoidTolls,
                                                    avoidFerries,
                                                    prioritySharePct,
                                                )
                                        },
                                    )
                                }
                                Row(
                                    modifier = Modifier.fillMaxWidth(),
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.SpaceBetween,
                                ) {
                                    Text("Avoid toll roads")
                                    Switch(
                                        checked = avoidTolls,
                                        onCheckedChange = { on ->
                                            avoidTolls = on
                                            DiagnosticLog.logToggle("avoid_tolls", on)
                                            status =
                                                formatRouteAvoidanceReport(
                                                    avoidMotorways,
                                                    avoidTolls,
                                                    avoidFerries,
                                                    prioritySharePct,
                                                )
                                        },
                                        enabled =
                                            profile == TravelProfile.CAR ||
                                                profile == TravelProfile.TRUCK ||
                                                profile == TravelProfile.MOBILE_HOME ||
                                                profile == TravelProfile.MOTORCYCLE ||
                                                profile == TravelProfile.CAR_ELECTRIC ||
                                                profile == TravelProfile.TRUCK_ELECTRIC ||
                                                profile == TravelProfile.MOTORCYCLE_ELECTRIC ||
                                                profile == TravelProfile.BICYCLE ||
                                                profile == TravelProfile.BICYCLE_ELECTRIC ||
                                                profile == TravelProfile.HIKING,
                                    )
                                }
                                Row(
                                    modifier = Modifier.fillMaxWidth(),
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.SpaceBetween,
                                ) {
                                    Text("Avoid ferries")
                                    Switch(
                                        checked = avoidFerries,
                                        onCheckedChange = { on ->
                                            avoidFerries = on
                                            DiagnosticLog.logToggle("avoid_ferries", on)
                                            status =
                                                formatRouteAvoidanceReport(
                                                    avoidMotorways,
                                                    avoidTolls,
                                                    avoidFerries,
                                                    prioritySharePct,
                                                )
                                        },
                                        enabled =
                                            profile == TravelProfile.CAR ||
                                                profile == TravelProfile.TRUCK ||
                                                profile == TravelProfile.MOBILE_HOME ||
                                                profile == TravelProfile.MOTORCYCLE ||
                                                profile == TravelProfile.CAR_ELECTRIC ||
                                                profile == TravelProfile.TRUCK_ELECTRIC ||
                                                profile == TravelProfile.MOTORCYCLE_ELECTRIC,
                                    )
                                }
                                Text(
                                    formatRouteAvoidanceReport(
                                        avoidMotorways,
                                        avoidTolls,
                                        avoidFerries,
                                        prioritySharePct,
                                    ),
                                    style = MaterialTheme.typography.bodySmall,
                                )
                                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                    Button(
                                        onClick = {
                                            runCatching {
                                                if (usesTruckRestSettings(profile)) {
                                                    val rest = loadTruckRestSettings(dataDir.absolutePath)
                                                    saveTruckRestSettings(
                                                        dataDir.absolutePath,
                                                        FfiTruckRestSettings(
                                                            mandatoryBreakAfterHours =
                                                                rest.mandatoryBreakAfterHours,
                                                            breakDurationMinutes = rest.breakDurationMinutes,
                                                            preferSplitBreak = rest.preferSplitBreak,
                                                            maxDailyDrivingHours = rest.maxDailyDrivingHours,
                                                            maxDailyDrivingExtendedHours =
                                                                rest.maxDailyDrivingExtendedHours,
                                                            maxDailyExtensionsPerWeek =
                                                                rest.maxDailyExtensionsPerWeek,
                                                            maxWeeklyDrivingHours = rest.maxWeeklyDrivingHours,
                                                            maxFortnightlyDrivingHours =
                                                                rest.maxFortnightlyDrivingHours,
                                                            exceptionalExtensionArmed =
                                                                rest.exceptionalExtensionArmed,
                                                            ecoModeEnabled = ecoEnabled,
                                                        ),
                                                    )
                                                } else {
                                                    val rest = loadCarRestSettings(dataDir.absolutePath)
                                                    saveCarRestSettings(
                                                        dataDir.absolutePath,
                                                        FfiCarRestSettings(
                                                            breakIntervalHours = rest.breakIntervalHours,
                                                            restDurationMinutes = rest.restDurationMinutes,
                                                            ecoModeEnabled = ecoEnabled,
                                                        ),
                                                    )
                                                }
                                            }
                                            status = "Profile settings saved"
                                        },
                                        modifier = Modifier.testTag("btn_save_profile"),
                                    ) { Text("Save") }
                                    TextButton(
                                        onClick = {
                                            showProfilePanel = false
                                            hideSearch = true
                                            showTools = false
                                            status = "Route planning closed"
                                        },
                                        modifier = Modifier.testTag("btn_close_profile"),
                                    ) { Text("Close") }
                                }
                            } // end showProfilePanel
                        }
                    }

                    Spacer(modifier = Modifier.height(8.dp))

                    if (showVehicleLimitsPanel || !showVehiclePanel) {
                        Surface(
                            shape = RoundedCornerShape(12.dp),
                            tonalElevation = 3.dp,
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Column(modifier = Modifier.padding(10.dp)) {
                                if (!showVehiclePanel) {
                                    TextButton(
                                        onClick = { showVehiclePanel = true },
                                        modifier = Modifier.testTag("btn_open_vehicle"),
                                    ) { Text("Vehicle") }
                                } else if (showVehicleLimitsPanel) {
                                    Text("Vehicle limits", style = MaterialTheme.typography.labelLarge)
                                    if (showVehicleClearance) {
                                        OutlinedTextField(
                                            value = axleKg,
                                            onValueChange = { axleKg = it },
                                            label = { Text("Axle weight (kg)") },
                                            singleLine = true,
                                            modifier = Modifier.fillMaxWidth(),
                                        )
                                        OutlinedTextField(
                                            value = bogieKg,
                                            onValueChange = { bogieKg = it },
                                            label = { Text("Max bogie weight (kg)") },
                                            singleLine = true,
                                            modifier = Modifier.fillMaxWidth(),
                                        )
                                    }
                                    if (showVehicleHeightOnly || showVehicleClearance) {
                                        OutlinedTextField(
                                            value = heightM,
                                            onValueChange = { heightM = it },
                                            label = { Text("Height (m)") },
                                            singleLine = true,
                                            modifier =
                                                Modifier
                                                    .fillMaxWidth()
                                                    .testTag("field_vehicle_height"),
                                        )
                                    }
                                    if (showVehicleClearance) {
                                        OutlinedTextField(
                                            value = widthM,
                                            onValueChange = { widthM = it },
                                            label = { Text("Width (m)") },
                                            singleLine = true,
                                            modifier = Modifier.fillMaxWidth(),
                                        )
                                        OutlinedTextField(
                                            value = lengthM,
                                            onValueChange = { lengthM = it },
                                            label = { Text("Length (m)") },
                                            singleLine = true,
                                            modifier = Modifier.fillMaxWidth(),
                                        )
                                    }
                                    Button(onClick = { persistVehicle() }, modifier = Modifier.fillMaxWidth().testTag("btn_save_vehicle")) {
                                        Text("Save")
                                    }
                                    TextButton(
                                        onClick = {
                                            showVehiclePanel = false
                                            hideSearch = true
                                            showTools = false
                                            status = "Route planning closed"
                                        },
                                        modifier = Modifier.testTag("btn_close_vehicle"),
                                    ) { Text("Close") }
                                }
                            }
                        }
                    }

                    Spacer(modifier = Modifier.height(8.dp))

                    Surface(
                        shape = RoundedCornerShape(12.dp),
                        tonalElevation = 3.dp,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Column(modifier = Modifier.padding(10.dp)) {
                            if (!showRoutesPanel) {
                                TextButton(
                                    onClick = { showRoutesPanel = true },
                                    modifier = Modifier.testTag("btn_open_routes"),
                                ) { Text("Saved routes") }
                            } else {
                                Row(
                                    modifier = Modifier.fillMaxWidth(),
                                    horizontalArrangement = Arrangement.SpaceBetween,
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Text("Saved routes", style = MaterialTheme.typography.labelLarge)
                                    TextButton(onClick = { refreshRoutes() }) { Text("Refresh") }
                                }
                                if (mapState.polyline.isNotBlank()) {
                                    TextButton(
                                        onClick = { clearActiveRoute("Planned route deleted") },
                                        modifier =
                                            Modifier
                                                .fillMaxWidth()
                                                .testTag("btn_delete_planned_route_saved"),
                                    ) {
                                        Text("Delete planned route")
                                    }
                                }
                                if (savedRoutes.isEmpty()) {
                                    Text("No saved routes", style = MaterialTheme.typography.bodySmall)
                                } else {
                                    savedRoutes.forEach { route ->
                                        Row(
                                            modifier =
                                                Modifier
                                                    .fillMaxWidth()
                                                    .padding(vertical = 4.dp),
                                            horizontalArrangement = Arrangement.SpaceBetween,
                                            verticalAlignment = Alignment.CenterVertically,
                                        ) {
                                            Column(modifier = Modifier.weight(1f)) {
                                                Text(
                                                    "${route.startName} -> ${route.endName}",
                                                    style = MaterialTheme.typography.bodyMedium,
                                                )
                                                Text(
                                                    "${route.profile} · ${route.createdAt}",
                                                    style = MaterialTheme.typography.bodySmall,
                                                )
                                            }
                                            TextButton(
                                                onClick = {
                                                    if (deleteSavedRoute(dataDir.absolutePath, route.id)) {
                                                        refreshRoutes()
                                                        status = "Deleted saved route ${route.id.take(8)}"
                                                    } else {
                                                        status = "Could not delete saved route"
                                                    }
                                                },
                                                modifier = Modifier.testTag("btn_delete_saved_route"),
                                            ) { Text("Delete route") }
                                        }
                                    }
                                }
                                Button(
                                    onClick = {
                                        if (toPoint.name.isBlank()) {
                                            status = "Set a To destination first"
                                            return@Button
                                        }
                                        val start = fromPoint ?: Waypoint("Start", 61.2, 10.7)
                                        val viaJson =
                                            if (viaPoints.isEmpty()) {
                                                "[]"
                                            } else {
                                                viaPoints.joinToString(",", "[", "]") {
                                                    """{"name":${org.json.JSONObject.quote(it.name)},"lat":${it.lat},"lon":${it.lon}}"""
                                                }
                                            }
                                        val report =
                                            saveNamedRoute(
                                                dataDir = dataDir.absolutePath,
                                                startLat = start.lat,
                                                startLon = start.lon,
                                                startName = start.name,
                                                endLat = toPoint.lat,
                                                endLon = toPoint.lon,
                                                endName = toPoint.name,
                                                viaJson = viaJson,
                                                profile = profile.name.lowercase(),
                                                summaryJson = """{"avoid_motorways":$avoidMotorways,"avoid_tolls":$avoidTolls,"avoid_ferries":$avoidFerries,"priority_share_pct":$prioritySharePct}""",
                                            )
                                        refreshRoutes()
                                        status = report
                                    },
                                    modifier =
                                        Modifier
                                            .fillMaxWidth()
                                            .testTag("btn_save_routes"),
                                ) {
                                    Text("Save")
                                }
                                TextButton(
                                    onClick = {
                                        showRoutesPanel = false
                                        hideSearch = true
                                        showTools = false
                                        status = "Route planning closed"
                                    },
                                    modifier = Modifier.testTag("btn_close_routes"),
                                ) { Text("Close") }
                            }
                        }
                    }

                    Spacer(modifier = Modifier.height(8.dp))

                    Surface(
                        shape = RoundedCornerShape(12.dp),
                        tonalElevation = 3.dp,
                        modifier = Modifier.fillMaxWidth().testTag("saved_places_panel"),
                    ) {
                        Column(modifier = Modifier.padding(10.dp)) {
                            if (!showPlacesPanel) {
                                TextButton(
                                    onClick = {
                                        showPlacesPanel = true
                                        refreshPlaces()
                                    },
                                    modifier = Modifier.testTag("btn_open_places"),
                                ) { Text("Saved places") }
                            } else {
                                Row(
                                    modifier = Modifier.fillMaxWidth(),
                                    horizontalArrangement = Arrangement.SpaceBetween,
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Text("Saved places", style = MaterialTheme.typography.labelLarge)
                                    TextButton(
                                        onClick = { refreshPlaces() },
                                        modifier = Modifier.testTag("btn_refresh_places"),
                                    ) { Text("Refresh") }
                                }
                                Text(
                                    "Named points for From / Via / To (not full routes).",
                                    style = MaterialTheme.typography.bodySmall,
                                )
                                if (savedPlaces.isEmpty()) {
                                    Text(
                                        "No saved places",
                                        style = MaterialTheme.typography.bodySmall,
                                        modifier = Modifier.testTag("saved_places_empty"),
                                    )
                                } else {
                                    savedPlaces.forEach { place ->
                                        Column(
                                            modifier =
                                                Modifier
                                                    .fillMaxWidth()
                                                    .padding(vertical = 4.dp)
                                                    .testTag("saved_place_row"),
                                        ) {
                                            Text(
                                                place.name,
                                                style = MaterialTheme.typography.bodyMedium,
                                            )
                                            Text(
                                                formatCoordWaypointName(place.lat, place.lon),
                                                style = MaterialTheme.typography.bodySmall,
                                            )
                                            Row(
                                                horizontalArrangement = Arrangement.spacedBy(4.dp),
                                            ) {
                                                TextButton(
                                                    onClick = {
                                                        searchTarget = SearchTarget.From
                                                        applyHit(
                                                            PlaceHit(
                                                                osmId = 0L,
                                                                name = place.name,
                                                                kind = place.kind.ifBlank { "saved-place" },
                                                                lat = place.lat,
                                                                lon = place.lon,
                                                                subArea = "",
                                                                municipality = "",
                                                            ),
                                                        )
                                                    },
                                                    modifier = Modifier.testTag("btn_place_as_from"),
                                                ) { Text("From") }
                                                TextButton(
                                                    onClick = {
                                                        searchTarget = SearchTarget.Via
                                                        applyHit(
                                                            PlaceHit(
                                                                osmId = 0L,
                                                                name = place.name,
                                                                kind = place.kind.ifBlank { "saved-place" },
                                                                lat = place.lat,
                                                                lon = place.lon,
                                                                subArea = "",
                                                                municipality = "",
                                                            ),
                                                        )
                                                    },
                                                    modifier = Modifier.testTag("btn_place_as_via"),
                                                ) { Text("Via") }
                                                TextButton(
                                                    onClick = {
                                                        searchTarget = SearchTarget.To
                                                        applyHit(
                                                            PlaceHit(
                                                                osmId = 0L,
                                                                name = place.name,
                                                                kind = place.kind.ifBlank { "saved-place" },
                                                                lat = place.lat,
                                                                lon = place.lon,
                                                                subArea = "",
                                                                municipality = "",
                                                            ),
                                                        )
                                                    },
                                                    modifier = Modifier.testTag("btn_place_as_to"),
                                                ) { Text("To") }
                                                TextButton(
                                                    onClick = {
                                                        renamePlaceId = place.id
                                                        renamePlaceDraft = place.name
                                                    },
                                                    modifier = Modifier.testTag("btn_rename_saved_place"),
                                                ) { Text("Rename") }
                                                TextButton(
                                                    onClick = {
                                                        if (deleteSavedPlace(dataDir.absolutePath, place.id)) {
                                                            refreshPlaces()
                                                            status = "Deleted saved place"
                                                        } else {
                                                            status = "Could not delete place"
                                                        }
                                                    },
                                                    modifier = Modifier.testTag("btn_delete_saved_place"),
                                                ) { Text("Delete") }
                                            }
                                        }
                                    }
                                }
                                TextButton(
                                    onClick = {
                                        showPlacesPanel = false
                                        hideSearch = true
                                        showTools = false
                                        status = "Route planning closed"
                                    },
                                    modifier = Modifier.testTag("btn_close_places"),
                                ) { Text("Close") }
                            }
                        }
                    }
                } else if (!hideChrome) {
                    // Compact reopen when planning chrome is dismissed.
                    Row(
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .padding(bottom = 8.dp),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Button(
                            onClick = {
                                hideSearch = false
                                showProfilePanel = true
                                showVehiclePanel = true
                                showRoutesPanel = true
                                showPlacesPanel = true
                                refreshPlaces()
                            },
                            modifier = Modifier.testTag("btn_open_search"),
                        ) {
                            Text("Route")
                        }
                        // Same Material3 Button as Route (filled pill), not TextButton —
                        // bare text had no container for shape/clip to show.
                        Button(
                            onClick = { showTools = !showTools },
                            modifier = Modifier.testTag("btn_tools_collapsed"),
                        ) {
                            Text(if (showTools) "Hide tools" else "Tools")
                        }
                    }
                }
            } // end if (!hideChrome)
        }

        if (showTools && !hideChrome) {
            Surface(
                shape = RoundedCornerShape(12.dp),
                tonalElevation = 6.dp,
                modifier =
                    Modifier
                        .align(Alignment.BottomCenter)
                        .fillMaxWidth()
                        .padding(10.dp)
                        .padding(bottom = 88.dp)
                        .heightIn(max = 360.dp)
                        .zIndex(2f)
                        .testTag("tools_menu"),
            ) {
                Column(
                    modifier =
                        Modifier
                            .padding(12.dp)
                            .verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Text("Region", style = MaterialTheme.typography.titleSmall)
                    Text("Map layers: $mapLayerCount", style = MaterialTheme.typography.bodySmall)
                    if (updateReminderDue) {
                        Text(
                            "Weekly OSM update check is due (opt-in reminder — nothing was downloaded).",
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                    Text(
                        "Download scope (Geofabrik)",
                        style = MaterialTheme.typography.labelLarge,
                    )
                    Text(
                        "Countries and bboxes come from Geofabrik's published index. " +
                            "Central America extracts are listed under North America. " +
                            "Jurisdiction packs still follow GPS, not this picker.",
                        style = MaterialTheme.typography.bodySmall,
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        FilterChip(
                            selected = downloadScopeCountry,
                            onClick = {
                                downloadScopeCountry = true
                                val current =
                                    GeofabrikDownloadCatalog.findByPath(selectedGeofabrikPath)
                                val pick =
                                    current
                                        ?: GeofabrikDownloadCatalog
                                            .countriesIn(downloadContinent)
                                            .firstOrNull()
                                        ?: GeofabrikDownloadCatalog.countries.first()
                                downloadContinent = pick.continent
                                selectedGeofabrikPath = pick.path
                            },
                            label = { Text("Country") },
                            modifier = Modifier.testTag("chip_download_country"),
                        )
                        FilterChip(
                            selected = !downloadScopeCountry,
                            onClick = {
                                downloadScopeCountry = false
                                if (GeofabrikDownloadCatalog.hasRegionChips(selectedGeofabrikPath)) {
                                    if (selectedGeofabrikPath == "europe/norway" ||
                                        !selectedGeofabrikPath.startsWith("europe/norway/")
                                    ) {
                                        selectedGeofabrikPath = "europe/norway/ostlandet"
                                    }
                                } else {
                                    // Keep country path; sub-region chips are Norway-only.
                                    val country =
                                        GeofabrikDownloadCatalog.findByPath(selectedGeofabrikPath)
                                    if (country != null) {
                                        selectedGeofabrikPath = country.path
                                    }
                                }
                            },
                            label = { Text("Region in country") },
                            modifier = Modifier.testTag("chip_download_region"),
                        )
                    }
                    if (downloadScopeCountry) {
                        Text(
                            "Country-scale extracts may be slow or fail on low-RAM devices " +
                                "(~4 GB). Prefer a region in country when possible.",
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.testTag("country_download_low_ram_warning"),
                        )
                        Text("Continent", style = MaterialTheme.typography.labelMedium)
                        Row(
                            modifier = Modifier.horizontalScroll(rememberScrollState()),
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            GeofabrikDownloadCatalog.continents.forEach { continent ->
                                FilterChip(
                                    selected = downloadContinent == continent,
                                    onClick = {
                                        downloadContinent = continent
                                        GeofabrikDownloadCatalog
                                            .countriesIn(continent)
                                            .firstOrNull()
                                            ?.let { selectedGeofabrikPath = it.path }
                                    },
                                    label = { Text(continent.label) },
                                    modifier = Modifier.testTag(continent.testTag),
                                )
                            }
                        }
                        Text("Country", style = MaterialTheme.typography.labelMedium)
                        val continentCountries =
                            GeofabrikDownloadCatalog.countriesIn(downloadContinent)
                        if (continentCountries.isEmpty()) {
                            Text(
                                GeofabrikDownloadCatalog.EMPTY_CONTINENT_NOTE,
                                style = MaterialTheme.typography.bodySmall,
                                modifier = Modifier.testTag("continent_empty_note"),
                            )
                        } else {
                            Row(
                                modifier = Modifier.horizontalScroll(rememberScrollState()),
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                            ) {
                                continentCountries.forEach { country ->
                                    FilterChip(
                                        selected = selectedGeofabrikPath == country.path,
                                        onClick = { selectedGeofabrikPath = country.path },
                                        label = { Text(country.label) },
                                        modifier = Modifier.testTag(country.testTag),
                                    )
                                }
                            }
                            GeofabrikDownloadCatalog.findByPath(selectedGeofabrikPath)?.let { country ->
                                if (country.path == selectedGeofabrikPath.trim().trim('/') &&
                                    country.continent == downloadContinent
                                ) {
                                    Text(
                                        country.supportNote,
                                        style = MaterialTheme.typography.bodySmall,
                                        modifier = Modifier.testTag("country_support_note"),
                                    )
                                }
                            }
                        }
                    } else if (GeofabrikDownloadCatalog.hasRegionChips(selectedGeofabrikPath)) {
                        Row(
                            modifier = Modifier.horizontalScroll(rememberScrollState()),
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            GeofabrikDownloadCatalog.norwayRegions.forEach { (slug, label) ->
                                val path = "europe/norway/$slug"
                                FilterChip(
                                    selected = selectedGeofabrikPath == path,
                                    onClick = { selectedGeofabrikPath = path },
                                    label = { Text(label) },
                                )
                            }
                        }
                    } else {
                        Text(
                            GeofabrikDownloadCatalog.regionGranularityNote(selectedGeofabrikPath),
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.testTag("region_chips_norway_only_note"),
                        )
                    }
                    OutlinedTextField(
                        value = selectedGeofabrikPath,
                        onValueChange = { selectedGeofabrikPath = it.trim() },
                        label = { Text("Geofabrik path") },
                        singleLine = true,
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .testTag("field_geofabrik_path"),
                    )
                    Button(
                        onClick = {
                            val path = selectedGeofabrikPath.trim().trim('/')
                            if (path.isEmpty()) {
                                status = "Enter a Geofabrik path (e.g. europe/norway/ostlandet)."
                            } else {
                                startRegionDownload(path)
                            }
                        },
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .testTag("btn_download_region"),
                    ) {
                        Text("Download region + build place index")
                    }
                    Button(
                        onClick = {
                            scope.launch {
                                val pbf =
                                    dataDir.listFiles()?.firstOrNull {
                                        it.isFile && it.name.endsWith(".osm.pbf")
                                    }
                                if (pbf == null) {
                                    status = "No local region PBF to rebuild indexed maps from"
                                    return@launch
                                }
                                val elevDir =
                                    File(dataDir, "elevation").takeIf { it.isDirectory }
                                val before =
                                    withContext(Dispatchers.IO) {
                                        indexedMapsStatus(pbf.absolutePath, dataDir.absolutePath)
                                    }
                                IndexedMapsBackground.ensureStarted(scope, pbf, dataDir, elevDir)
                                status =
                                    "indexed before=$before — rebuild started in background " +
                                    "(region stays usable via PBF fallback)"
                            }
                        },
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .testTag("btn_rebuild_indexed_maps"),
                    ) {
                        Text("Rebuild indexed maps (local PBF, background)")
                    }
                    if (indexedMapsUiLine.isNotBlank()) {
                        Text(
                            indexedMapsUiLine,
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.testTag("indexed_maps_bg_status"),
                        )
                    }
                    if (placeIndexUiLine.isNotBlank()) {
                        Text(
                            placeIndexUiLine,
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.testTag("place_index_bg_status"),
                        )
                    }
                    if (regionDownloadProgress.isNotBlank()) {
                        Text(
                            regionDownloadProgress,
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.testTag("region_download_progress"),
                        )
                    }
                    Text(
                        "Basemap (PMTiles) — range-extract from Protomaps planet",
                        style = MaterialTheme.typography.labelLarge,
                    )
                    OutlinedTextField(
                        value = pmtilesBaseUrl,
                        onValueChange = {
                            pmtilesBaseUrl = it.trim()
                            MapHudPrefs.savePmtilesBaseUrl(context, pmtilesBaseUrl)
                        },
                        label = { Text("Planet PMTiles URL (blank = latest Protomaps)") },
                        singleLine = true,
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .testTag("field_pmtiles_base_url"),
                    )
                    if (offlineIntegrity.canRestoreFromStaging) {
                        Text(
                            offlineIntegrity.userMessage()
                                ?: "Staged offline map files are available to restore.",
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.testTag("offline_data_mismatch_msg"),
                        )
                        Button(
                            onClick = {
                                scope.launch {
                                    status = "Restoring staged offline maps…"
                                    val report =
                                        withContext(Dispatchers.IO) {
                                            OfflinePmtilesBootstrap.restoreOstlandetFromStaging(dataDir)
                                        }
                                    status = report
                                    if (report.startsWith("OK:")) {
                                        MapHudPrefs.rememberDownloadedPmtilesRegion(
                                            context,
                                            "europe_norway_ostlandet",
                                        )
                                        offlineIntegrity =
                                            OfflineDataIntegrity.inspect(context, dataDir)
                                        styleEpoch += 1
                                    }
                                }
                            },
                            modifier =
                                Modifier
                                    .fillMaxWidth()
                                    .testTag("btn_restore_staged_pmtiles"),
                        ) {
                            Text("Restore staged offline maps")
                        }
                    }
                    Button(
                        onClick = {
                            scope.launch {
                                val path = selectedGeofabrikPath.trim().trim('/')
                                if (path.isEmpty()) {
                                    status = "Select a Geofabrik path first."
                                    return@launch
                                }
                                val base = pmtilesBaseUrl.ifBlank { null }
                                downloadProgressClear()
                                status = "Extracting PMTiles for $path from Protomaps..."
                                val job =
                                    withContext(Dispatchers.IO) {
                                        pmtilesQueueRegion(
                                            dataDir.absolutePath,
                                            path,
                                            base,
                                        )
                                    }
                                if (job.id.isBlank() || job.status.startsWith("failed")) {
                                    status = "PMTiles queue failed: ${job.status}"
                                    return@launch
                                }
                                pmtilesJobId = job.id
                                pmtilesProgress = "Downloading map tiles for region… 0%"
                                downloadPolling = true
                                status = "Downloading basemap ${job.regionKey} (range extract)..."
                                val done =
                                    withContext(Dispatchers.IO) {
                                        pmtilesRunJob(dataDir.absolutePath, job.id)
                                    }
                                downloadPolling = false
                                pmtilesProgress =
                                    formatProgressPct(
                                        done.bytesReceived,
                                        done.totalBytes,
                                        "Downloading map tiles for region…",
                                    )
                                status = "PMTiles ${done.status}: ${done.localPath}"
                                if (done.status == "completed") {
                                    MapHudPrefs.rememberDownloadedPmtilesRegion(
                                        context,
                                        done.regionKey.ifBlank {
                                            File(done.localPath).nameWithoutExtension
                                        },
                                    )
                                    offlineIntegrity = OfflineDataIntegrity.inspect(context, dataDir)
                                    styleEpoch += 1
                                }
                            }
                        },
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .testTag("btn_download_pmtiles"),
                    ) {
                        Text("Download basemap (PMTiles)")
                    }
                    Button(
                        onClick = {
                            scope.launch {
                                val path = selectedGeofabrikPath.trim().trim('/')
                                if (path.isEmpty()) {
                                    status = "Select a Geofabrik path first."
                                    return@launch
                                }
                                downloadProgressClear()
                                status = "Extracting Mapterhorn DEM for $path..."
                                val job =
                                    withContext(Dispatchers.IO) {
                                        uniffi.navi.pmtilesQueueDemRegion(
                                            dataDir.absolutePath,
                                            path,
                                        )
                                    }
                                if (job.id.isBlank() || job.status.startsWith("failed")) {
                                    status = "DEM queue failed: ${job.status}"
                                    return@launch
                                }
                                pmtilesJobId = job.id
                                pmtilesProgress = "Downloading terrain DEM… 0%"
                                downloadPolling = true
                                val done =
                                    withContext(Dispatchers.IO) {
                                        pmtilesRunJob(dataDir.absolutePath, job.id)
                                    }
                                downloadPolling = false
                                pmtilesProgress =
                                    formatProgressPct(
                                        done.bytesReceived,
                                        done.totalBytes,
                                        "Downloading terrain DEM…",
                                    )
                                status = "DEM ${done.status}: ${done.localPath}"
                                if (done.status == "completed") {
                                    MapHudPrefs.rememberDownloadedPmtilesRegion(
                                        context,
                                        done.regionKey.ifBlank {
                                            File(done.localPath).nameWithoutExtension
                                        },
                                    )
                                    offlineIntegrity = OfflineDataIntegrity.inspect(context, dataDir)
                                    styleEpoch += 1
                                }
                            }
                        },
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .testTag("btn_download_dem"),
                    ) {
                        Text("Download terrain DEM (Mapterhorn)")
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        TextButton(
                            onClick = {
                                pmtilesJobId?.let { id ->
                                    pmtilesPauseJob(id)
                                    status = "PMTiles paused"
                                }
                            },
                            enabled = pmtilesJobId != null,
                            modifier = Modifier.testTag("btn_pmtiles_pause"),
                        ) { Text("Pause") }
                        TextButton(
                            onClick = {
                                val id = pmtilesJobId ?: return@TextButton
                                // Only clear the pause flag — do not start a second run_job
                                // (that raced the paused extract and prevented resume).
                                pmtilesResumeJob(id)
                                status = "PMTiles resuming…"
                                downloadPolling = true
                            },
                            enabled = pmtilesJobId != null,
                            modifier = Modifier.testTag("btn_pmtiles_resume"),
                        ) { Text("Resume") }
                        TextButton(
                            onClick = {
                                pmtilesJobId?.let { id ->
                                    pmtilesCancelJob(id)
                                    status = "PMTiles cancel requested"
                                }
                            },
                            enabled = pmtilesJobId != null,
                            modifier = Modifier.testTag("btn_pmtiles_cancel"),
                        ) { Text("Cancel") }
                    }
                    if (pmtilesProgress.isNotBlank()) {
                        Text(
                            pmtilesProgress,
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.testTag("pmtiles_progress"),
                        )
                    }
                    Text(
                        "OSM updates (Geofabrik) — opt-in, never silent",
                        style = MaterialTheme.typography.labelLarge,
                    )
                    Button(
                        onClick = {
                            scope.launch {
                                val raw =
                                    withContext(Dispatchers.IO) {
                                        checkOsmUpdates(dataDir.absolutePath)
                                    }
                                pendingUpdatePlan = raw
                                status = OsmUpdateUserCopy.forCheckReport(raw)
                                updateReminderDue = osmWeeklyReminderDue(dataDir.absolutePath)
                            }
                        },
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .testTag("btn_check_osm_updates"),
                    ) {
                        Text("Check for OSM updates")
                    }
                    Button(
                        onClick = {
                            scope.launch {
                                val plan = pendingUpdatePlan
                                if (plan.isNullOrBlank() || plan.contains("up to date", ignoreCase = true)) {
                                    status = OsmUpdateUserCopy.NEED_CHECK
                                    return@launch
                                }
                                if (plan.contains("Unsupported", ignoreCase = true) ||
                                    plan.contains("unsupported", ignoreCase = true)
                                ) {
                                    status = OsmUpdateUserCopy.NO_BINDING
                                    return@launch
                                }
                                status = OsmUpdateUserCopy.APPLYING
                                val raw =
                                    withContext(Dispatchers.IO) {
                                        applyOsmUpdate(dataDir.absolutePath)
                                    }
                                pendingUpdatePlan = null
                                // applyOsmUpdate clears place_index + graph-cache and
                                // fingerprints the new PBF so packs become stale_pbf.
                                // Mirror the download button: rebuild index + queue packs.
                                if (raw.contains("PASS", ignoreCase = true)) {
                                    val pbf = resolveRegionPbf()
                                    if (pbf != null && pbf.isFile) {
                                        withContext(Dispatchers.IO) {
                                            ensurePlaceIndex(
                                                pbf.absolutePath,
                                                placeIndexDbForWrite().absolutePath,
                                            )
                                        }
                                        val elevDir =
                                            File(dataDir, "elevation").takeIf { it.isDirectory }
                                        IndexedMapsBackground.ensureStarted(
                                            scope,
                                            pbf,
                                            dataDir,
                                            elevDir,
                                        )
                                        status = OsmUpdateUserCopy.UPDATED_INDEXING
                                    } else {
                                        status = OsmUpdateUserCopy.UPDATED
                                    }
                                } else {
                                    status = OsmUpdateUserCopy.forApplyReport(raw)
                                }
                            }
                        },
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .testTag("btn_apply_osm_update"),
                        enabled = !pendingUpdatePlan.isNullOrBlank(),
                    ) {
                        Text("Apply pending OSM update")
                    }
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Text("Weekly update reminder (no auto-download)")
                        Switch(
                            checked = weeklyReminder,
                            onCheckedChange = { on ->
                                weeklyReminder = on
                                setOsmWeeklyReminder(dataDir.absolutePath, on)
                                updateReminderDue = osmWeeklyReminderDue(dataDir.absolutePath)
                                status =
                                    if (on) {
                                        "Weekly OSM check reminder enabled"
                                    } else {
                                        "Weekly OSM check reminder disabled"
                                    }
                            },
                        )
                    }
                    Text("Diagnostics", style = MaterialTheme.typography.titleSmall)
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Text("Diagnostic logging")
                        Switch(
                            checked = diagnosticLogging,
                            onCheckedChange = { on ->
                                DiagnosticLog.setEnabled(context, on)
                                diagnosticLogging = on
                                if (on) {
                                    DiagnosticLog.maybeLogSystem(context.filesDir, nowMs = 0L)
                                    status =
                                        "Diagnostic logging on — " +
                                        DiagnosticLog.publicLocationDescription()
                                } else {
                                    status = "Diagnostic logging off"
                                }
                            },
                            modifier = Modifier.testTag("toggle_diagnostic_logging"),
                        )
                    }
                    Text(
                        "Writes a dated session log under Documents/debug " +
                            "(USB/MTP: Internal storage → Documents → debug). " +
                            "GPS, toggles, route plan, eco, POIs, pauses, instructions, " +
                            "fuel, system. Off by default; not uploaded.",
                        style = MaterialTheme.typography.bodySmall,
                    )
                    Button(
                        onClick = {
                            val ok = DiagnosticLog.shareLatest(context)
                            status =
                                if (ok) {
                                    "Share sheet opened for diagnostic log"
                                } else {
                                    "No diagnostic log file yet — turn logging on first"
                                }
                        },
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .testTag("btn_export_diagnostic_log"),
                        enabled =
                            diagnosticLogging ||
                                DiagnosticLog.listSessionFiles(context).isNotEmpty(),
                    ) {
                        Text("Export diagnostic log")
                    }
                    Text(
                        userFacingStatus(status),
                        style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.testTag("tools_status"),
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Button(
                            onClick = {
                                MapHudPrefs.savePmtilesBaseUrl(context, pmtilesBaseUrl)
                                MapHudPrefs.saveGeofabrikPath(context, selectedGeofabrikPath)
                                DiagnosticLog.logSettingSaved("pmtiles_base_url", pmtilesBaseUrl)
                                DiagnosticLog.logSettingSaved("geofabrik_path", selectedGeofabrikPath)
                                status = "Tools settings saved"
                            },
                            modifier = Modifier.testTag("btn_save_tools"),
                        ) { Text("Save") }
                        TextButton(
                            onClick = { showTools = false },
                            modifier = Modifier.testTag("btn_close_tools"),
                        ) { Text("Close") }
                    }
                }
            }
        }

        if (!hideChrome) {
            Column(
                modifier =
                    Modifier
                        .align(Alignment.BottomCenter)
                        .fillMaxWidth()
                        .windowInsetsPadding(
                            WindowInsets.safeDrawing.only(
                                WindowInsetsSides.Bottom + WindowInsetsSides.Horizontal,
                            ),
                        ).padding(10.dp)
                        .zIndex(1f),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                BottomDriveHud(
                    state = driveHud.copy(ecoActive = ecoEnabled),
                    iconDir = iconsDir.absolutePath,
                    routePlanned = mapState.polyline.isNotBlank(),
                    showRecenter =
                        !mapState.followGps &&
                            (mapState.gpsLat != 0.0 || mapState.gpsLon != 0.0),
                    onZoomIn = {
                        val z = (mapState.cameraZoom ?: 12.0) + 1.0
                        val next = z.coerceAtMost(20.0)
                        val hasGps = mapState.gpsLat != 0.0 || mapState.gpsLon != 0.0
                        mapState =
                            if (mapState.followGps && hasGps) {
                                mapState.copy(
                                    cameraZoom = next,
                                    cameraLat = mapState.gpsLat,
                                    cameraLon = mapState.gpsLon,
                                )
                            } else {
                                // Preserve the panned center; only change zoom.
                                mapState.copy(cameraZoom = next)
                            }
                        NaviMapTestHooks.lastCameraZoom = next
                    },
                    onZoomOut = {
                        val z = (mapState.cameraZoom ?: 12.0) - 1.0
                        val next = z.coerceAtLeast(3.0)
                        val hasGps = mapState.gpsLat != 0.0 || mapState.gpsLon != 0.0
                        mapState =
                            if (mapState.followGps && hasGps) {
                                mapState.copy(
                                    cameraZoom = next,
                                    cameraLat = mapState.gpsLat,
                                    cameraLon = mapState.gpsLon,
                                )
                            } else {
                                mapState.copy(cameraZoom = next)
                            }
                        NaviMapTestHooks.lastCameraZoom = next
                    },
                    onRecenter = {
                        val hasGps = mapState.gpsLat != 0.0 || mapState.gpsLon != 0.0
                        if (!hasGps) return@BottomDriveHud
                        mapState =
                            mapState.copy(
                                followGps = true,
                                cameraLat = mapState.gpsLat,
                                cameraLon = mapState.gpsLon,
                                layerEpoch = mapState.layerEpoch + 1,
                            )
                        NaviMapTestHooks.followGps = true
                        DiagnosticLog.logToggle("follow_gps", true)
                    },
                    onOpenSettings = {
                        showDriveSettings = !showDriveSettings
                        if (showDriveSettings) showMapSettings = false
                        NaviMapTestHooks.driveSettingsOpen = showDriveSettings
                    },
                )
            }

            // Status chip: short user-facing messages only (never pipeline debug dumps).
            val toast = userFacingStatus(status)
            if (toast.isNotBlank() && !toast.equals("Ready", ignoreCase = true)) {
                Text(
                    text = toast,
                    modifier =
                        Modifier
                            .align(Alignment.BottomEnd)
                            .zIndex(3f)
                            .padding(end = 12.dp, bottom = 88.dp)
                            .background(Color(0xCCFFFFFF), RoundedCornerShape(8.dp))
                            .padding(horizontal = 10.dp, vertical = 8.dp)
                            .testTag("status_toast"),
                    style = MaterialTheme.typography.bodySmall,
                )
            }

            if (showMapSettings) {
                MapSettingsSheet(
                    state = driveHud.copy(ecoActive = ecoEnabled),
                    onRotation = { mode ->
                        DiagnosticLog.logToggle("rotation_mode", mode.name)
                        cancelPendingRotationSnap()
                        manualRotationSticky = false
                        NaviMapTestHooks.manualRotationOverrideActive = false
                        driveHud = driveHud.copy(rotationMode = mode)
                        NaviMapTestHooks.lastRotationMode = mode
                        val bearing =
                            when (mode) {
                                MapRotationMode.NorthUp -> 0.0
                                MapRotationMode.Compass ->
                                    NaviMapTestHooks.magneticHeadingDeg ?: mapState.cameraBearing
                                MapRotationMode.DirectionOfTravel ->
                                    NaviMapTestHooks.gpsBearingDeg ?: mapState.cameraBearing
                            }
                        if (NaviMapTestHooks.applyBearingToMap) {
                            mapState = mapState.copy(cameraBearing = bearing)
                            bearingApplyEpoch += 1
                        }
                        NaviMapTestHooks.lastCameraBearing = bearing
                    },
                    onToggleSnapRotationBack = { on ->
                        DiagnosticLog.logToggle("snap_rotation_back", on)
                        driveHud = driveHud.copy(snapRotationBackToMode = on)
                        MapHudPrefs.saveSnapRotationBack(context, on)
                        NaviMapTestHooks.lastSnapRotationBack = on
                        if (on) {
                            manualRotationSticky = false
                            reassertModeBearing(force = true)
                        }
                    },
                    onToggleTripEta = { on ->
                        DiagnosticLog.logToggle("trip_eta", on)
                        driveHud =
                            driveHud.copy(
                                showTripEta = on,
                                tripEtaMinutes =
                                    when {
                                        on && driveHud.tripEtaMinutes == null -> 95.0
                                        else -> driveHud.tripEtaMinutes
                                    },
                            )
                    },
                    onToggleBreakReminders = { on ->
                        DiagnosticLog.logToggle("break_reminders", on)
                        driveHud = driveHud.copy(breakRemindersEnabled = on)
                    },
                    onToggleAutoZoom = { on ->
                        DiagnosticLog.logToggle("auto_zoom", on)
                        driveHud = driveHud.copy(autoZoomWhileMoving = on)
                        MapHudPrefs.saveAutoZoom(
                            context,
                            driveHud.autoZoomLevel,
                            enabled = on,
                        )
                        if (on) {
                            mapState =
                                mapState.copy(
                                    cameraZoom = driveHud.autoZoomLevel,
                                    layerEpoch = mapState.layerEpoch + 1,
                                )
                            NaviMapTestHooks.lastCameraZoom = driveHud.autoZoomLevel
                        }
                    },
                    onAutoZoomLevelChange = { level ->
                        val next = MapHudPrefs.clampZoom(level)
                        driveHud = driveHud.copy(autoZoomLevel = next)
                        MapHudPrefs.saveAutoZoom(
                            context,
                            next,
                            enabled = driveHud.autoZoomWhileMoving,
                        )
                        if (driveHud.autoZoomWhileMoving) {
                            mapState =
                                mapState.copy(
                                    cameraZoom = next,
                                    layerEpoch = mapState.layerEpoch + 1,
                                )
                            NaviMapTestHooks.lastCameraZoom = next
                        }
                    },
                    onToggle3d = { on ->
                        if (on && !driveHud.vulkanAvailable) {
                            status = "3D requires Vulkan renderer"
                            return@MapSettingsSheet
                        }
                        DiagnosticLog.logToggle("opt_in_3d", on)
                        driveHud = driveHud.copy(optIn3d = on)
                        MapHudPrefs.saveOptIn3d(context, on)
                        styleEpoch += 1
                    },
                    onToggleContours = { on ->
                        DiagnosticLog.logToggle("contours_enabled", on)
                        driveHud = driveHud.copy(contoursEnabled = on)
                        MapHudPrefs.saveContoursEnabled(context, on)
                        styleEpoch += 1
                    },
                    onCameraTiltChange = { deg ->
                        val next =
                            if (driveHud.vulkanAvailable) {
                                MapHudPrefs.snapTilt(deg)
                            } else {
                                0.0
                            }
                        DiagnosticLog.logToggle("camera_tilt_deg", next)
                        driveHud = driveHud.copy(cameraTiltDeg = next)
                        MapHudPrefs.saveCameraTiltDeg(context, next)
                        // Do not write lastCameraPitch here — only MapLibre camera state may.
                        styleEpoch += 1
                    },
                    onSave = {
                        MapHudPrefs.saveAutoZoom(
                            context,
                            driveHud.autoZoomLevel,
                            enabled = driveHud.autoZoomWhileMoving,
                        )
                        MapHudPrefs.saveOptIn3d(context, driveHud.optIn3d)
                        MapHudPrefs.saveContoursEnabled(context, driveHud.contoursEnabled)
                        MapHudPrefs.saveCameraTiltDeg(context, driveHud.cameraTiltDeg)
                        DiagnosticLog.logSettingSaved("map_hud_auto_zoom_level", driveHud.autoZoomLevel)
                        DiagnosticLog.logSettingSaved("map_hud_opt_in_3d", driveHud.optIn3d)
                        DiagnosticLog.logSettingSaved("map_hud_contours_enabled", driveHud.contoursEnabled)
                        DiagnosticLog.logSettingSaved("map_hud_camera_tilt_deg", driveHud.cameraTiltDeg)
                        // Re-apply basemap so Save after toggling 3D always refreshes
                        // hillshade/tilt (toggle alone can race the style callback).
                        styleEpoch += 1
                        showMapSettings = false
                        NaviMapTestHooks.mapSettingsOpen = false
                        status = "Map settings saved"
                    },
                    onClose = {
                        showMapSettings = false
                        NaviMapTestHooks.mapSettingsOpen = false
                    },
                    modifier =
                        Modifier
                            .align(Alignment.TopCenter)
                            .zIndex(4f)
                            .padding(10.dp),
                )
            }

            if (showDriveSettings) {
                DriveSettingsSheet(
                    dataDir = dataDir.absolutePath,
                    iconDir = iconsDir.absolutePath,
                    travelProfile = profile,
                    onTravelProfileChange = { p ->
                        profile = p
                        ecoEnabled = ecoModeDefault(p)
                        driveHud = driveHud.copy(ecoActive = ecoEnabled)
                        status = "Profile: ${p.name.lowercase()}"
                    },
                    ecoActive = ecoEnabled,
                    onEcoChange = {
                        ecoEnabled = it
                        driveHud = driveHud.copy(ecoActive = it)
                        DiagnosticLog.logToggle(
                            "eco_mode",
                            it,
                            mapOf("profile" to profile.name),
                        )
                    },
                    breakAsDistance = driveHud.breakAsDistance,
                    onBreakAsDistanceChange = { on ->
                        MapHudPrefs.saveBreakAsDistance(context, on)
                        driveHud = driveHud.copy(breakAsDistance = on)
                        DiagnosticLog.logToggle("break_as_distance", on)
                        DiagnosticLog.logSettingSaved("break_as_distance", on)
                    },
                    unitSystem = driveHud.unitSystem,
                    onUnitSystemChange = { system ->
                        MapHudPrefs.saveUnitSystem(context, system)
                        driveHud = driveHud.copy(unitSystem = system)
                        DiagnosticLog.logToggle("unit_system", system.persistId)
                        DiagnosticLog.logSettingSaved("unit_system", system.persistId)
                    },
                    onApplied = {
                        showDriveSettings = false
                        NaviMapTestHooks.driveSettingsOpen = false
                        status = "Drive settings saved"
                        driveHud = driveHud.copy(ecoActive = ecoEnabled)
                        runCatching {
                            val intervalH =
                                breakIntervalHoursForProfile(
                                    dataDir.absolutePath,
                                    profile,
                                )
                            val routeActive = mapState.polyline.isNotBlank()
                            val minsLeft =
                                if (routeActive) {
                                    ((intervalH - drivingHoursSinceBreak) * 60.0)
                                        .coerceAtLeast(0.0)
                                } else {
                                    null
                                }
                            val ecoFromStore =
                                if (usesTruckRestSettings(profile)) {
                                    loadTruckRestSettings(dataDir.absolutePath).ecoModeEnabled
                                } else {
                                    loadCarRestSettings(dataDir.absolutePath).ecoModeEnabled
                                }
                            driveHud =
                                driveHud.copy(
                                    ecoActive = ecoFromStore || ecoEnabled,
                                    minutesToBreak = minsLeft,
                                    breakAsDistance = MapHudPrefs.loadBreakAsDistance(context),
                                    unitSystem = MapHudPrefs.loadUnitSystem(context),
                                )
                            ecoEnabled = ecoFromStore || ecoEnabled
                        }
                    },
                    onDismiss = {
                        showDriveSettings = false
                        NaviMapTestHooks.driveSettingsOpen = false
                    },
                    modifier =
                        Modifier
                            .align(Alignment.BottomCenter)
                            .zIndex(4f)
                            .padding(10.dp),
                )
            }
        }
    }
}

@Composable
private fun SimulatingBannerOverlay(
    active: Boolean,
    modifier: Modifier = Modifier,
) {
    // Hook mirror covers seek/start races where Compose `simulating` lags one frame.
    var hookActive by remember { mutableStateOf(NaviMapTestHooks.simulatingActive) }
    LaunchedEffect(Unit) {
        while (isActive) {
            val next = NaviMapTestHooks.simulatingActive
            if (next != hookActive) hookActive = next
            delay(50)
        }
    }
    if (!active && !hookActive) return
    Surface(
        color = Color(0xFFB71C1C),
        shape = RoundedCornerShape(6.dp),
        shadowElevation = 6.dp,
        modifier =
            modifier
                .testTag("simulating_banner")
                .semantics { contentDescription = "SIMULATING" },
    ) {
        Text(
            "SIMULATING",
            color = Color.White,
            style = MaterialTheme.typography.titleMedium,
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
        )
    }
}

@Composable
private fun RecalculatingRouteBanner(
    active: Boolean,
    title: String,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var hookActive by remember { mutableStateOf(NaviMapTestHooks.reroutingActive) }
    LaunchedEffect(Unit) {
        while (isActive) {
            val next = NaviMapTestHooks.reroutingActive
            if (next != hookActive) hookActive = next
            delay(50)
        }
    }
    if (!active && !hookActive) return
    Surface(
        color = Color(0xFFE65100),
        shape = RoundedCornerShape(6.dp),
        shadowElevation = 6.dp,
        modifier =
            modifier
                .testTag("rerouting_banner")
                .semantics { contentDescription = title },
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
        ) {
            Text(
                title,
                color = Color.White,
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.padding(end = 8.dp),
            )
            TextButton(onClick = onCancel, modifier = Modifier.testTag("btn_cancel_reroute")) {
                Text("Cancel", color = Color.White)
            }
        }
    }
}

@OptIn(ExperimentalComposeUiApi::class)
@Composable
private fun CorridorMapView(
    state: MapRouteState,
    dataDir: java.io.File,
    prefer3d: Boolean,
    contoursEnabled: Boolean,
    cameraTiltDeg: Double,
    vulkanAvailable: Boolean,
    unitSystem: UnitSystem,
    styleEpoch: Int,
    bearingEpoch: Int = 0,
    modifier: Modifier = Modifier,
    onLayerCount: (Int) -> Unit,
    onUserPan: () -> Unit = {},
    onMapLongPress: (lat: Double, lon: Double) -> Unit = { _, _ -> },
    onUserRotate: (bearingDeg: Double) -> Unit = {},
    onCameraIdleTarget: (lat: Double, lon: Double, zoom: Double) -> Unit = { _, _, _ -> },
    onStyleNote: (String?) -> Unit = {},
    on3dFailed: () -> Unit = {},
) {
    val context = LocalContext.current
    val mapView =
        remember {
            MapView(context).apply {
                setBackgroundColor(android.graphics.Color.parseColor("#E8EEF2"))
                onCreate(null)
            }
        }
    var mapRef by remember { mutableStateOf<MapLibreMap?>(null) }

    data class OverlayMark(
        val x: Float,
        val y: Float,
        val track: TrackMarker,
        val icon: Bitmap?,
    )

    data class WaypointOverlayMark(
        val x: Float,
        val y: Float,
        val name: String,
    )

    data class GpsOverlayMark(
        val x: Float,
        val y: Float,
    )
    var overlayMarks by remember { mutableStateOf<List<OverlayMark>>(emptyList()) }
    var waypointMarks by remember { mutableStateOf<List<WaypointOverlayMark>>(emptyList()) }
    var gpsMark by remember { mutableStateOf<GpsOverlayMark?>(null) }
    val iconCache = remember { mutableMapOf<String, Bitmap>() }
    val styleReady = remember { androidx.compose.runtime.mutableStateOf(false) }
    val styleLoadStarted =
        remember {
            java.util.concurrent.atomic
                .AtomicBoolean(false)
        }
    val currentStyleUri = remember { androidx.compose.runtime.mutableStateOf<String?>(null) }
    val currentStyleKind =
        remember {
            androidx.compose.runtime.mutableStateOf<BasemapStyleResolver.StyleKind?>(null)
        }
    // Drop stale MapLibre getStyle/setStyle callbacks so a prior 3D apply cannot
    // re-tilt after the user has already switched back to flat.
    val styleApplyGen =
        remember {
            java.util.concurrent.atomic
                .AtomicInteger(0)
        }
    val stateRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference(state)
        }
    stateRef.set(state)
    val onUserPanRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference(onUserPan)
        }
    onUserPanRef.set(onUserPan)
    val onMapLongPressRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference(onMapLongPress)
        }
    onMapLongPressRef.set(onMapLongPress)
    val mapScope = rememberCoroutineScope()
    var holdJob by remember { mutableStateOf<Job?>(null) }
    var holdProgress by remember { mutableFloatStateOf(0f) }
    var holdScreenX by remember { mutableFloatStateOf(0f) }
    var holdScreenY by remember { mutableFloatStateOf(0f) }
    var holdActive by remember { mutableStateOf(false) }
    val onUserRotateRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference(onUserRotate)
        }
    onUserRotateRef.set(onUserRotate)
    val onCameraIdleRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference(onCameraIdleTarget)
        }
    onCameraIdleRef.set(onCameraIdleTarget)
    // Camera-idle / one-shot getMapAsync listeners capture applyResolvedStyle once;
    // always read the live 3D preference from these refs (not the Compose closure).
    val prefer3dRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference(prefer3d)
        }
    val contoursEnabledRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference(contoursEnabled)
        }
    val vulkanRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference(vulkanAvailable)
        }
    val tiltRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference(cameraTiltDeg)
        }
    val unitSystemRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference(unitSystem)
        }
    prefer3dRef.set(prefer3d)
    contoursEnabledRef.set(contoursEnabled)
    vulkanRef.set(vulkanAvailable)
    tiltRef.set(cameraTiltDeg)
    unitSystemRef.set(unitSystem)

    fun effectiveTiltDeg(): Double {
        val liveVulkan = vulkanRef.get()
        if (!liveVulkan) return 0.0
        return MapHudPrefs.snapTilt(tiltRef.get())
    }

    fun applyCameraTilt(
        map: MapLibreMap,
        tiltDeg: Double = effectiveTiltDeg(),
    ) {
        val target =
            if (vulkanRef.get()) {
                MapHudPrefs.snapTilt(tiltDeg)
            } else {
                0.0
            }
        // Keep MapLibre's clamp aligned with our presets (default is already 60,
        // but re-assert in case a style/options path lowered it).
        runCatching {
            map.setMinPitchPreference(0.0)
            map.setMaxPitchPreference(MapHudPrefs.MAPLIBRE_MAX_TILT_DEG)
        }
        val cur = map.cameraPosition.tilt
        if (kotlin.math.abs(cur - target) < 0.25) {
            NaviMapTestHooks.lastCameraPitch = cur
            return
        }
        val pos =
            org.maplibre.android.camera.CameraPosition
                .Builder()
                .target(map.cameraPosition.target)
                .zoom(map.cameraPosition.zoom)
                .bearing(map.cameraPosition.bearing)
                .tilt(target)
                .padding(map.cameraPosition.padding)
                .build()
        runCatching { map.cancelTransitions() }
        map.moveCamera(CameraUpdateFactory.newCameraPosition(pos))
        // Prefer the engine's post-clamp pitch so hooks/UI stay truthful.
        NaviMapTestHooks.lastCameraPitch = map.cameraPosition.tilt
    }

    fun applyTerrainAndPitch(
        map: MapLibreMap,
        style: Style,
        resolved: BasemapStyleResolver.ResolvedStyle,
        applyGen: Int,
    ) {
        if (applyGen != styleApplyGen.get()) return
        var terrainOk = false
        if (resolved.attachMapterhornTerrain) {
            val demUri = resolved.demSourceUri ?: MapterhornTerrain.TILEJSON_URL
            terrainOk = MapterhornTerrain.attach(style, demUri)
            if (!terrainOk) {
                onStyleNote(
                    "Mapterhorn hillshade unavailable; tilt still applied",
                )
            }
        } else if (MapterhornTerrain.usesBakedOfflineHillshade(resolved)) {
            resolved.coveringJob?.localPath?.let { basemapPath ->
                MapterhornTerrain.localDemBesideBasemap(basemapPath)?.let { dem ->
                    MapterhornTerrain.ensureLocalDemTileJsonUrl(dem)
                }
            }
            val demUri = resolved.demSourceUri
            terrainOk =
                if (demUri != null) {
                    MapterhornTerrain.attach(style, demUri)
                } else {
                    MapterhornTerrain.isAttached(style)
                }
            if (!terrainOk) {
                onStyleNote("Offline hillshade missing from prepared style")
            }
        } else {
            // Strip hillshade when 3D is off — setStyle(same Liberty URL)
            // may no-op and leave a previous attach in place.
            MapterhornTerrain.detach(style)
            terrainOk = false
        }
        var contoursOk = false
        if (contoursEnabledRef.get()) {
            val config = DemTileFetcher.Config.fromResolved(context, resolved)
            if (config != null) {
                contoursOk = MapterhornContours.attach(style, DemTileFetcher(config), unitSystemRef.get())
                if (!contoursOk) {
                    onStyleNote("Contour lines unavailable")
                }
            } else {
                MapterhornContours.detach(style)
                onStyleNote("Contour lines need Mapterhorn DEM (download terrain or go online)")
            }
        } else {
            MapterhornContours.detach(style)
        }
        if (applyGen != styleApplyGen.get()) return
        NaviMapTestHooks.lastTerrainAttached = terrainOk
        NaviMapTestHooks.lastContoursAttached = contoursOk
        // setStyle / offline 3D JSON wipe GeoJSON overlays — re-apply immediately
        // so the corridor is not missing while layerEpoch catches up.
        applyRouteToStyle(style, stateRef.get())
        applyTracksToStyle(style, stateRef.get().tracks, mapView.context)
        BasemapLabelPolicy.apply(style)
        BasemapPathPaint.apply(style)
        BasemapProtectedAreaStyle.apply(style)
        BasemapHousenumberStyle.apply(style)
        BasemapGlacierOutlineStyle.apply(style)
        BasemapPeakElevationStyle.apply(style, unitSystemRef.get())
        BasemapContourLabelStyle.apply(style, unitSystemRef.get())
        ensureRouteAboveHillshade(style)
        applyCameraTilt(map)
        styleReady.value = true
        NaviMapTestHooks.styleReady = true
        NaviMapTestHooks.lastBasemapKind = currentStyleKind.value?.name ?: resolved.kind.name
        map.triggerRepaint()
    }

    fun applyResolvedStyle(
        map: MapLibreMap,
        force: Boolean = false,
    ) {
        val livePrefer3d = prefer3dRef.get()
        val liveVulkan = vulkanRef.get()
        val want3d = livePrefer3d && liveVulkan
        val latest = stateRef.get()
        // Prefer Compose camera target (pendingCamera) over the live MapLibre
        // camera so style switches are not delayed one idle frame behind state.
        val lat =
            latest.cameraLat
                ?: map.cameraPosition.target?.latitude
                ?: 60.0
        val lon =
            latest.cameraLon
                ?: map.cameraPosition.target?.longitude
                ?: 10.0
        val resolved =
            BasemapStyleResolver.resolve(
                context = context,
                dataDir = dataDir,
                lat = lat,
                lon = lon,
                prefer3d = livePrefer3d,
                vulkanAvailable = liveVulkan,
                forceOnline2d = NaviMapTestHooks.forceOnlineBasemap,
            )
        val sameUri = resolved.styleUri == currentStyleUri.value
        val sameKind = resolved.kind == currentStyleKind.value

        // Coverage-only idle callbacks: if URI/kind unchanged and terrain/tilt
        // already match, do not bump applyGen (that would cancel an in-flight 3D attach).
        if (!force && sameUri && sameKind) {
            map.getStyle { style ->
                if (style == null) return@getStyle
                val attached = MapterhornTerrain.isAttached(style)
                val tilt = map.cameraPosition.tilt
                // Compare against the achievable tilt (MapLibre clamps at 60°).
                val wantTilt = effectiveTiltDeg()
                val tiltMatches = kotlin.math.abs(tilt - wantTilt) <= 1.0
                val wantTerrain = MapterhornTerrain.wantHillshadeAttached(resolved)
                val terrainMatches = attached == wantTerrain
                val wantContours = contoursEnabledRef.get()
                val contoursAttached = MapterhornContours.isAttached(style)
                val contoursMatches = contoursAttached == wantContours
                if (terrainMatches && tiltMatches && contoursMatches) return@getStyle
                val applyGen = styleApplyGen.incrementAndGet()
                applyTerrainAndPitch(map, style, resolved, applyGen)
            }
            return
        }

        val applyGen = styleApplyGen.incrementAndGet()
        // Tilt is independent of hillshade 3D; apply preferred tilt (0 when no Vulkan).
        applyCameraTilt(map)
        if (!want3d) {
            NaviMapTestHooks.lastTerrainAttached = false
        }
        styleReady.value = false
        NaviMapTestHooks.styleReady = false
        onStyleNote(resolved.note)
        if (resolved.kind == BasemapStyleResolver.StyleKind.OfflineProtomaps &&
            resolved.note.isNullOrBlank()
        ) {
            onStyleNote("Offline basemap (Protomaps)")
        }

        // Same basemap URI (e.g. Liberty URL for both OnlineLiberty and Online3d):
        // MapLibre often no-ops setStyle(sameUri), so mutate the live style instead.
        if (sameUri) {
            currentStyleKind.value = resolved.kind
            map.getStyle { style ->
                if (applyGen != styleApplyGen.get()) return@getStyle
                if (style == null) {
                    currentStyleUri.value = null
                    applyResolvedStyle(map, force = true)
                    return@getStyle
                }
                applyTerrainAndPitch(map, style, resolved, applyGen)
            }
            return
        }

        currentStyleUri.value = resolved.styleUri
        currentStyleKind.value = resolved.kind
        map.setStyle(resolved.styleUri) { style ->
            if (applyGen != styleApplyGen.get()) return@setStyle
            if (style == null) {
                NaviMapTestHooks.lastStyleLoadError = "setStyle returned null (${resolved.styleUri})"
                onStyleNote("Basemap load failed; falling back to 2D Liberty")
                if (livePrefer3d) {
                    on3dFailed()
                }
                val fallbackUri = BasemapStyleResolver.LIBERTY_URL
                currentStyleUri.value = fallbackUri
                currentStyleKind.value = BasemapStyleResolver.StyleKind.OnlineLiberty
                map.setStyle(fallbackUri) { fallback ->
                    if (applyGen != styleApplyGen.get()) return@setStyle
                    if (fallback != null) {
                        applyTerrainAndPitch(
                            map,
                            fallback,
                            BasemapStyleResolver.ResolvedStyle(
                                kind = BasemapStyleResolver.StyleKind.OnlineLiberty,
                                styleUri = fallbackUri,
                            ),
                            applyGen,
                        )
                    } else {
                        styleReady.value = true
                        NaviMapTestHooks.styleReady = true
                        NaviMapTestHooks.lastBasemapKind =
                            BasemapStyleResolver.StyleKind.OnlineLiberty.name
                        NaviMapTestHooks.lastCameraPitch = 0.0
                        NaviMapTestHooks.lastTerrainAttached = false
                    }
                }
                return@setStyle
            }
            applyTerrainAndPitch(map, style, resolved, applyGen)
        }
    }

    fun loadTrackBitmap(symbolKey: String): Bitmap? {
        iconCache[symbolKey]?.let { return it }
        return try {
            context.assets.open("icons/aprs/$symbolKey.png").use { input ->
                val raw = BitmapFactory.decodeStream(input) ?: return null
                val padded = padIconOnDisk(raw)
                iconCache[symbolKey] = padded
                padded
            }
        } catch (_: Exception) {
            null
        }
    }

    fun refreshTrackOverlay(map: MapLibreMap) {
        val latest = stateRef.get()
        val marks =
            latest.tracks.map { t ->
                val screen = map.projection.toScreenLocation(LatLng(t.lat, t.lon))
                OverlayMark(screen.x.toFloat(), screen.y.toFloat(), t, loadTrackBitmap(t.symbolKey))
            }
        overlayMarks = marks
        NaviMapTestHooks.lastTrackOverlayCount = marks.size
        NaviMapTestHooks.lastTrackFeatureCount = latest.tracks.size
        NaviMapTestHooks.lastTrackImagesReady = marks.count { it.icon != null }
        NaviMapTestHooks.lastOverlayScreenFingerprint =
            marks
                .map { mark ->
                    "${mark.track.id}:${mark.x.toInt()}:${mark.y.toInt()}"
                }.sorted()
                .joinToString("|")

        // Compose-drawn start/via/end labels (reliable when SymbolLayer glyphs fail).
        val wps = mutableListOf<WaypointOverlayMark>()

        fun addWp(
            name: String,
            lat: Double,
            lon: Double,
        ) {
            if (name.isBlank() || (lat == 0.0 && lon == 0.0)) return
            if (wps.any { it.name.equals(name, ignoreCase = true) }) return
            val screen = map.projection.toScreenLocation(LatLng(lat, lon))
            wps.add(WaypointOverlayMark(screen.x.toFloat(), screen.y.toFloat(), name))
        }
        addWp(latest.startName, latest.startLat, latest.startLon)
        addWp(latest.viaName, latest.viaLat, latest.viaLon)
        for (v in latest.viaPoints) {
            addWp(v.name, v.lat, v.lon)
        }
        addWp(latest.endName, latest.endLat, latest.endLon)
        for (b in latest.breakPois) {
            // Peaks/mountains are never pause labels (e.g. Store Ramshøgda).
            if (b.name.contains("Ramsh", ignoreCase = true)) continue
            addWp(b.name, b.lat, b.lon)
        }
        waypointMarks = wps

        gpsMark =
            if (latest.gpsLat != 0.0 || latest.gpsLon != 0.0) {
                val screen = map.projection.toScreenLocation(LatLng(latest.gpsLat, latest.gpsLon))
                GpsOverlayMark(screen.x.toFloat(), screen.y.toFloat())
            } else {
                null
            }
    }

    DisposableEffect(Unit) {
        mapView.onStart()
        mapView.onResume()
        NaviMapTestHooks.mapPauseHandler = { pause ->
            if (pause) {
                runCatching { mapView.onPause() }
            } else {
                runCatching {
                    mapView.onStart()
                    mapView.onResume()
                }
            }
        }
        NaviMapTestHooks.mapViewTouch = { event -> mapView.dispatchTouchEvent(event) }
        onDispose {
            NaviMapTestHooks.mapPauseHandler = null
            NaviMapTestHooks.mapViewTouch = null
            if (NaviMapTestHooks.deferMapViewDestroy) {
                // Do not pause/stop/destroy: MapLibre 11.8.8 Vulkan can drop the
                // Java MapRenderer while leaving a FinalizerDaemon native peer that
                // SIGSEGVs in AndroidVulkanRendererBackend::~ on this AAOS AVD.
                NaviMapTestHooks.retainedMapViews.add(mapView)
            } else {
                runCatching { mapView.onPause() }
                runCatching { mapView.onStop() }
                runCatching { mapView.onDestroy() }
            }
        }
    }

    LaunchedEffect(unitSystem, styleReady.value) {
        if (!styleReady.value) return@LaunchedEffect
        mapRef?.getStyle { style ->
            if (style != null) {
                BasemapPeakElevationStyle.apply(style, unitSystem)
                BasemapContourLabelStyle.apply(style, unitSystem)
            }
        }
    }

    Box(modifier = modifier) {
        AndroidView(
            factory = { mapView },
            modifier = Modifier.fillMaxSize(),
            update = { view ->
                if (!styleLoadStarted.compareAndSet(false, true)) return@AndroidView
                view.getMapAsync { map ->
                    mapRef = map
                    map.addOnCameraMoveListener {
                        // Keep Compose waypoint/track pins glued to geo while
                        // pan/zoom is in progress (idle-only refresh drifts on screen).
                        refreshTrackOverlay(map)
                    }
                    map.addOnCameraIdleListener {
                        refreshTrackOverlay(map)
                        val pos = map.cameraPosition
                        NaviMapTestHooks.lastCameraZoom = pos.zoom
                        NaviMapTestHooks.lastCameraBearing = pos.bearing
                        NaviMapTestHooks.lastCameraPitch = pos.tilt
                        // Re-assert preferred tilt if a concurrent camera update raced us.
                        val wantTilt = effectiveTiltDeg()
                        if (kotlin.math.abs(pos.tilt - wantTilt) > 1.0) {
                            applyCameraTilt(map, wantTilt)
                        }
                        pos.target?.let { target ->
                            NaviMapTestHooks.lastCameraLat = target.latitude
                            NaviMapTestHooks.lastCameraLon = target.longitude
                            onCameraIdleRef.get().invoke(
                                target.latitude,
                                target.longitude,
                                pos.zoom,
                            )
                            DiagnosticLog.logCamera(
                                zoom = pos.zoom,
                                lat = target.latitude,
                                lon = target.longitude,
                                pitchDeg = pos.tilt,
                                bearingDeg = pos.bearing,
                            )
                        } ?: DiagnosticLog.logCamera(
                            zoom = pos.zoom,
                            pitchDeg = pos.tilt,
                            bearingDeg = pos.bearing,
                        )
                        applyResolvedStyle(map, force = false)
                    }
                    map.addOnMoveListener(
                        object : org.maplibre.android.maps.MapLibreMap.OnMoveListener {
                            override fun onMoveBegin(detector: org.maplibre.android.gestures.MoveGestureDetector) {
                                NaviMapTestHooks.mapGestureMoves += 1
                                onUserPanRef.get().invoke()
                            }

                            override fun onMove(detector: org.maplibre.android.gestures.MoveGestureDetector) {}

                            override fun onMoveEnd(detector: org.maplibre.android.gestures.MoveGestureDetector) {}
                        },
                    )
                    map.addOnScaleListener(
                        object : org.maplibre.android.maps.MapLibreMap.OnScaleListener {
                            override fun onScaleBegin(detector: org.maplibre.android.gestures.StandardScaleGestureDetector) {
                                NaviMapTestHooks.mapGestureScales += 1
                            }

                            override fun onScale(detector: org.maplibre.android.gestures.StandardScaleGestureDetector) {}

                            override fun onScaleEnd(detector: org.maplibre.android.gestures.StandardScaleGestureDetector) {}
                        },
                    )
                    map.addOnRotateListener(
                        object : org.maplibre.android.maps.MapLibreMap.OnRotateListener {
                            override fun onRotateBegin(
                                detector: org.maplibre.android.gestures.RotateGestureDetector,
                            ) {
                                NaviMapTestHooks.mapGestureRotates += 1
                            }

                            override fun onRotate(
                                detector: org.maplibre.android.gestures.RotateGestureDetector,
                            ) {}

                            override fun onRotateEnd(
                                detector: org.maplibre.android.gestures.RotateGestureDetector,
                            ) {
                                onUserRotateRef.get().invoke(map.cameraPosition.bearing)
                            }
                        },
                    )
                    map.uiSettings.setAllGesturesEnabled(true)
                    map.uiSettings.isTiltGesturesEnabled = true
                    runCatching {
                        map.setMinPitchPreference(0.0)
                        map.setMaxPitchPreference(MapHudPrefs.MAPLIBRE_MAX_TILT_DEG)
                    }
                    applyResolvedStyle(map, force = true)
                }
            },
        )

        // Forward touches so the marker overlay Canvas does not block MapLibre pan/pinch.
        // A 4 s stationary hold opens the map-mark menu (cancelled by pan/pinch/short release).
        Canvas(
            modifier =
                Modifier
                    .fillMaxSize()
                    .pointerInteropFilter { event ->
                        val action = event.actionMasked
                        when (action) {
                            android.view.MotionEvent.ACTION_DOWN -> {
                                holdJob?.cancel()
                                holdActive = true
                                holdScreenX = event.x
                                holdScreenY = event.y
                                holdProgress = 0f
                                val sx = event.x
                                val sy = event.y
                                holdJob =
                                    mapScope.launch {
                                        val startMs = android.os.SystemClock.uptimeMillis()
                                        while (isActive) {
                                            val elapsed =
                                                android.os.SystemClock.uptimeMillis() - startMs
                                            holdProgress =
                                                (elapsed.toFloat() / MAP_LONG_PRESS_HOLD_MS.toFloat())
                                                    .coerceIn(0f, 1f)
                                            if (elapsed >= MAP_LONG_PRESS_HOLD_MS) {
                                                val map = mapRef
                                                val ll =
                                                    map
                                                        ?.projection
                                                        ?.fromScreenLocation(
                                                            android.graphics.PointF(sx, sy),
                                                        )
                                                if (ll != null) {
                                                    onMapLongPressRef.get().invoke(
                                                        ll.latitude,
                                                        ll.longitude,
                                                    )
                                                }
                                                val now = android.os.SystemClock.uptimeMillis()
                                                val cancel =
                                                    android.view.MotionEvent.obtain(
                                                        now,
                                                        now,
                                                        android.view.MotionEvent.ACTION_CANCEL,
                                                        sx,
                                                        sy,
                                                        0,
                                                    )
                                                mapView.dispatchTouchEvent(cancel)
                                                cancel.recycle()
                                                holdActive = false
                                                holdProgress = 0f
                                                holdJob = null
                                                break
                                            }
                                            delay(16)
                                        }
                                    }
                                mapView.dispatchTouchEvent(event)
                                true
                            }
                            android.view.MotionEvent.ACTION_POINTER_DOWN,
                            android.view.MotionEvent.ACTION_POINTER_UP,
                            -> {
                                holdJob?.cancel()
                                holdJob = null
                                holdActive = false
                                holdProgress = 0f
                                mapView.dispatchTouchEvent(event)
                                true
                            }
                            android.view.MotionEvent.ACTION_MOVE -> {
                                if (holdActive && holdJob != null) {
                                    val dx = event.x - holdScreenX
                                    val dy = event.y - holdScreenY
                                    val dist = kotlin.math.hypot(dx.toDouble(), dy.toDouble()).toFloat()
                                    if (dist > MAP_LONG_PRESS_MOVE_SLOP_PX) {
                                        holdJob?.cancel()
                                        holdJob = null
                                        holdActive = false
                                        holdProgress = 0f
                                        mapView.dispatchTouchEvent(event)
                                    }
                                    // Within slop: swallow MOVE so MapLibre does not pan mid-hold.
                                    true
                                } else {
                                    mapView.dispatchTouchEvent(event)
                                    true
                                }
                            }
                            android.view.MotionEvent.ACTION_UP,
                            android.view.MotionEvent.ACTION_CANCEL,
                            -> {
                                holdJob?.cancel()
                                holdJob = null
                                holdActive = false
                                holdProgress = 0f
                                mapView.dispatchTouchEvent(event)
                                true
                            }
                            else -> {
                                mapView.dispatchTouchEvent(event)
                                true
                            }
                        }
                    },
        ) {
            if (holdProgress > 0f) {
                val center = Offset(holdScreenX, holdScreenY)
                val radius = 42f
                drawCircle(Color(0x55000000), radius = radius + 8f, center = center)
                drawCircle(
                    Color(0x88FFFFFF),
                    radius = radius,
                    center = center,
                    style = Stroke(width = 6f),
                )
                drawArc(
                    color = Color(0xFF1565C0),
                    startAngle = -90f,
                    sweepAngle = 360f * holdProgress.coerceIn(0f, 1f),
                    useCenter = false,
                    topLeft = Offset(center.x - radius, center.y - radius),
                    size =
                        androidx.compose.ui.geometry
                            .Size(radius * 2f, radius * 2f),
                    style =
                        Stroke(
                            width = 8f,
                            cap = androidx.compose.ui.graphics.StrokeCap.Round,
                        ),
                )
                drawCircle(Color(0xFF1565C0), radius = 8f, center = center)
            }
            val labelPaint =
                AndroidPaint(AndroidPaint.ANTI_ALIAS_FLAG).apply {
                    color = android.graphics.Color.BLACK
                    textSize = 34f
                    style = AndroidPaint.Style.FILL
                }
            val haloPaint =
                AndroidPaint(AndroidPaint.ANTI_ALIAS_FLAG).apply {
                    color = android.graphics.Color.WHITE
                    textSize = 34f
                    style = AndroidPaint.Style.STROKE
                    strokeWidth = 8f
                }
            for (mark in overlayMarks) {
                val center = Offset(mark.x, mark.y)
                drawCircle(Color(0xFFFFEB3B), radius = 34f, center = center)
                drawCircle(Color(0xFF111111), radius = 34f, center = center, style = Stroke(width = 4f))
                val bmp = mark.icon
                if (bmp != null) {
                    val img = bmp.asImageBitmap()
                    val w = 48f
                    val h = 48f
                    drawImage(
                        image = img,
                        srcOffset = androidx.compose.ui.unit.IntOffset.Zero,
                        srcSize =
                            androidx.compose.ui.unit
                                .IntSize(bmp.width, bmp.height),
                        dstOffset =
                            androidx.compose.ui.unit.IntOffset(
                                (mark.x - w / 2f).toInt(),
                                (mark.y - h / 2f).toInt(),
                            ),
                        dstSize =
                            androidx.compose.ui.unit
                                .IntSize(w.toInt(), h.toInt()),
                    )
                }
                drawContext.canvas.nativeCanvas.drawText(
                    mark.track.label,
                    mark.x - 40f,
                    mark.y + 58f,
                    haloPaint,
                )
                drawContext.canvas.nativeCanvas.drawText(
                    mark.track.label,
                    mark.x - 40f,
                    mark.y + 58f,
                    labelPaint,
                )
            }
            val wpHalo =
                AndroidPaint(AndroidPaint.ANTI_ALIAS_FLAG).apply {
                    color = android.graphics.Color.WHITE
                    textSize = 36f
                    style = AndroidPaint.Style.STROKE
                    strokeWidth = 10f
                }
            val wpPaint =
                AndroidPaint(AndroidPaint.ANTI_ALIAS_FLAG).apply {
                    color = android.graphics.Color.parseColor("#111111")
                    textSize = 36f
                    style = AndroidPaint.Style.FILL
                    isFakeBoldText = true
                }
            for (wp in waypointMarks) {
                drawCircle(Color(0xFFC62828), radius = 10f, center = Offset(wp.x, wp.y))
                drawCircle(
                    Color.White,
                    radius = 10f,
                    center = Offset(wp.x, wp.y),
                    style = Stroke(width = 3f),
                )
                val textX = wp.x - wpPaint.measureText(wp.name) / 2f
                // Keep labels on-screen: near top edge, draw below the pin.
                val textY = if (wp.y < 90f) wp.y + 42f else wp.y - 22f
                drawContext.canvas.nativeCanvas.drawText(wp.name, textX, textY, wpHalo)
                drawContext.canvas.nativeCanvas.drawText(wp.name, textX, textY, wpPaint)
            }
            gpsMark?.let { g ->
                val c = Offset(g.x, g.y)
                drawCircle(Color(0x334285F4), radius = 36f, center = c)
                drawCircle(Color(0xFF1A73E8), radius = 14f, center = c)
                drawCircle(Color.White, radius = 14f, center = c, style = Stroke(width = 4f))
                drawCircle(Color.White, radius = 5f, center = c)
            }
        }
    }

    LaunchedEffect(styleEpoch, prefer3d, contoursEnabled, vulkanAvailable, cameraTiltDeg) {
        val map = mapRef ?: return@LaunchedEffect
        applyResolvedStyle(map, force = true)
    }

    LaunchedEffect(state.cameraLat, state.cameraLon, prefer3d, contoursEnabled, cameraTiltDeg) {
        val map = mapRef ?: return@LaunchedEffect
        if (state.cameraLat != null && state.cameraLon != null) {
            applyResolvedStyle(map, force = false)
        }
    }

    LaunchedEffect(state.layerEpoch, styleReady.value, prefer3d, contoursEnabled, vulkanAvailable, cameraTiltDeg) {
        if (!styleReady.value) return@LaunchedEffect
        val want3d = prefer3dRef.get() && vulkanRef.get()
        val wantContours = contoursEnabledRef.get()
        val pitch = effectiveTiltDeg()
        mapView.getMapAsync { map ->
            mapRef = map
            applyCameraTilt(map, pitch)
            map.getStyle { style ->
                val latest = stateRef.get()
                if (!want3d) {
                    MapterhornTerrain.detach(style)
                    NaviMapTestHooks.lastTerrainAttached = false
                }
                if (!wantContours) {
                    MapterhornContours.detach(style)
                    NaviMapTestHooks.lastContoursAttached = false
                }
                applyRouteToStyle(style, latest)
                applyTracksToStyle(style, latest.tracks, mapView.context)
                ensureRouteAboveHillshade(style)
                map.triggerRepaint()
                onLayerCount(style.layers.size)
                refreshTrackOverlay(map)
                if (latest.cameraLat != null && latest.cameraLon != null) {
                    val pos =
                        org.maplibre.android.camera.CameraPosition
                            .Builder()
                            .target(LatLng(latest.cameraLat, latest.cameraLon))
                            .zoom(latest.cameraZoom ?: 12.0)
                            .bearing(latest.cameraBearing)
                            .tilt(pitch)
                            .build()
                    map.moveCamera(CameraUpdateFactory.newCameraPosition(pos))
                } else if (latest.polyline.isNotBlank()) {
                    val pts = parsePolyline(latest.polyline)
                    if (pts.size >= 2) {
                        val bounds =
                            LatLngBounds
                                .Builder()
                                .apply {
                                    pts.forEach { include(it) }
                                    if (latest.poiLat != 0.0 || latest.poiLon != 0.0) {
                                        include(LatLng(latest.poiLat, latest.poiLon))
                                    }
                                }.build()
                        // Fit entire route; keep user tilt so hillshade/perspective stay visible.
                        map.animateCamera(CameraUpdateFactory.newLatLngBounds(bounds, 120))
                        map.moveCamera(
                            CameraUpdateFactory.newCameraPosition(
                                org.maplibre.android.camera.CameraPosition
                                    .Builder(map.cameraPosition)
                                    .tilt(pitch)
                                    .build(),
                            ),
                        )
                    }
                } else {
                    // No explicit camera / polyline: only snap to GPS while follow mode
                    // is on. After a manual pan, leave the MapLibre camera alone.
                    if (!latest.followGps) {
                        NaviMapTestHooks.lastCameraPitch = map.cameraPosition.tilt
                        refreshTrackOverlay(map)
                        NaviMapTestHooks.tracksAppliedEpoch = NaviMapTestHooks.tracksEpoch
                        return@getStyle
                    }
                    val fallbackLat =
                        when {
                            latest.gpsLat != 0.0 || latest.gpsLon != 0.0 -> latest.gpsLat
                            else -> 61.2
                        }
                    val fallbackLon =
                        when {
                            latest.gpsLat != 0.0 || latest.gpsLon != 0.0 -> latest.gpsLon
                            else -> 10.7
                        }
                    val pos =
                        org.maplibre.android.camera.CameraPosition
                            .Builder()
                            .target(LatLng(fallbackLat, fallbackLon))
                            .zoom(latest.cameraZoom ?: 6.5)
                            .bearing(latest.cameraBearing)
                            .tilt(pitch)
                            .build()
                    map.moveCamera(CameraUpdateFactory.newCameraPosition(pos))
                }
                NaviMapTestHooks.lastCameraPitch = map.cameraPosition.tilt
                refreshTrackOverlay(map)
                NaviMapTestHooks.tracksAppliedEpoch = NaviMapTestHooks.tracksEpoch
            }
        }
    }

    // Bearing-only updates must not re-apply Compose zoom (that undoes user pinch / double-tap).
    LaunchedEffect(state.cameraBearing, bearingEpoch, styleReady.value) {
        if (!styleReady.value) return@LaunchedEffect
        if (!NaviMapTestHooks.applyBearingToMap) {
            NaviMapTestHooks.lastCameraBearing = state.cameraBearing
            return@LaunchedEffect
        }
        val bearing = stateRef.get().cameraBearing
        mapView.getMapAsync { map ->
            try {
                val pitch = effectiveTiltDeg()
                map.moveCamera(
                    CameraUpdateFactory.newCameraPosition(
                        org.maplibre.android.camera.CameraPosition
                            .Builder(map.cameraPosition)
                            .bearing(bearing)
                            // Pin tilt: async bearing updates otherwise race a just-applied
                            // pitch and freeze the camera at 0° (prefs correct, view flat).
                            .tilt(pitch)
                            .build(),
                    ),
                )
                NaviMapTestHooks.lastCameraBearing = bearing
                NaviMapTestHooks.lastCameraPitch = map.cameraPosition.tilt
            } catch (e: Exception) {
                android.util.Log.e("HudVerification", "bearing update failed", e)
            }
        }
    }

    // Zoom updates from HUD / pendingCamera. Retarget to live GPS only while
    // follow mode is on; after a manual pan, change zoom and keep the camera center.
    LaunchedEffect(state.cameraZoom, styleReady.value) {
        if (!styleReady.value) return@LaunchedEffect
        val latest = stateRef.get()
        val zoom = latest.cameraZoom ?: return@LaunchedEffect
        mapView.getMapAsync { map ->
            try {
                val pitch = effectiveTiltDeg()
                val builder =
                    org.maplibre.android.camera.CameraPosition
                        .Builder(map.cameraPosition)
                        .zoom(zoom)
                        .tilt(pitch)
                when {
                    !NaviMapTestHooks.disableGpsFollow &&
                        latest.followGps &&
                        (latest.gpsLat != 0.0 || latest.gpsLon != 0.0) -> {
                        builder.target(LatLng(latest.gpsLat, latest.gpsLon))
                    }
                    latest.cameraLat != null && latest.cameraLon != null -> {
                        builder.target(LatLng(latest.cameraLat, latest.cameraLon))
                    }
                    // else: keep MapLibre's current target (user-panned view)
                }
                map.moveCamera(CameraUpdateFactory.newCameraPosition(builder.build()))
                NaviMapTestHooks.lastCameraZoom = zoom
                NaviMapTestHooks.lastCameraPitch = map.cameraPosition.tilt
            } catch (e: Exception) {
                android.util.Log.e("HudVerification", "zoom update failed", e)
            }
        }
    }

    // Instrumented tests: wait for MapLibre fully-rendered + idle before screencap.
    // styleReady alone is not enough — hydro fill/line AA can still be mid-composite
    // while roads already look sharp (see map-styles.md hydro fringe note).
    LaunchedEffect(Unit) {
        while (true) {
            kotlinx.coroutines.delay(100)
            val req = NaviMapTestHooks.renderSettleRequestId
            if (req <= NaviMapTestHooks.lastRenderSettleId || !styleReady.value) continue

            var done = false
            var gotFully = false
            var gotIdle = false
            lateinit var idleListener: MapView.OnDidBecomeIdleListener
            lateinit var frameListener: MapView.OnDidFinishRenderingFrameListener

            fun maybeComplete() {
                if (done || !gotFully || !gotIdle) return
                done = true
                mapView.removeOnDidBecomeIdleListener(idleListener)
                mapView.removeOnDidFinishRenderingFrameListener(frameListener)
                NaviMapTestHooks.lastRenderSettleId = req
            }

            idleListener =
                object : MapView.OnDidBecomeIdleListener {
                    override fun onDidBecomeIdle() {
                        gotIdle = true
                        maybeComplete()
                    }
                }
            frameListener =
                object : MapView.OnDidFinishRenderingFrameListener {
                    override fun onDidFinishRenderingFrame(
                        fully: Boolean,
                        framingTime: Double,
                        renderingTime: Double,
                    ) {
                        if (!fully) return
                        gotFully = true
                        maybeComplete()
                    }
                }
            mapView.addOnDidBecomeIdleListener(idleListener)
            mapView.addOnDidFinishRenderingFrameListener(frameListener)
            mapView.getMapAsync { map -> map.triggerRepaint() }

            val waitUntil = System.currentTimeMillis() + 12_000
            while (
                NaviMapTestHooks.lastRenderSettleId < req &&
                System.currentTimeMillis() < waitUntil
            ) {
                kotlinx.coroutines.delay(50)
            }
            if (NaviMapTestHooks.lastRenderSettleId < req) {
                mapView.removeOnDidBecomeIdleListener(idleListener)
                mapView.removeOnDidFinishRenderingFrameListener(frameListener)
                // Timeout: still publish so tests do not hang forever.
                NaviMapTestHooks.lastRenderSettleId = req
            }
        }
    }

    // Map framebuffer capture for instrumented tests.
    // map.snapshot() often returns a tiny blank PNG on this emulator; PixelCopy
    // of the MapView surface reliably includes basemap + track symbol/circle layers.
    // Wait for idle (or enough frames) so we are not copying a black first buffer.
    LaunchedEffect(Unit) {
        while (true) {
            kotlinx.coroutines.delay(200)
            val req = NaviMapTestHooks.snapshotRequestId
            if (req <= NaviMapTestHooks.lastSnapshotId || !styleReady.value) continue

            fun publishPng(bmp: android.graphics.Bitmap) {
                val out = java.io.ByteArrayOutputStream()
                bmp.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, out)
                NaviMapTestHooks.lastSnapshotPng = out.toByteArray()
                NaviMapTestHooks.lastSnapshotId = req
            }

            fun findSurfaceView(v: android.view.View): android.view.SurfaceView? {
                if (v is android.view.SurfaceView) return v
                if (v is android.view.ViewGroup) {
                    for (i in 0 until v.childCount) {
                        findSurfaceView(v.getChildAt(i))?.let { return it }
                    }
                }
                return null
            }

            fun captureView() {
                val w = mapView.width
                val h = mapView.height
                if (w <= 0 || h <= 0) {
                    mapView.getMapAsync { map ->
                        map.snapshot { bmp -> publishPng(bmp) }
                    }
                    return
                }
                val bmp =
                    android.graphics.Bitmap.createBitmap(
                        w,
                        h,
                        android.graphics.Bitmap.Config.ARGB_8888,
                    )
                val handler = android.os.Handler(android.os.Looper.getMainLooper())
                val surface = findSurfaceView(mapView)
                val window = (mapView.context as? android.app.Activity)?.window
                val listener =
                    android.view.PixelCopy.OnPixelCopyFinishedListener { result ->
                        if (result == android.view.PixelCopy.SUCCESS) {
                            publishPng(bmp)
                        } else {
                            mapView.getMapAsync { map ->
                                map.snapshot { snap -> publishPng(snap) }
                            }
                        }
                    }
                try {
                    when {
                        window != null ->
                            android.view.PixelCopy.request(window, bmp, listener, handler)
                        surface != null ->
                            android.view.PixelCopy.request(surface, bmp, listener, handler)
                        else ->
                            mapView.getMapAsync { map ->
                                map.snapshot { snap -> publishPng(snap) }
                            }
                    }
                } catch (_: Exception) {
                    mapView.getMapAsync { map ->
                        map.snapshot { snap -> publishPng(snap) }
                    }
                }
            }

            var done = false
            val idleListener =
                object : MapView.OnDidBecomeIdleListener {
                    override fun onDidBecomeIdle() {
                        if (done) return
                        done = true
                        mapView.removeOnDidBecomeIdleListener(this)
                        captureView()
                    }
                }
            val frameListener =
                object : MapView.OnDidFinishRenderingFrameListener {
                    private var frames = 0

                    override fun onDidFinishRenderingFrame(
                        fully: Boolean,
                        framingTime: Double,
                        renderingTime: Double,
                    ) {
                        frames++
                        if (!fully && frames < 8) return
                        if (done) return
                        done = true
                        mapView.removeOnDidFinishRenderingFrameListener(this)
                        mapView.removeOnDidBecomeIdleListener(idleListener)
                        captureView()
                    }
                }
            mapView.addOnDidBecomeIdleListener(idleListener)
            mapView.addOnDidFinishRenderingFrameListener(frameListener)
            mapView.getMapAsync { map -> map.triggerRepaint() }

            val waitUntil = System.currentTimeMillis() + 8_000
            while (
                NaviMapTestHooks.lastSnapshotId < req &&
                System.currentTimeMillis() < waitUntil
            ) {
                kotlinx.coroutines.delay(100)
            }
            if (NaviMapTestHooks.lastSnapshotId < req) {
                mapView.removeOnDidBecomeIdleListener(idleListener)
                mapView.removeOnDidFinishRenderingFrameListener(frameListener)
                captureView()
                val fallbackUntil = System.currentTimeMillis() + 3_000
                while (
                    NaviMapTestHooks.lastSnapshotId < req &&
                    System.currentTimeMillis() < fallbackUntil
                ) {
                    kotlinx.coroutines.delay(100)
                }
            }
        }
    }
}

private fun ensureRouteAboveHillshade(style: Style) {
    // Hills now sit below water; overlays must stay on top of the full basemap
    // stack (land + hills + water + roads), not merely above navi-hills.
    for (id in listOf(
        "route-line",
        "route-off-trail-line",
        "waypoints-dots",
        "waypoints-layer",
        "gps-accuracy",
        "gps-dot",
    )) {
        val layer = style.getLayer(id) ?: continue
        val moved =
            runCatching {
                style.removeLayer(layer)
                style.addLayer(layer)
                true
            }.getOrDefault(false)
        if (!moved && style.getLayer(id) == null) {
            runCatching { style.addLayer(layer) }
        }
    }
}

private fun applyRouteToStyle(
    style: Style,
    state: MapRouteState,
) {
    val (onTrailPoly, offTrailPoly) = splitRouteSegmentPolylines(state.routeSegmentsJson)
    val onTrailPts =
        when {
            onTrailPoly.isNotBlank() -> parsePolyline(onTrailPoly)
            state.polyline.isNotBlank() && offTrailPoly.isBlank() -> parsePolyline(state.polyline)
            state.polyline.isNotBlank() && onTrailPoly.isBlank() && offTrailPoly.isNotBlank() ->
                emptyList()
            else -> parsePolyline(state.polyline)
        }
    val offTrailPts = if (offTrailPoly.isNotBlank()) parsePolyline(offTrailPoly) else emptyList()

    fun upsertLine(
        sourceId: String,
        layerId: String,
        pts: List<LatLng>,
        color: String,
        dashed: Boolean,
    ) {
        if (pts.size < 2) {
            if (style.getLayer(layerId) != null) style.removeLayer(layerId)
            if (style.getSource(sourceId) != null) style.removeSource(sourceId)
            return
        }
        val line = LineString.fromLngLats(pts.map { Point.fromLngLat(it.longitude, it.latitude) })
        if (style.getSource(sourceId) == null) {
            style.addSource(GeoJsonSource(sourceId, line))
        } else {
            (style.getSource(sourceId) as? GeoJsonSource)?.setGeoJson(line)
        }
        if (style.getLayer(layerId) == null) {
            val layer = LineLayer(layerId, sourceId)
            if (dashed) {
                layer.withProperties(
                    PropertyFactory.lineColor(color),
                    PropertyFactory.lineWidth(6f),
                    PropertyFactory.lineCap("round"),
                    PropertyFactory.lineJoin("round"),
                    PropertyFactory.lineDasharray(arrayOf(2f, 2f)),
                )
            } else {
                layer.withProperties(
                    PropertyFactory.lineColor(color),
                    PropertyFactory.lineWidth(6f),
                    PropertyFactory.lineCap("round"),
                    PropertyFactory.lineJoin("round"),
                )
            }
            style.addLayer(layer)
        }
    }

    if (onTrailPts.size >= 2 || offTrailPts.size >= 2) {
        upsertLine("route-src", "route-line", onTrailPts, "#C62828", dashed = false)
        upsertLine("route-off-trail-src", "route-off-trail-line", offTrailPts, "#6D4C41", dashed = true)
    } else if (state.polyline.isNotBlank()) {
        upsertLine("route-src", "route-line", parsePolyline(state.polyline), "#C62828", dashed = false)
        upsertLine("route-off-trail-src", "route-off-trail-line", emptyList(), "#6D4C41", dashed = true)
    } else {
        upsertLine("route-src", "route-line", emptyList(), "#C62828", dashed = false)
        upsertLine("route-off-trail-src", "route-off-trail-line", emptyList(), "#6D4C41", dashed = true)
    }

    if (state.poiLat != 0.0 || state.poiLon != 0.0) {
        val feature = Feature.fromGeometry(Point.fromLngLat(state.poiLon, state.poiLat))
        feature.addStringProperty("name", state.poiName)
        if (style.getImage("poi-icon") == null && state.poiIconPng.isNotEmpty()) {
            val bmp = BitmapFactory.decodeByteArray(state.poiIconPng, 0, state.poiIconPng.size)
            if (bmp != null) {
                style.addImage("poi-icon", bmp)
            }
        }
        if (style.getSource("poi-src") == null) {
            style.addSource(GeoJsonSource("poi-src", FeatureCollection.fromFeature(feature)))
            style.addLayer(
                SymbolLayer("poi-layer", "poi-src").withProperties(
                    PropertyFactory.iconImage("poi-icon"),
                    PropertyFactory.iconSize(1.0f),
                    PropertyFactory.iconAllowOverlap(true),
                    PropertyFactory.textField("{name}"),
                    PropertyFactory.textOffset(arrayOf(0f, 1.2f)),
                    PropertyFactory.textSize(12f),
                    PropertyFactory.textHaloColor("#FFFFFF"),
                    PropertyFactory.textHaloWidth(1.5f),
                    PropertyFactory.textAllowOverlap(true),
                ),
            )
        } else {
            (style.getSource("poi-src") as? GeoJsonSource)
                ?.setGeoJson(FeatureCollection.fromFeature(feature))
        }
    } else {
        if (style.getLayer("poi-layer") != null) style.removeLayer("poi-layer")
        if (style.getSource("poi-src") != null) style.removeSource("poi-src")
    }

    // Start / via / end place names — always drawn when coords are set so labels
    // remain visible even when search chrome is hidden for screenshots.
    val waypointFeatures = mutableListOf<Feature>()

    fun addWaypoint(
        name: String,
        lat: Double,
        lon: Double,
        kind: String,
    ) {
        if (name.isBlank() || (lat == 0.0 && lon == 0.0)) return
        val f = Feature.fromGeometry(Point.fromLngLat(lon, lat))
        f.addStringProperty("name", name)
        f.addStringProperty("kind", kind)
        waypointFeatures.add(f)
    }
    addWaypoint(state.startName, state.startLat, state.startLon, "start")
    if (state.viaPoints.isNotEmpty()) {
        for (v in state.viaPoints) {
            addWaypoint(v.name, v.lat, v.lon, "via")
        }
    } else {
        addWaypoint(state.viaName, state.viaLat, state.viaLon, "via")
    }
    addWaypoint(state.endName, state.endLat, state.endLon, "end")
    if (waypointFeatures.isNotEmpty()) {
        val collection = FeatureCollection.fromFeatures(waypointFeatures)
        if (style.getSource("waypoints-src") == null) {
            style.addSource(GeoJsonSource("waypoints-src", collection))
            style.addLayer(
                SymbolLayer("waypoints-layer", "waypoints-src").withProperties(
                    PropertyFactory.textField("{name}"),
                    PropertyFactory.textSize(14f),
                    PropertyFactory.textColor("#111111"),
                    PropertyFactory.textHaloColor("#FFFFFF"),
                    PropertyFactory.textHaloWidth(2f),
                    PropertyFactory.textOffset(arrayOf(0f, -1.4f)),
                    PropertyFactory.textAnchor("bottom"),
                    PropertyFactory.textAllowOverlap(true),
                    PropertyFactory.textIgnorePlacement(true),
                ),
            )
            // Circle under the label for visibility without custom icons.
            style.addLayerBelow(
                org.maplibre.android.style.layers
                    .CircleLayer("waypoints-dots", "waypoints-src")
                    .withProperties(
                        PropertyFactory.circleRadius(6f),
                        PropertyFactory.circleColor("#C62828"),
                        PropertyFactory.circleStrokeColor("#FFFFFF"),
                        PropertyFactory.circleStrokeWidth(2f),
                    ),
                "waypoints-layer",
            )
        } else {
            (style.getSource("waypoints-src") as? GeoJsonSource)?.setGeoJson(collection)
        }
    } else {
        if (style.getLayer("waypoints-layer") != null) style.removeLayer("waypoints-layer")
        if (style.getLayer("waypoints-dots") != null) style.removeLayer("waypoints-dots")
        if (style.getSource("waypoints-src") != null) style.removeSource("waypoints-src")
    }

    // Current GPS / device position (dot only; no text label).
    if (state.gpsLat != 0.0 || state.gpsLon != 0.0) {
        val gpsFeature = Feature.fromGeometry(Point.fromLngLat(state.gpsLon, state.gpsLat))
        val gpsCollection = FeatureCollection.fromFeature(gpsFeature)
        if (style.getSource("gps-src") == null) {
            style.addSource(GeoJsonSource("gps-src", gpsCollection))
            style.addLayer(
                org.maplibre.android.style.layers
                    .CircleLayer("gps-accuracy", "gps-src")
                    .withProperties(
                        PropertyFactory.circleRadius(18f),
                        PropertyFactory.circleColor("#4285F4"),
                        PropertyFactory.circleOpacity(0.22f),
                        PropertyFactory.circleStrokeWidth(0f),
                    ),
            )
            style.addLayer(
                org.maplibre.android.style.layers
                    .CircleLayer("gps-dot", "gps-src")
                    .withProperties(
                        PropertyFactory.circleRadius(8f),
                        PropertyFactory.circleColor("#1A73E8"),
                        PropertyFactory.circleStrokeColor("#FFFFFF"),
                        PropertyFactory.circleStrokeWidth(2.5f),
                    ),
            )
        } else {
            (style.getSource("gps-src") as? GeoJsonSource)?.setGeoJson(gpsCollection)
        }
        if (style.getLayer("gps-label") != null) style.removeLayer("gps-label")
    } else {
        if (style.getLayer("gps-label") != null) style.removeLayer("gps-label")
        if (style.getLayer("gps-dot") != null) style.removeLayer("gps-dot")
        if (style.getLayer("gps-accuracy") != null) style.removeLayer("gps-accuracy")
        if (style.getSource("gps-src") != null) style.removeSource("gps-src")
    }
    ensureRouteAboveHillshade(style)
}

private fun applyTracksToStyle(
    style: Style,
    tracks: List<TrackMarker>,
    context: android.content.Context,
) {
    val features =
        tracks.map { t ->
            val f = Feature.fromGeometry(Point.fromLngLat(t.lon, t.lat))
            f.addStringProperty("id", t.id)
            f.addStringProperty("label", t.label)
            f.addStringProperty("icon", "track-${t.symbolKey}")
            f
        }
    val collection = FeatureCollection.fromFeatures(features)
    NaviMapTestHooks.lastTrackFeatureCount = tracks.size

    var imagesReady = 0
    for (t in tracks) {
        val imageId = "track-${t.symbolKey}"
        if (style.getImage(imageId) == null) {
            try {
                context.assets.open("icons/aprs/${t.symbolKey}.png").use { input ->
                    val bytes = input.readBytes()
                    val raw = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
                    if (raw != null) {
                        style.addImage(imageId, padIconOnDisk(raw))
                        imagesReady++
                    }
                }
            } catch (e: Exception) {
                android.util.Log.w("NaviTracks", "icon load failed for $imageId: $e")
            }
        } else {
            imagesReady++
        }
    }
    NaviMapTestHooks.lastTrackImagesReady = imagesReady

    // Rebuild source+layers whenever we have tracks. Updating an empty GeoJsonSource
    // in place left features in the style with no visible paint on this MapLibre/AA build.
    if (tracks.isNotEmpty()) {
        try {
            style.getLayer("tracks-layer")?.let { style.removeLayer(it) }
        } catch (_: Exception) {
        }
        try {
            style.getLayer("tracks-halo")?.let { style.removeLayer(it) }
        } catch (_: Exception) {
        }
        try {
            style.getSource("tracks-src")?.let { style.removeSource(it) }
        } catch (_: Exception) {
        }

        style.addSource(GeoJsonSource("tracks-src", collection))
        val halo =
            CircleLayer("tracks-halo", "tracks-src").withProperties(
                PropertyFactory.circleRadius(26f),
                PropertyFactory.circleColor("#FFEB3B"),
                PropertyFactory.circleOpacity(1f),
                PropertyFactory.circleStrokeWidth(3f),
                PropertyFactory.circleStrokeColor("#000000"),
            )
        val symbols =
            SymbolLayer("tracks-layer", "tracks-src").withProperties(
                PropertyFactory.iconImage(Expression.get("icon")),
                PropertyFactory.iconSize(1.8f),
                PropertyFactory.iconAllowOverlap(true),
                PropertyFactory.iconIgnorePlacement(true),
                PropertyFactory.iconOptional(true),
                PropertyFactory.textField(Expression.get("label")),
                PropertyFactory.textSize(14f),
                PropertyFactory.textOffset(arrayOf(0f, 2.0f)),
                PropertyFactory.textColor("#000000"),
                PropertyFactory.textHaloColor("#FFFFFF"),
                PropertyFactory.textHaloWidth(2f),
                PropertyFactory.textAllowOverlap(true),
                PropertyFactory.textIgnorePlacement(true),
            )
        // Append at end of the stack (above basemap + annotation helpers).
        style.addLayer(halo)
        style.addLayer(symbols)
        val preview = tracks.take(2).joinToString { "${it.id}@${it.lat},${it.lon}" }
        android.util.Log.i(
            "NaviTracks",
            "tracks rebuilt n=${tracks.size} images=$imagesReady sample=[$preview] " +
                "geojsonChars=${collection.toJson().length}",
        )
        return
    }

    // Empty snapshot: clear features but keep layers if present.
    if (style.getSource("tracks-src") == null) {
        style.addSource(GeoJsonSource("tracks-src", collection))
        android.util.Log.i("NaviTracks", "tracks empty source created")
    } else {
        (style.getSource("tracks-src") as? GeoJsonSource)?.setGeoJson(collection)
        android.util.Log.i("NaviTracks", "tracks cleared (n=0)")
    }
}

private fun padIconOnDisk(src: android.graphics.Bitmap): android.graphics.Bitmap {
    val size = maxOf(src.width, src.height) + 12
    val out = android.graphics.Bitmap.createBitmap(size, size, android.graphics.Bitmap.Config.ARGB_8888)
    val canvas = android.graphics.Canvas(out)
    val paint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG)
    paint.color = android.graphics.Color.WHITE
    canvas.drawCircle(size / 2f, size / 2f, size / 2f - 1f, paint)
    paint.color = android.graphics.Color.BLACK
    paint.style = android.graphics.Paint.Style.STROKE
    paint.strokeWidth = 2f
    canvas.drawCircle(size / 2f, size / 2f, size / 2f - 2f, paint)
    val left = (size - src.width) / 2f
    val top = (size - src.height) / 2f
    canvas.drawBitmap(src, left, top, null)
    return out
}

private fun parseBreakPoisJson(raw: String): List<BreakPoiMark> {
    if (raw.isBlank() || raw == "[]") return emptyList()
    return try {
        val arr = org.json.JSONArray(raw)
        buildList {
            for (i in 0 until arr.length()) {
                val o = arr.optJSONObject(i) ?: continue
                val name = o.optString("name").trim()
                val lat = o.optDouble("lat", Double.NaN)
                val lon = o.optDouble("lon", Double.NaN)
                if (name.isEmpty() || lat.isNaN() || lon.isNaN()) continue
                add(
                    BreakPoiMark(
                        name = name,
                        lat = lat,
                        lon = lon,
                        kind = o.optString("kind", "hut"),
                    ),
                )
            }
        }
    } catch (_: Exception) {
        emptyList()
    }
}

/** Split `route_segments_json` into concatenated on-trail / off-trail polylines. */
private fun splitRouteSegmentPolylines(raw: String): Pair<String, String> {
    if (raw.isBlank() || raw == "[]") return "" to ""
    return try {
        val arr = org.json.JSONArray(raw)
        val on = StringBuilder()
        val off = StringBuilder()
        for (i in 0 until arr.length()) {
            val o = arr.optJSONObject(i) ?: continue
            val kind = o.optString("kind")
            val poly = o.optString("polyline").trim()
            if (poly.isEmpty()) continue
            val dest =
                when (kind) {
                    "off_trail" -> off
                    else -> on
                }
            if (dest.isNotEmpty()) dest.append(';')
            dest.append(poly)
        }
        on.toString() to off.toString()
    } catch (_: Exception) {
        "" to ""
    }
}

private fun parsePolyline(encoded: String): List<LatLng> {
    return encoded.split(';').mapNotNull { part ->
        val bits = part.split(',')
        if (bits.size != 2) return@mapNotNull null
        val lon = bits[0].toDoubleOrNull() ?: return@mapNotNull null
        val lat = bits[1].toDoubleOrNull() ?: return@mapNotNull null
        LatLng(lat, lon)
    }
}

private fun ensureIconsCopied(
    context: android.content.Context,
    dest: File,
) {
    dest.mkdirs()
    try {
        copyAssetDir(context.assets, "icons", dest)
    } catch (_: Exception) {
    }
    // Always refresh leaf.svg so eco HUD never falls back to a missing-icon path
    // after a partial/old iconsDir (e.g. early return when aprs/ was listed first).
    runCatching {
        context.assets.open("icons/leaf.svg").use { input ->
            File(dest, "leaf.svg").outputStream().use { output -> input.copyTo(output) }
        }
    }
    // Same for the custom speed-camera mark (lean-pack inclusion / unknown.svg bug).
    runCatching {
        context.assets.open("icons/speed_camera.svg").use { input ->
            File(dest, "speed_camera.svg").outputStream().use { output -> input.copyTo(output) }
        }
    }
    // Look-forward 20 km/h plate (Navi stand-in; upstream catalogue has svg=null).
    runCatching {
        val roadSigns = File(dest, "road-signs")
        roadSigns.mkdirs()
        context.assets.open("icons/road-signs/no_sign_362_20.svg").use { input ->
            File(roadSigns, "no_sign_362_20.svg").outputStream().use { output ->
                input.copyTo(output)
            }
        }
    }
}

private fun copyAssetDir(
    am: android.content.res.AssetManager,
    assetPath: String,
    destDir: File,
) {
    val names = am.list(assetPath) ?: return
    destDir.mkdirs()
    for (name in names) {
        val childAsset = "$assetPath/$name"
        val children = am.list(childAsset)
        if (children != null && children.isNotEmpty()) {
            copyAssetDir(am, childAsset, File(destDir, name))
        } else {
            runCatching {
                am.open(childAsset).use { input ->
                    File(destDir, name).outputStream().use { output -> input.copyTo(output) }
                }
            }
        }
    }
}
