package no.navi.app

import android.graphics.BitmapFactory
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
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
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.maplibre.android.MapLibre
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.geometry.LatLngBounds
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
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
    var weeklyReminder by remember { mutableStateOf(false) }
    var pendingUpdatePlan by remember { mutableStateOf<String?>(null) }
    var updateReminderDue by remember { mutableStateOf(false) }

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
        while (true) {
            delay(250)
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
            hideChrome = NaviMapTestHooks.hideUiChrome
            NaviMapTestHooks.lastReportedLayerCount = mapLayerCount
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
                .padding(10.dp)
                .heightIn(max = 520.dp)
                .verticalScroll(rememberScrollState()),
        ) {
            if (!hideChrome) {
            Surface(
                shape = RoundedCornerShape(12.dp),
                tonalElevation = 4.dp,
                modifier = Modifier.fillMaxWidth(),
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
                        TextButton(onClick = { showTools = !showTools }) {
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
                modifier = Modifier.fillMaxWidth(),
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
                    .heightIn(max = 220.dp),
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
        } else if (!hideChrome) {
            Text(
                text = status.take(160),
                modifier = Modifier
                    .align(Alignment.BottomStart)
                    .padding(12.dp)
                    .background(Color(0xCCFFFFFF), RoundedCornerShape(8.dp))
                    .padding(8.dp),
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
}

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

    DisposableEffect(Unit) {
        mapView.onStart()
        mapView.onResume()
        onDispose {
            mapView.onPause()
            mapView.onStop()
            mapView.onDestroy()
        }
    }

    AndroidView(
        factory = { mapView },
        modifier = modifier,
        update = { view ->
            view.getMapAsync { map ->
                map.setStyle("https://tiles.openfreemap.org/styles/liberty") { style ->
                    onLayerCount(style.layers.size)
                    applyRouteToStyle(style, state)
                    when {
                        state.cameraLat != null && state.cameraLon != null -> {
                            map.animateCamera(
                                CameraUpdateFactory.newLatLngZoom(
                                    LatLng(state.cameraLat!!, state.cameraLon!!),
                                    state.cameraZoom ?: 12.0,
                                ),
                            )
                        }
                        state.polyline.isNotBlank() -> {
                            val pts = parsePolyline(state.polyline)
                            if (pts.size >= 2) {
                                val bounds = LatLngBounds.Builder().apply {
                                    pts.forEach { include(it) }
                                    if (state.poiLat != 0.0 || state.poiLon != 0.0) {
                                        include(LatLng(state.poiLat, state.poiLon))
                                    }
                                }.build()
                                map.animateCamera(CameraUpdateFactory.newLatLngBounds(bounds, 64))
                            }
                        }
                        else -> {
                            map.animateCamera(
                                CameraUpdateFactory.newLatLngZoom(LatLng(61.2, 10.7), 6.5),
                            )
                        }
                    }
                    onLayerCount(style.layers.size)
                }
            }
        },
    )

    LaunchedEffect(state.layerEpoch) {
        mapView.getMapAsync { map ->
            map.getStyle { style ->
                applyRouteToStyle(style, state)
                onLayerCount(style.layers.size)
                if (state.cameraLat != null && state.cameraLon != null) {
                    map.animateCamera(
                        CameraUpdateFactory.newLatLngZoom(
                            LatLng(state.cameraLat, state.cameraLon),
                            state.cameraZoom ?: 12.0,
                        ),
                    )
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
    if (dest.isDirectory && dest.list()?.isNotEmpty() == true) return
    dest.mkdirs()
    try {
        val am = context.assets
        val names = am.list("icons") ?: return
        for (name in names) {
            am.open("icons/$name").use { input ->
                File(dest, name).outputStream().use { output -> input.copyTo(output) }
            }
        }
    } catch (_: Exception) {
    }
}
