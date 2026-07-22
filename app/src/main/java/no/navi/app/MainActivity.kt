package no.navi.app

import android.Manifest
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Paint as AndroidPaint
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableIntStateOf
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
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.zIndex
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
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
import uniffi.navi.FfiIconTheme
import uniffi.navi.FfiSavedRoute
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.PlaceHit
import uniffi.navi.TravelProfile
import uniffi.navi.applyOsmUpdate
import uniffi.navi.bindGeofabrikRegion
import uniffi.navi.checkOsmUpdates
import uniffi.navi.deleteSavedRoute
import uniffi.navi.detectedParallelism
import uniffi.navi.ecoModeDefault
import uniffi.navi.ecoModeToggleable
import uniffi.navi.ensurePlaceIndex
import uniffi.navi.formatAvoidMajorReport
import uniffi.navi.lastGpsFix
import uniffi.navi.listSavedRoutes
import uniffi.navi.loadCarRestSettings
import uniffi.navi.loadVehicleLimits
import uniffi.navi.osmWeeklyReminderDue
import uniffi.navi.provisionRegionData
import uniffi.navi.rasterizeIconPng
import uniffi.navi.runCarCorridorPipeline
import uniffi.navi.saveNamedRoute
import uniffi.navi.saveVehicleLimits
import uniffi.navi.searchPlaces
import uniffi.navi.setOsmWeeklyReminder
import uniffi.navi.travelProfileMenuFocus
import android.os.Handler
import android.os.Looper
import java.io.File

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        MapLibre.getInstance(this)
        setContent {
            MaterialTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    NaviMapScreen()
                }
            }
        }
    }
}

data class MapRouteState(
    val polyline: String = "",
    val poiLat: Double = 0.0,
    val poiLon: Double = 0.0,
    val poiName: String = "",
    val poiIconPng: ByteArray = ByteArray(0),
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
)

private enum class SearchTarget { To, Via }
private enum class SearchMode { Place, Address }

@Composable
private fun NaviMapScreen() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var status by remember { mutableStateOf("Ready. Cores: ${detectedParallelism()}") }
    var mapState by remember { mutableStateOf(MapRouteState()) }
    var mapLayerCount by remember { mutableIntStateOf(0) }
    var profile by remember { mutableStateOf(TravelProfile.CAR) }
    var ecoEnabled by remember { mutableStateOf(ecoModeDefault(TravelProfile.CAR)) }
    var searchMode by remember { mutableStateOf(SearchMode.Place) }
    var searchTarget by remember { mutableStateOf(SearchTarget.To) }
    var query by remember { mutableStateOf("") }
    var hits by remember { mutableStateOf<List<PlaceHit>>(emptyList()) }
    var searchBusy by remember { mutableStateOf(false) }
    var showTools by remember { mutableStateOf(false) }
    var searchJob by remember { mutableStateOf<Job?>(null) }
    var toPoint by remember { mutableStateOf(Waypoint()) }
    var viaPoint by remember { mutableStateOf<Waypoint?>(null) }
    var fromPoint by remember { mutableStateOf<Waypoint?>(null) }
    var avoidMajor by remember { mutableStateOf(false) }
    var prioritySharePct by remember { mutableDoubleStateOf(0.0) }
    var savedRoutes by remember { mutableStateOf<List<FfiSavedRoute>>(emptyList()) }
    var axleKg by remember { mutableStateOf("") }
    var heightM by remember { mutableStateOf("") }
    var widthM by remember { mutableStateOf("") }
    var hideChrome by remember { mutableStateOf(false) }
    var hideSearch by remember { mutableStateOf(false) }
    var weeklyReminder by remember { mutableStateOf(false) }
    var pendingUpdatePlan by remember { mutableStateOf<String?>(null) }
    var updateReminderDue by remember { mutableStateOf(false) }
    var driveHud by remember {
        mutableStateOf(
            DriveHudState(
                autoZoomLevel = MapHudPrefs.loadAutoZoomLevel(context),
                autoZoomWhileMoving = MapHudPrefs.loadAutoZoomOn(context),
            ),
        )
    }
    var showDriveSettings by remember { mutableStateOf(false) }
    var showMapSettings by remember { mutableStateOf(false) }
    var drivingHoursSinceBreak by remember { mutableDoubleStateOf(0.0) }
    var locationPermGranted by remember {
        mutableStateOf(
            ContextCompat.checkSelfPermission(
                context,
                Manifest.permission.ACCESS_FINE_LOCATION,
            ) == PackageManager.PERMISSION_GRANTED,
        )
    }
    val locationPermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted -> locationPermGranted = granted }

    val dataDir = remember {
        (context.getExternalFilesDir(null) ?: context.filesDir).also { it.mkdirs() }
    }
    val iconsDir = remember {
        File(context.filesDir, "icons").also { ensureIconsCopied(context, it) }
    }
    val indexDb = remember { File(dataDir, "place_index.db") }
    val pbfFile = remember { File(dataDir, "espa-atnbrufossen-corridor.osm.pbf") }

    fun refreshRoutes() {
        savedRoutes = listSavedRoutes(dataDir.absolutePath)
    }

    fun persistVehicle() {
        val limits = FfiVehicleLimits(
            axleWeightKg = axleKg.toDoubleOrNull(),
            heightM = heightM.toDoubleOrNull(),
            widthM = widthM.toDoubleOrNull(),
            totalWeightKg = null,
        )
        val ok = saveVehicleLimits(dataDir.absolutePath, limits)
        status = if (ok) "Vehicle limits saved" else "Failed to save vehicle limits"
    }

    LaunchedEffect(Unit) {
        val limits = loadVehicleLimits(dataDir.absolutePath)
        axleKg = limits.axleWeightKg?.toString().orEmpty()
        heightM = limits.heightM?.toString().orEmpty()
        widthM = limits.widthM?.toString().orEmpty()
        refreshRoutes()
        updateReminderDue = osmWeeklyReminderDue(dataDir.absolutePath)
        runCatching {
            val rest = loadCarRestSettings(dataDir.absolutePath)
            ecoEnabled = rest.ecoModeEnabled || ecoModeDefault(profile)
            val intervalH = rest.breakIntervalHours
            val minsLeft = ((intervalH - drivingHoursSinceBreak) * 60.0).coerceAtLeast(0.0)
            driveHud = driveHud.copy(
                ecoActive = ecoEnabled,
                minutesToBreak = minsLeft,
                distanceToTurnKm = null,
            )
        }
        if (!locationPermGranted) {
            locationPermissionLauncher.launch(Manifest.permission.ACCESS_FINE_LOCATION)
        }

        // Continuous hook poll via Handler so it survives Compose LaunchedEffect
        // cancellation that was observed mid-instrumented-test (after several screenshots).
    }
    DisposableEffect(locationPermGranted) {
        if (!locationPermGranted) {
            return@DisposableEffect onDispose { }
        }
        val lm = context.getSystemService(LocationManager::class.java)
        fun applyFix(loc: Location?) {
            if (loc == null || !loc.hasAltitude()) return
            // Test hook overrides live sensor in the poll loop.
            if (NaviMapTestHooks.gpsAltitudeM != null) return
            driveHud = driveHud.copy(altitudeM = loc.altitude)
            NaviMapTestHooks.lastHudAltitudeM = loc.altitude
        }
        val listener = LocationListener { loc -> applyFix(loc) }
        try {
            applyFix(
                lm.getLastKnownLocation(LocationManager.GPS_PROVIDER)
                    ?: lm.getLastKnownLocation(LocationManager.NETWORK_PROVIDER),
            )
            when {
                lm.isProviderEnabled(LocationManager.GPS_PROVIDER) ->
                    lm.requestLocationUpdates(LocationManager.GPS_PROVIDER, 1_000L, 1f, listener)
                lm.isProviderEnabled(LocationManager.NETWORK_PROVIDER) ->
                    lm.requestLocationUpdates(LocationManager.NETWORK_PROVIDER, 2_000L, 5f, listener)
            }
        } catch (_: SecurityException) {
            // Permission revoked mid-session; HUD keeps last altitude or "--".
        }
        onDispose {
            runCatching { lm.removeUpdates(listener) }
        }
    }
    DisposableEffect(Unit) {
        val handler = Handler(Looper.getMainLooper())
        val runnable = object : Runnable {
            override fun run() {
                try {
                    val pending = NaviMapTestHooks.pendingRoute
                    if (pending != null) {
                        NaviMapTestHooks.pendingRoute = null
                        val iconPng = NaviMapTestHooks.pendingIconPng
                        mapState = MapRouteState(
                            polyline = pending.routePolyline,
                            poiLat = pending.poiLat,
                            poiLon = pending.poiLon,
                            poiName = pending.poiName,
                            poiIconPng = iconPng,
                            layerEpoch = mapState.layerEpoch + 1,
                        )
                        status = pending.report
                    }
                    val cam = NaviMapTestHooks.pendingCamera
                    if (cam != null) {
                        NaviMapTestHooks.pendingCamera = null
                        mapState = mapState.copy(
                            cameraLat = cam.first,
                            cameraLon = cam.second,
                            cameraZoom = cam.third,
                            layerEpoch = mapState.layerEpoch + 1,
                        )
                    }
                    val rotReq = NaviMapTestHooks.requestRotationMode
                    if (rotReq != null) {
                        NaviMapTestHooks.requestRotationMode = null
                        driveHud = driveHud.copy(rotationMode = rotReq)
                        NaviMapTestHooks.lastRotationMode = rotReq
                    }
                    val pendingBearing = NaviMapTestHooks.pendingBearing
                    if (pendingBearing != null) {
                        NaviMapTestHooks.pendingBearing = null
                        if (NaviMapTestHooks.applyBearingToMap) {
                            mapState = mapState.copy(cameraBearing = pendingBearing)
                        }
                        NaviMapTestHooks.lastCameraBearing = pendingBearing
                    }
                    val tracks = NaviMapTestHooks.pendingTracks
                    if (tracks != null) {
                        NaviMapTestHooks.pendingTracks = null
                        mapState = mapState.copy(
                            tracks = tracks,
                            layerEpoch = mapState.layerEpoch + 1,
                        )
                        NaviMapTestHooks.lastTrackIds = tracks.map { it.id }
                        NaviMapTestHooks.tracksEpoch += 1
                    }
                    hideChrome = NaviMapTestHooks.hideUiChrome
                    hideSearch = NaviMapTestHooks.hideSearchChrome
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
                        driveHud = driveHud.copy(
                            showTripEta = tripReq,
                            tripEtaMinutes = when {
                                tripReq && driveHud.tripEtaMinutes == null -> 95.0
                                else -> driveHud.tripEtaMinutes
                            },
                        )
                        NaviMapTestHooks.lastShowTripEta = tripReq
                    }
                    val breakReq = NaviMapTestHooks.requestBreakReminders
                    if (breakReq != null) {
                        NaviMapTestHooks.requestBreakReminders = null
                        driveHud = driveHud.copy(breakRemindersEnabled = breakReq)
                        NaviMapTestHooks.lastBreakRemindersEnabled = breakReq
                    }
                    val hookAlt = NaviMapTestHooks.gpsAltitudeM
                    if (hookAlt != null && driveHud.altitudeM != hookAlt) {
                        driveHud = driveHud.copy(altitudeM = hookAlt)
                    }
                    NaviMapTestHooks.lastReportedLayerCount = mapLayerCount
                    NaviMapTestHooks.driveSettingsOpen = showDriveSettings
                    NaviMapTestHooks.mapSettingsOpen = showMapSettings
                    NaviMapTestHooks.lastRotationMode = driveHud.rotationMode
                    // Do not overwrite lastCameraZoom / lat / lon from Compose state here —
                    // MapLibre camera-idle is the source of truth (user pan/pinch/double-tap).
                    NaviMapTestHooks.lastCameraBearing = mapState.cameraBearing
                    NaviMapTestHooks.lastBreakRemindersEnabled = driveHud.breakRemindersEnabled
                    NaviMapTestHooks.lastShowTripEta = driveHud.showTripEta
                    NaviMapTestHooks.lastMinutesToBreak = driveHud.minutesToBreak
                    NaviMapTestHooks.lastHudAltitudeM = driveHud.altitudeM

                    val targetBearing = when (driveHud.rotationMode) {
                        MapRotationMode.NorthUp -> 0.0
                        MapRotationMode.Compass -> NaviMapTestHooks.magneticHeadingDeg
                        MapRotationMode.DirectionOfTravel -> NaviMapTestHooks.gpsBearingDeg
                    }
                    if (targetBearing != null) {
                        val cur = mapState.cameraBearing
                        if (kotlin.math.abs(cur - targetBearing) > 0.05) {
                            if (NaviMapTestHooks.applyBearingToMap) {
                                mapState = mapState.copy(cameraBearing = targetBearing)
                            }
                            NaviMapTestHooks.lastCameraBearing = targetBearing
                        }
                    }
                } catch (e: Exception) {
                    android.util.Log.e("HudVerification", "hook poll error", e)
                }
                handler.postDelayed(this, 250)
            }
        }
        handler.post(runnable)
        onDispose {
            handler.removeCallbacks(runnable)
        }
    }

    fun runSearch(q: String) {
        searchJob?.cancel()
        if (q.trim().length < 2) {
            hits = emptyList()
            return
        }
        searchBusy = true
        searchJob = scope.launch {
            delay(200)
            val list = withContext(Dispatchers.IO) {
                searchPlaces(indexDb.absolutePath, q.trim(), 20u)
            }
            hits = when (searchMode) {
                SearchMode.Place -> list.filter {
                    val k = it.kind.lowercase()
                    k.contains("place") || k.contains("amenity") || k.contains("tourism") ||
                        k.contains("peak") || k.contains("hut") || k.contains("natural")
                }.ifEmpty { list }
                SearchMode.Address -> list.filter {
                    val k = it.kind.lowercase()
                    k.contains("highway") || k.contains("place") || k.contains("addr")
                }.ifEmpty { list }
            }
            searchBusy = false
        }
    }

    fun applyHit(hit: PlaceHit) {
        val wp = Waypoint(name = hit.name, lat = hit.lat, lon = hit.lon)
        when (searchTarget) {
            SearchTarget.To -> toPoint = wp
            SearchTarget.Via -> viaPoint = wp
        }
        mapState = mapState.copy(
            cameraLat = hit.lat,
            cameraLon = hit.lon,
            cameraZoom = 12.0,
            poiLat = hit.lat,
            poiLon = hit.lon,
            poiName = hit.name,
            layerEpoch = mapState.layerEpoch + 1,
        )
        query = hit.name
        hits = emptyList()
        status = "Set ${searchTarget.name.lowercase()}: ${hit.name} (${hit.kind})"
    }

    Box(modifier = Modifier.fillMaxSize()) {
        CorridorMapView(
            state = mapState,
            modifier = Modifier.fillMaxSize(),
            onLayerCount = { mapLayerCount = it },
        )

        Column(
            modifier = Modifier
                .align(Alignment.TopCenter)
                .fillMaxWidth()
                .zIndex(1f)
                .padding(10.dp)
                .heightIn(max = 520.dp)
                .verticalScroll(rememberScrollState()),
        ) {
            if (!hideChrome) {
            TopDriveHud(
                state = driveHud.copy(
                    ecoActive = ecoEnabled,
                ),
                expanded = showMapSettings,
                onToggleExpanded = {
                    showMapSettings = !showMapSettings
                    if (showMapSettings) showDriveSettings = false
                },
                modifier = Modifier.padding(bottom = 8.dp),
            )
            if (!hideSearch) {
            Surface(
                shape = RoundedCornerShape(12.dp),
                tonalElevation = 4.dp,
                modifier = Modifier
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
                            FilterChip(
                                selected = searchTarget == SearchTarget.To,
                                onClick = { searchTarget = SearchTarget.To },
                                label = { Text("To") },
                            )
                            FilterChip(
                                selected = searchTarget == SearchTarget.Via,
                                onClick = { searchTarget = SearchTarget.Via },
                                label = { Text("Via") },
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
                        TextButton(
                            onClick = { showTools = !showTools },
                            modifier = Modifier.testTag("btn_tools"),
                        ) {
                            Text(if (showTools) "Hide" else "Tools")
                        }
                    }
                    Text(
                        "To: ${toPoint.name.ifBlank { "(unset)" }}  |  Via: ${viaPoint?.name ?: "(none)"}",
                        style = MaterialTheme.typography.bodySmall,
                    )
                    OutlinedTextField(
                        value = query,
                        onValueChange = {
                            query = it
                            runSearch(it)
                        },
                        modifier = Modifier.fillMaxWidth(),
                        singleLine = true,
                        placeholder = {
                            Text(
                                when (searchMode) {
                                    SearchMode.Place -> "Search place, hut, amenity..."
                                    SearchMode.Address -> "Search road or settlement..."
                                },
                            )
                        },
                    )
                    if (searchBusy) {
                        Text("Searching...", style = MaterialTheme.typography.bodySmall)
                    }
                    if (hits.isNotEmpty()) {
                        hits.take(8).forEach { hit ->
                            Column(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .clickable { applyHit(hit) }
                                    .padding(vertical = 6.dp, horizontal = 4.dp),
                            ) {
                                Text(hit.name, style = MaterialTheme.typography.bodyLarge)
                                Text(hit.kind, style = MaterialTheme.typography.bodySmall)
                            }
                        }
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        TextButton(
                            onClick = {
                                val fix = lastGpsFix()
                                if (fix.available) {
                                    fromPoint = Waypoint("GPS", fix.lat, fix.lon)
                                    mapState = mapState.copy(
                                        cameraLat = fix.lat,
                                        cameraLon = fix.lon,
                                        cameraZoom = 12.0,
                                        layerEpoch = mapState.layerEpoch + 1,
                                    )
                                    status = "Start from GPS: ${fix.lat}, ${fix.lon}"
                                } else {
                                    status = "GPS unavailable"
                                }
                            },
                        ) { Text("Start from GPS") }
                        TextButton(
                            onClick = {
                                val last = savedRoutes.firstOrNull {
                                    it.lastBreakLat != null && it.lastBreakLon != null
                                }
                                if (last?.lastBreakLat != null && last.lastBreakLon != null) {
                                    fromPoint = Waypoint(
                                        "Last stop",
                                        last.lastBreakLat!!,
                                        last.lastBreakLon!!,
                                    )
                                    status = "Continue from last stop on ${last.endName}"
                                } else if (viaPoint != null) {
                                    fromPoint = viaPoint
                                    status = "Continue from via (${viaPoint!!.name})"
                                } else {
                                    status = "No last stop saved yet"
                                }
                            },
                        ) { Text("Continue from last stop") }
                    }
                }
            }

            Spacer(modifier = Modifier.height(8.dp))

            Surface(
                shape = RoundedCornerShape(12.dp),
                tonalElevation = 3.dp,
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("profile_menu"),
            ) {
                Column(modifier = Modifier.padding(10.dp)) {
                    Text("Profile", style = MaterialTheme.typography.labelLarge)
                    Row(
                        modifier = Modifier
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
                                    Text(
                                        when (p) {
                                            TravelProfile.CAR -> "Car"
                                            TravelProfile.BICYCLE -> "Bicycle"
                                            TravelProfile.HIKING -> "Hiking"
                                            TravelProfile.MOTORCYCLE -> "Motorcycle"
                                            else -> p.name
                                        },
                                    )
                                },
                            )
                        }
                    }
                    Text(
                        "Also in enum (not primary menu): Truck, CarElectric, TruckElectric, MotorcycleElectric",
                        style = MaterialTheme.typography.bodySmall,
                    )
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
                                }
                            },
                            enabled = ecoModeToggleable(profile),
                        )
                    }
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Text("Avoid motorways/trunk/primary")
                        Switch(
                            checked = avoidMajor,
                            onCheckedChange = { on ->
                                avoidMajor = on
                                // Demo share until live route metrics are wired to this toggle.
                                prioritySharePct = if (on) 72.5 else 41.0
                                status = formatAvoidMajorReport(avoidMajor, prioritySharePct)
                            },
                        )
                    }
                    Text(
                        formatAvoidMajorReport(avoidMajor, prioritySharePct),
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }

            Spacer(modifier = Modifier.height(8.dp))

            Surface(
                shape = RoundedCornerShape(12.dp),
                tonalElevation = 3.dp,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Column(modifier = Modifier.padding(10.dp)) {
                    Text("Vehicle limits", style = MaterialTheme.typography.labelLarge)
                    OutlinedTextField(
                        value = axleKg,
                        onValueChange = { axleKg = it },
                        label = { Text("Axle weight (kg)") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                    )
                    OutlinedTextField(
                        value = heightM,
                        onValueChange = { heightM = it },
                        label = { Text("Height (m)") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                    )
                    OutlinedTextField(
                        value = widthM,
                        onValueChange = { widthM = it },
                        label = { Text("Width (m)") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Button(onClick = { persistVehicle() }, modifier = Modifier.fillMaxWidth()) {
                        Text("Save vehicle limits")
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
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text("Saved routes", style = MaterialTheme.typography.labelLarge)
                        TextButton(onClick = { refreshRoutes() }) { Text("Refresh") }
                    }
                    if (savedRoutes.isEmpty()) {
                        Text("No saved routes", style = MaterialTheme.typography.bodySmall)
                    } else {
                        savedRoutes.forEach { route ->
                            Row(
                                modifier = Modifier
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
                                            status = "Deleted route ${route.id.take(8)}"
                                        }
                                    },
                                ) { Text("Delete") }
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
                            val viaJson = viaPoint?.let {
                                """[{"name":"${it.name}","lat":${it.lat},"lon":${it.lon}}]"""
                            } ?: "[]"
                            val report = saveNamedRoute(
                                dataDir = dataDir.absolutePath,
                                startLat = start.lat,
                                startLon = start.lon,
                                startName = start.name,
                                endLat = toPoint.lat,
                                endLon = toPoint.lon,
                                endName = toPoint.name,
                                viaJson = viaJson,
                                profile = profile.name.lowercase(),
                                summaryJson = """{"avoid_major":$avoidMajor,"priority_share_pct":$prioritySharePct}""",
                            )
                            refreshRoutes()
                            status = report
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("Save current To/Via as route")
                    }
                }
            }
            } // end if (!hideSearch)
            } // end if (!hideChrome)
        }

        if (showTools && !hideChrome) {
            Surface(
                shape = RoundedCornerShape(12.dp),
                tonalElevation = 6.dp,
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .fillMaxWidth()
                    .padding(10.dp)
                    .padding(bottom = 88.dp)
                    .heightIn(max = 220.dp)
                    .testTag("tools_menu"),
            ) {
                Column(
                    modifier = Modifier
                        .padding(12.dp)
                        .verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Text("Region / debug", style = MaterialTheme.typography.titleSmall)
                    Text("Map layers: $mapLayerCount", style = MaterialTheme.typography.bodySmall)
                    if (updateReminderDue) {
                        Text(
                            "Weekly OSM update check is due (opt-in reminder — nothing was downloaded).",
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                    Button(
                        onClick = {
                            scope.launch {
                                status = "Provisioning region..."
                                val url = System.getProperty("navi.fixture.pbf.url")
                                    ?: "http://10.0.2.2:8765/espa-atnbrufossen-corridor.osm.pbf"
                                val report = withContext(Dispatchers.IO) {
                                    provisionRegionData(
                                        dataDir = dataDir.absolutePath,
                                        pbfUrl = url,
                                        pbfFilename = "espa-atnbrufossen-corridor.osm.pbf",
                                        elevationTarUrl = "http://10.0.2.2:8765/elevation-corridor.tar",
                                    )
                                }
                                status = report
                                if (report.contains("PASS") && pbfFile.isFile) {
                                    status = ensurePlaceIndex(
                                        pbfFile.absolutePath,
                                        indexDb.absolutePath,
                                    )
                                }
                            }
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("Download corridor region + build place index")
                    }
                    Text(
                        "OSM updates (Geofabrik) — opt-in, never silent",
                        style = MaterialTheme.typography.labelLarge,
                    )
                    Button(
                        onClick = {
                            scope.launch {
                                status = withContext(Dispatchers.IO) {
                                    checkOsmUpdates(dataDir.absolutePath)
                                }
                                pendingUpdatePlan = status
                                updateReminderDue = osmWeeklyReminderDue(dataDir.absolutePath)
                            }
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("Check for OSM updates")
                    }
                    Button(
                        onClick = {
                            scope.launch {
                                val plan = pendingUpdatePlan
                                if (plan.isNullOrBlank() || plan.contains("up to date", ignoreCase = true)) {
                                    status = "Run Check for OSM updates first, or already up to date."
                                    return@launch
                                }
                                if (plan.contains("Unsupported", ignoreCase = true) ||
                                    plan.contains("unsupported", ignoreCase = true)
                                ) {
                                    status = "This extract has no Geofabrik binding. Bind a region or re-provision."
                                    return@launch
                                }
                                status = "Applying OSM update (user confirmed)..."
                                status = withContext(Dispatchers.IO) {
                                    applyOsmUpdate(dataDir.absolutePath)
                                }
                                pendingUpdatePlan = null
                            }
                        },
                        modifier = Modifier.fillMaxWidth(),
                        enabled = !pendingUpdatePlan.isNullOrBlank(),
                    ) {
                        Text("Apply pending OSM update")
                    }
                    Button(
                        onClick = {
                            scope.launch {
                                status = withContext(Dispatchers.IO) {
                                    bindGeofabrikRegion(
                                        dataDir = dataDir.absolutePath,
                                        geofabrikRegion = "europe/norway/ostlandet",
                                        pbfFilename = "ostlandet-latest.osm.pbf",
                                        localSequence = null,
                                    )
                                }
                            }
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("Bind Geofabrik: europe/norway/ostlandet")
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
                                status = if (on) {
                                    "Weekly OSM check reminder enabled"
                                } else {
                                    "Weekly OSM check reminder disabled"
                                }
                            },
                        )
                    }
                    Button(
                        onClick = {
                            scope.launch {
                                status = "Running corridor route..."
                                val elev = File(dataDir, "elevation")
                                val cache = File(dataDir, "graph-cache")
                                val result = withContext(Dispatchers.IO) {
                                    runCarCorridorPipeline(
                                        pbfPath = pbfFile.absolutePath,
                                        elevDir = elev.absolutePath,
                                        cacheDir = cache.absolutePath,
                                        breakIntervalHours = 1.0,
                                    )
                                }
                                val iconPng = withContext(Dispatchers.IO) {
                                    rasterizeIconPng(
                                        key = result.poiIconKey.ifBlank { "fuel" },
                                        theme = FfiIconTheme.DAY,
                                        width = 64u,
                                        height = 64u,
                                        bundledDir = iconsDir.absolutePath,
                                    )
                                }
                                mapState = MapRouteState(
                                    polyline = result.routePolyline,
                                    poiLat = result.poiLat,
                                    poiLon = result.poiLon,
                                    poiName = result.poiName,
                                    poiIconPng = iconPng,
                                    layerEpoch = mapState.layerEpoch + 1,
                                )
                                status = result.report
                            }
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("Route Espa -> Atnbrufossen")
                    }
                    Text(status, style = MaterialTheme.typography.bodySmall)
                }
            }
        }

        if (!hideChrome) {
            Column(
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .fillMaxWidth()
                    .padding(10.dp)
                    .zIndex(1f),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                BottomDriveHud(
                    state = driveHud.copy(ecoActive = ecoEnabled),
                    iconDir = iconsDir.absolutePath,
                    onZoomIn = {
                        val z = (mapState.cameraZoom ?: 12.0) + 1.0
                        val next = z.coerceAtMost(20.0)
                        mapState = mapState.copy(
                            cameraZoom = next,
                            layerEpoch = mapState.layerEpoch + 1,
                        )
                        NaviMapTestHooks.lastCameraZoom = next
                    },
                    onZoomOut = {
                        val z = (mapState.cameraZoom ?: 12.0) - 1.0
                        val next = z.coerceAtLeast(3.0)
                        mapState = mapState.copy(
                            cameraZoom = next,
                            layerEpoch = mapState.layerEpoch + 1,
                        )
                        NaviMapTestHooks.lastCameraZoom = next
                    },
                    onOpenSettings = {
                        showDriveSettings = !showDriveSettings
                        if (showDriveSettings) showMapSettings = false
                        NaviMapTestHooks.driveSettingsOpen = showDriveSettings
                    },
                )
            }

            // Status chip: bottom-end so it never covers MapLibre/OSM attribution (bottom-left).
            Text(
                text = status.take(120),
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .zIndex(3f)
                    .padding(end = 12.dp, bottom = 88.dp)
                    .background(Color(0xCCFFFFFF), RoundedCornerShape(8.dp))
                    .padding(horizontal = 10.dp, vertical = 8.dp)
                    .testTag("status_toast"),
                style = MaterialTheme.typography.bodySmall,
            )

            if (showMapSettings) {
                MapSettingsSheet(
                    state = driveHud.copy(ecoActive = ecoEnabled),
                    onRotation = { mode ->
                        driveHud = driveHud.copy(rotationMode = mode)
                        NaviMapTestHooks.lastRotationMode = mode
                        val bearing = when (mode) {
                            MapRotationMode.NorthUp -> 0.0
                            MapRotationMode.Compass ->
                                NaviMapTestHooks.magneticHeadingDeg ?: mapState.cameraBearing
                            MapRotationMode.DirectionOfTravel ->
                                NaviMapTestHooks.gpsBearingDeg ?: mapState.cameraBearing
                        }
                        if (NaviMapTestHooks.applyBearingToMap) {
                            mapState = mapState.copy(cameraBearing = bearing)
                        }
                        NaviMapTestHooks.lastCameraBearing = bearing
                    },
                    onToggleTripEta = { on ->
                        driveHud = driveHud.copy(
                            showTripEta = on,
                            tripEtaMinutes = when {
                                on && driveHud.tripEtaMinutes == null -> 95.0
                                else -> driveHud.tripEtaMinutes
                            },
                        )
                    },
                    onToggleBreakReminders = { on ->
                        driveHud = driveHud.copy(breakRemindersEnabled = on)
                    },
                    onToggleAutoZoom = { on ->
                        driveHud = driveHud.copy(autoZoomWhileMoving = on)
                        MapHudPrefs.saveAutoZoom(
                            context,
                            driveHud.autoZoomLevel,
                            enabled = on,
                        )
                        if (on) {
                            mapState = mapState.copy(
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
                            mapState = mapState.copy(
                                cameraZoom = next,
                                layerEpoch = mapState.layerEpoch + 1,
                            )
                            NaviMapTestHooks.lastCameraZoom = next
                        }
                    },
                    onClose = { showMapSettings = false },
                    modifier = Modifier
                        .align(Alignment.TopCenter)
                        .zIndex(4f)
                        .padding(10.dp),
                )
            }

            if (showDriveSettings) {
                DriveSettingsSheet(
                    dataDir = dataDir.absolutePath,
                    iconDir = iconsDir.absolutePath,
                    ecoActive = ecoEnabled,
                    onEcoChange = {
                        ecoEnabled = it
                        driveHud = driveHud.copy(ecoActive = it)
                    },
                    onApplied = {
                        showDriveSettings = false
                        NaviMapTestHooks.driveSettingsOpen = false
                        status = "Drive settings applied"
                        driveHud = driveHud.copy(ecoActive = ecoEnabled)
                        runCatching {
                            val rest = loadCarRestSettings(dataDir.absolutePath)
                            val intervalH = rest.breakIntervalHours
                            val minsLeft =
                                ((intervalH - drivingHoursSinceBreak) * 60.0).coerceAtLeast(0.0)
                            driveHud = driveHud.copy(
                                ecoActive = rest.ecoModeEnabled || ecoEnabled,
                                minutesToBreak = minsLeft,
                            )
                            ecoEnabled = rest.ecoModeEnabled || ecoEnabled
                        }
                    },
                    onDismiss = {
                        showDriveSettings = false
                        NaviMapTestHooks.driveSettingsOpen = false
                    },
                    modifier = Modifier
                        .align(Alignment.BottomCenter)
                        .zIndex(4f)
                        .padding(10.dp),
                )
            }
        }
    }
}

@OptIn(ExperimentalComposeUiApi::class)
@Composable
private fun CorridorMapView(
    state: MapRouteState,
    modifier: Modifier = Modifier,
    onLayerCount: (Int) -> Unit,
) {
    val context = LocalContext.current
    val mapView = remember {
        MapView(context).apply {
            setBackgroundColor(android.graphics.Color.parseColor("#E8EEF2"))
            onCreate(null)
        }
    }
    var mapRef by remember { mutableStateOf<MapLibreMap?>(null) }
    data class OverlayMark(val x: Float, val y: Float, val track: TrackMarker, val icon: Bitmap?)
    var overlayMarks by remember { mutableStateOf<List<OverlayMark>>(emptyList()) }
    val iconCache = remember { mutableMapOf<String, Bitmap>() }
    val styleReady = remember { androidx.compose.runtime.mutableStateOf(false) }
    val styleLoadStarted = remember { java.util.concurrent.atomic.AtomicBoolean(false) }
    val stateRef = remember { java.util.concurrent.atomic.AtomicReference(state) }
    stateRef.set(state)

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
        val marks = latest.tracks.map { t ->
            val screen = map.projection.toScreenLocation(LatLng(t.lat, t.lon))
            OverlayMark(screen.x.toFloat(), screen.y.toFloat(), t, loadTrackBitmap(t.symbolKey))
        }
        overlayMarks = marks
        NaviMapTestHooks.lastTrackOverlayCount = marks.size
        NaviMapTestHooks.lastTrackFeatureCount = latest.tracks.size
        NaviMapTestHooks.lastTrackImagesReady = marks.count { it.icon != null }
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
            mapView.onPause()
            mapView.onStop()
            mapView.onDestroy()
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
                    map.addOnCameraIdleListener {
                        refreshTrackOverlay(map)
                        val pos = map.cameraPosition
                        NaviMapTestHooks.lastCameraZoom = pos.zoom
                        NaviMapTestHooks.lastCameraBearing = pos.bearing
                        pos.target?.let { target ->
                            NaviMapTestHooks.lastCameraLat = target.latitude
                            NaviMapTestHooks.lastCameraLon = target.longitude
                        }
                    }
                    map.addOnMoveListener(object : org.maplibre.android.maps.MapLibreMap.OnMoveListener {
                        override fun onMoveBegin(detector: org.maplibre.android.gestures.MoveGestureDetector) {
                            NaviMapTestHooks.mapGestureMoves += 1
                        }

                        override fun onMove(detector: org.maplibre.android.gestures.MoveGestureDetector) {}

                        override fun onMoveEnd(detector: org.maplibre.android.gestures.MoveGestureDetector) {}
                    })
                    map.addOnScaleListener(object : org.maplibre.android.maps.MapLibreMap.OnScaleListener {
                        override fun onScaleBegin(detector: org.maplibre.android.gestures.StandardScaleGestureDetector) {
                            NaviMapTestHooks.mapGestureScales += 1
                        }

                        override fun onScale(detector: org.maplibre.android.gestures.StandardScaleGestureDetector) {}

                        override fun onScaleEnd(detector: org.maplibre.android.gestures.StandardScaleGestureDetector) {}
                    })
                    map.uiSettings.setAllGesturesEnabled(true)
                    map.setStyle("https://tiles.openfreemap.org/styles/liberty") { _ ->
                        styleReady.value = true
                        NaviMapTestHooks.styleReady = true
                    }
                }
            },
        )

        // Forward touches so the marker overlay Canvas does not block MapLibre pan/pinch.
        Canvas(
            modifier = Modifier
                .fillMaxSize()
                .pointerInteropFilter { event ->
                    mapView.dispatchTouchEvent(event)
                },
        ) {
            val labelPaint = AndroidPaint(AndroidPaint.ANTI_ALIAS_FLAG).apply {
                color = android.graphics.Color.BLACK
                textSize = 34f
                style = AndroidPaint.Style.FILL
            }
            val haloPaint = AndroidPaint(AndroidPaint.ANTI_ALIAS_FLAG).apply {
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
                        srcSize = androidx.compose.ui.unit.IntSize(bmp.width, bmp.height),
                        dstOffset = androidx.compose.ui.unit.IntOffset(
                            (mark.x - w / 2f).toInt(),
                            (mark.y - h / 2f).toInt(),
                        ),
                        dstSize = androidx.compose.ui.unit.IntSize(w.toInt(), h.toInt()),
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
        }
    }

    LaunchedEffect(state.layerEpoch, styleReady.value) {
        if (!styleReady.value) return@LaunchedEffect
        mapView.getMapAsync { map ->
            mapRef = map
            map.getStyle { style ->
                val latest = stateRef.get()
                applyRouteToStyle(style, latest)
                applyTracksToStyle(style, latest.tracks, mapView.context)
                map.triggerRepaint()
                onLayerCount(style.layers.size)
                if (latest.cameraLat != null && latest.cameraLon != null) {
                    val pos = org.maplibre.android.camera.CameraPosition.Builder()
                        .target(LatLng(latest.cameraLat, latest.cameraLon))
                        .zoom(latest.cameraZoom ?: 12.0)
                        .bearing(latest.cameraBearing)
                        .build()
                    map.moveCamera(CameraUpdateFactory.newCameraPosition(pos))
                } else if (latest.polyline.isNotBlank()) {
                    val pts = parsePolyline(latest.polyline)
                    if (pts.size >= 2) {
                        val bounds = LatLngBounds.Builder().apply {
                            pts.forEach { include(it) }
                            if (latest.poiLat != 0.0 || latest.poiLon != 0.0) {
                                include(LatLng(latest.poiLat, latest.poiLon))
                            }
                        }.build()
                        map.animateCamera(CameraUpdateFactory.newLatLngBounds(bounds, 64))
                    }
                } else {
                    val pos = org.maplibre.android.camera.CameraPosition.Builder()
                        .target(LatLng(61.2, 10.7))
                        .zoom(latest.cameraZoom ?: 6.5)
                        .bearing(latest.cameraBearing)
                        .build()
                    map.moveCamera(CameraUpdateFactory.newCameraPosition(pos))
                }
                refreshTrackOverlay(map)
                NaviMapTestHooks.tracksAppliedEpoch = NaviMapTestHooks.tracksEpoch
            }
        }
    }

    // Bearing-only updates must not re-apply Compose zoom (that undoes user pinch / double-tap).
    LaunchedEffect(state.cameraBearing, styleReady.value) {
        if (!styleReady.value) return@LaunchedEffect
        if (!NaviMapTestHooks.applyBearingToMap) {
            NaviMapTestHooks.lastCameraBearing = state.cameraBearing
            return@LaunchedEffect
        }
        val bearing = stateRef.get().cameraBearing
        mapView.getMapAsync { map ->
            try {
                map.moveCamera(
                    CameraUpdateFactory.newCameraPosition(
                        org.maplibre.android.camera.CameraPosition.Builder(map.cameraPosition)
                            .bearing(bearing)
                            .build(),
                    ),
                )
                NaviMapTestHooks.lastCameraBearing = bearing
            } catch (e: Exception) {
                android.util.Log.e("HudVerification", "bearing update failed", e)
            }
        }
    }

    // Zoom updates from HUD / pendingCamera (layerEpoch path also sets zoom).
    LaunchedEffect(state.cameraZoom, styleReady.value) {
        if (!styleReady.value) return@LaunchedEffect
        val zoom = stateRef.get().cameraZoom ?: return@LaunchedEffect
        mapView.getMapAsync { map ->
            try {
                map.moveCamera(
                    CameraUpdateFactory.newCameraPosition(
                        org.maplibre.android.camera.CameraPosition.Builder(map.cameraPosition)
                            .zoom(zoom)
                            .build(),
                    ),
                )
                NaviMapTestHooks.lastCameraZoom = zoom
            } catch (e: Exception) {
                android.util.Log.e("HudVerification", "zoom update failed", e)
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
                val bmp = android.graphics.Bitmap.createBitmap(
                    w,
                    h,
                    android.graphics.Bitmap.Config.ARGB_8888,
                )
                val handler = android.os.Handler(android.os.Looper.getMainLooper())
                val surface = findSurfaceView(mapView)
                val window = (mapView.context as? android.app.Activity)?.window
                val listener = android.view.PixelCopy.OnPixelCopyFinishedListener { result ->
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
            val idleListener = object : MapView.OnDidBecomeIdleListener {
                override fun onDidBecomeIdle() {
                    if (done) return
                    done = true
                    mapView.removeOnDidBecomeIdleListener(this)
                    captureView()
                }
            }
            val frameListener = object : MapView.OnDidFinishRenderingFrameListener {
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

private fun applyRouteToStyle(style: Style, state: MapRouteState) {
    if (state.polyline.isNotBlank()) {
        val pts = parsePolyline(state.polyline).map { Point.fromLngLat(it.longitude, it.latitude) }
        if (pts.size >= 2) {
            if (style.getSource("route-src") == null) {
                style.addSource(GeoJsonSource("route-src", LineString.fromLngLats(pts)))
                style.addLayer(
                    LineLayer("route-line", "route-src").withProperties(
                        PropertyFactory.lineColor("#C62828"),
                        PropertyFactory.lineWidth(4f),
                    ),
                )
            } else {
                (style.getSource("route-src") as? GeoJsonSource)
                    ?.setGeoJson(LineString.fromLngLats(pts))
            }
        }
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
                ),
            )
        } else {
            (style.getSource("poi-src") as? GeoJsonSource)
                ?.setGeoJson(FeatureCollection.fromFeature(feature))
        }
    }
}

private fun applyTracksToStyle(
    style: Style,
    tracks: List<TrackMarker>,
    context: android.content.Context,
) {
    val features = tracks.map { t ->
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
        val halo = CircleLayer("tracks-halo", "tracks-src").withProperties(
            PropertyFactory.circleRadius(26f),
            PropertyFactory.circleColor("#FFEB3B"),
            PropertyFactory.circleOpacity(1f),
            PropertyFactory.circleStrokeWidth(3f),
            PropertyFactory.circleStrokeColor("#000000"),
        )
        val symbols = SymbolLayer("tracks-layer", "tracks-src").withProperties(
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

private fun parsePolyline(encoded: String): List<LatLng> {
    return encoded.split(';').mapNotNull { part ->
        val bits = part.split(',')
        if (bits.size != 2) return@mapNotNull null
        val lon = bits[0].toDoubleOrNull() ?: return@mapNotNull null
        val lat = bits[1].toDoubleOrNull() ?: return@mapNotNull null
        LatLng(lat, lon)
    }
}

private fun ensureIconsCopied(context: android.content.Context, dest: File) {
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
