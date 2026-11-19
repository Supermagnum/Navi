package no.navi.app

import android.os.SystemClock
import android.util.Log
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.compose.LocalLifecycleOwner
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import uniffi.navi.weatherMapSymbolsJson
import uniffi.navi.weatherMapZoomMax
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.abs
import kotlin.math.hypot
import kotlin.math.pow

/**
 * Overlay marks for the CoMaps drape. `icon` / `color` are `kml::BookmarkIcon` /
 * `kml::PredefinedColor` ordinals (CoMaps' own bookmark symbols; Navi's own sprites cannot be
 * added to the drape symbol atlas without rebuilding it, see docs/mwm-phase6b-drape.md).
 */
internal object DrapeIcon {
    const val NONE = 0
    const val HOTEL = 1
    const val MOUNTAIN = 12
    const val PARK = 15
    const val SIGHTS = 18
    const val TRANSPORT = 22
    const val INFORMATION = 32
}

internal object DrapeColor {
    const val RED = 1
    const val BLUE = 2
    const val PURPLE = 3
    const val YELLOW = 4
    const val BROWN = 6
    const val GREEN = 7
    const val ORANGE = 8
    const val LIGHT_BLUE = 10
    const val CYAN = 11
    const val TEAL = 12
    const val DEEP_ORANGE = 14
    const val GRAY = 15
}

private const val TAG = "ComapsDrapeMap"

/** The APRS sprites generated into the drape atlas (SVG sources in comaps-sdk/navi-symbols and core/src/icons/aprs); anything else keeps the pin. */
internal fun aprsSymbol(key: String): String? = key.takeIf { it in APRS_ATLAS_SYMBOLS }

private val APRS_ATLAS_SYMBOLS = setOf("aprs_car", "aprs_digi", "aprs_house", "aprs_human")

/** 0 = GPS (blue), 1 = estimated (amber), 2 = lost (grey): selects the drape puck sprite / arrow colour. */
internal fun puckState(
    positionLost: Boolean,
    estimated: Boolean,
): Int =
    when {
        positionLost -> 2
        estimated -> 1
        else -> 0
    }

private const val POLL_MS = 250L
private const val FOLLOW_CMD_GRACE_MS = 1_500L

/** One overlay mark before it is flattened for the JNI call. */
internal data class DrapeMark(
    val lat: Double,
    val lon: Double,
    val name: String,
    val icon: Int,
    val color: Int,
    /** Atlas symbol drawn instead of the predefined pin (Navi sprites, e.g. `aprs_car`); null = pin. */
    val symbol: String? = null,
)

/** A polyline overlay (tracks): `latlon` flattened `[lat, lon, ...]`. */
internal data class DrapeLine(
    val latlon: DoubleArray,
    val argb: Int,
    val widthPx: Float,
)

/**
 * Keeps overlay groups of the drape in sync, sending a group again only when its content
 * changed. Main thread only (the CoMaps BookmarkManager is single threaded).
 */
internal class DrapeOverlays(
    private val backend: ComapsDrape.Backend,
) {
    private val groups = HashMap<String, Long>()
    private val keys = HashMap<String, Int>()

    private fun group(name: String): Long = groups.getOrPut(name) { backend.createGroup(name) }

    fun marks(
        name: String,
        items: List<DrapeMark>,
    ) {
        val key = items.hashCode()
        if (keys[name] == key) return
        keys[name] = key
        val n = items.size
        val latlon = DoubleArray(n * 2)
        val names = Array(n) { "" }
        val icons = IntArray(n)
        val colors = IntArray(n)
        val symbols = Array(n) { "" }
        items.forEachIndexed { i, m ->
            latlon[2 * i] = m.lat
            latlon[2 * i + 1] = m.lon
            names[i] = m.name
            icons[i] = m.icon
            colors[i] = m.color
            symbols[i] = m.symbol.orEmpty()
        }
        backend.setMarks(group(name), latlon, names, icons, colors, if (items.any { it.symbol != null }) symbols else null)
    }

    fun lines(
        name: String,
        items: List<DrapeLine>,
    ) {
        val key = items.fold(1) { acc, l -> 31 * acc + l.latlon.contentHashCode() + l.argb }
        if (keys[name] == key) return
        keys[name] = key
        val counts = IntArray(items.size) { items[it].latlon.size / 2 }
        val flat = DoubleArray(items.sumOf { it.latlon.size })
        var off = 0
        for (l in items) {
            l.latlon.copyInto(flat, off)
            off += l.latlon.size
        }
        backend.setTracks(
            group(name),
            counts,
            flat,
            IntArray(items.size) { items[it].argb },
            FloatArray(items.size) { items[it].widthPx },
        )
    }

    /** Forget cached keys so every group is sent again (after a drape restart). */
    fun invalidate() = keys.clear()
}

/** `lon,lat;lon,lat;...` -> flattened `[lat, lon, ...]`. */
internal fun parsePolylineLatLon(encoded: String): DoubleArray {
    if (encoded.isBlank()) return DoubleArray(0)
    val out = ArrayList<Double>(256)
    for (part in encoded.split(';')) {
        val bits = part.split(',')
        if (bits.size != 2) continue
        val lon = bits[0].toDoubleOrNull() ?: continue
        val lat = bits[1].toDoubleOrNull() ?: continue
        out.add(lat)
        out.add(lon)
    }
    return out.toDoubleArray()
}

/** Which CoMaps route style the planner's profile maps to (0 car, 1 pedestrian, 2 bicycle). */
internal fun drapeRouteType(): Int = MwmProductionPlanner.lastRouteStyle

/**
 * The CoMaps drape as the one and only map surface of the MWM path.
 *
 * - Camera, gestures, rotation, perspective, heading-up and follow mode are the drape's own:
 *   Navi forwards positions ([ComapsDrape.pushLocation]) and asks for a follow mode.
 * - Route (with turn arrows), start / via / end pins, tracks, DATEX, weather and camping are
 *   drawn through the CoMaps route renderer, route points, user marks and tracks. Repeater sites
 *   are intentionally never drawn on the map (product rule); CAT shows them in its sheet only.
 * - Without installed maps the surface shows a placeholder with a download entry.
 */
@OptIn(ExperimentalComposeUiApi::class)
@Composable
internal fun ComapsDrapeMap(
    state: MapRouteState,
    prefer3d: Boolean,
    cameraTiltDeg: Double,
    contoursEnabled: Boolean,
    rotationMode: MapRotationMode,
    navigating: Boolean,
    nightMode: DrapeDayNight,
    breakRemindersEnabled: Boolean,
    weatherPluginEnabled: Boolean,
    weatherMapSymbolsEnabled: Boolean,
    weatherAppActive: Boolean,
    weatherIconsDir: String,
    placeIndexDbPath: String,
    weatherMapEpoch: Int,
    datexHud: DatexHudState,
    datexEpoch: Int,
    bearingEpoch: Int,
    dataDir: java.io.File,
    modifier: Modifier = Modifier,
    onUserPan: () -> Unit = {},
    onMapLongPress: (lat: Double, lon: Double) -> Unit = { _, _ -> },
    onUserRotate: (bearingDeg: Double) -> Unit = {},
    onCameraIdleTarget: (lat: Double, lon: Double, zoom: Double) -> Unit = { _, _, _ -> },
    onOpenMapDownloads: () -> Unit = {},
) {
    val context = LocalContext.current
    val owner = LocalLifecycleOwner.current
    val scope = rememberCoroutineScope()
    var ready by remember { mutableStateOf(ComapsDrape.state == ComapsDrape.State.READY) }
    var note by remember { mutableStateOf<String?>(null) }

    // Bring-up: wait for the headless session (maps installed), then init Platform + Framework
    // once. Keeps polling so maps installed later in this process start the drape live.
    LaunchedEffect(Unit) {
        while (isActive && !ready) {
            when {
                !MwmClientBootstrap.nativeLoaded -> note = "native"
                !ComapsDrape.eligible() -> note = "nomaps"
                else ->
                    when (ComapsDrape.ensureInitialized(context)) {
                        ComapsDrape.State.READY -> {
                            ready = true
                            note = null
                        }
                        ComapsDrape.State.FAILED, ComapsDrape.State.UNAVAILABLE -> {
                            note = "failed"
                            return@LaunchedEffect
                        }
                        else -> note = "starting"
                    }
            }
            delay(500L)
        }
    }

    val backend = ComapsDrape.backend
    if (!ready || backend == null) {
        DrapePlaceholder(
            modifier = modifier,
            note = note,
            detail = ComapsDrape.lastError,
            onOpenMapDownloads = onOpenMapDownloads,
        )
        return
    }

    val view = remember { backend.createMapView(context) }
    val overlays = remember(backend) { DrapeOverlays(backend) }
    val mainHandler = remember { android.os.Handler(android.os.Looper.getMainLooper()) }

    val stateRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference(state)
        }
    stateRef.set(state)
    val rotationRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference(rotationMode)
        }
    rotationRef.set(rotationMode)
    val navigatingRef = remember { AtomicBoolean(navigating) }
    navigatingRef.set(navigating)
    val onUserPanRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference(onUserPan)
        }
    onUserPanRef.set(onUserPan)
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
    val onLongPressRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference(onMapLongPress)
        }
    onLongPressRef.set(onMapLongPress)
    val nightRef =
        remember {
            java.util.concurrent.atomic
                .AtomicReference(nightMode)
        }
    nightRef.set(nightMode)
    // Drape pitch is automatic (zoom dependent, max ~53 deg in navigation). Navi's tilt presets and
    // the 3D switch therefore decide whether the perspective is allowed at all.
    val allowPerspectiveRef = remember { AtomicBoolean(prefer3d || cameraTiltDeg > 0.0) }
    allowPerspectiveRef.set(prefer3d || cameraTiltDeg > 0.0)
    val prefer3dRef = remember { AtomicBoolean(prefer3d) }
    prefer3dRef.set(prefer3d)
    val contoursRef = remember { AtomicBoolean(contoursEnabled) }
    contoursRef.set(contoursEnabled)
    val systemNight =
        (androidx.compose.ui.platform.LocalConfiguration.current.uiMode and android.content.res.Configuration.UI_MODE_NIGHT_MASK) ==
            android.content.res.Configuration.UI_MODE_NIGHT_YES
    val systemNightRef = remember { AtomicBoolean(systemNight) }
    systemNightRef.set(systemNight)

    DisposableEffect(owner) {
        backend.attachLifecycle(owner)
        onDispose { backend.detachLifecycle(owner) }
    }

    // Follow-mode bookkeeping shared by the mode listener and the poll loop.
    val followGraceUntil = remember { longArrayOf(0L) }
    val lastObservedMode = remember { intArrayOf(-1) }

    DisposableEffect(backend) {
        NaviMapTestHooks.mapViewTouch = { event -> view.dispatchTouchEvent(event) }
        backend.setModeListener { mode ->
            mainHandler.post {
                Log.i(TAG, "my-position mode=$mode wantFollow=${stateRef.get().followGps}")
                lastObservedMode[0] = mode
                // NOT_FOLLOW (2) while we asked for a follow mode = the user took the camera.
                if (mode == 2 &&
                    SystemClock.elapsedRealtime() > followGraceUntil[0] &&
                    stateRef.get().followGps &&
                    !NaviMapTestHooks.disableGpsFollow
                ) {
                    onUserPanRef.get().invoke()
                }
            }
        }
        NaviMapTestHooks.drapeBasemapVisible = true
        onDispose {
            NaviMapTestHooks.mapViewTouch = null
            backend.setModeListener(null)
            NaviMapTestHooks.drapeBasemapVisible = false
        }
    }

    // One-time: block tap selection (Navi has no CoMaps place page) and apply initial prefs.
    LaunchedEffect(backend) {
        runCatching { backend.blockTapEvents(true) }
    }

    // Poll loop: prefs, follow mode, navigation camera, compass, camera telemetry.
    LaunchedEffect(backend) {
        var navApplied = false
        var lastIdleSig = ""
        var lastCamChange = 0L
        var lastCamLog = 0L
        var lastNight: Boolean? = null
        while (isActive) {
            runCatching {
                val st = stateRef.get()
                val night =
                    NaviMapTestHooks.drapeNightOverride
                        ?: when (nightRef.get()) {
                            DrapeDayNight.SYSTEM -> systemNightRef.get()
                            DrapeDayNight.AUTO_SUN ->
                                if (st.gpsLat != 0.0 || st.gpsLon != 0.0) {
                                    !backend.isDayAt(st.gpsLat, st.gpsLon, System.currentTimeMillis() / 1000L)
                                } else {
                                    systemNightRef.get()
                                }
                        }
                if (night != lastNight || backend.isNight() != night) {
                    backend.setNight(night)
                    lastNight = night
                }
                backend.setContours(contoursRef.get())
                backend.set3d(allowPerspectiveRef.get(), prefer3dRef.get())

                val wantFollow =
                    when {
                        !st.followGps || NaviMapTestHooks.disableGpsFollow -> ComapsDrape.Follow.FREE
                        rotationRef.get() == MapRotationMode.NorthUp -> ComapsDrape.Follow.NORTH_UP
                        else -> ComapsDrape.Follow.HEADING_UP
                    }
                val hasFix = st.gpsLat != 0.0 || st.gpsLon != 0.0
                val cam = backend.camera()
                if (cam != null) {
                    val actual =
                        when (cam.mode) {
                            3 -> ComapsDrape.Follow.NORTH_UP
                            4 -> ComapsDrape.Follow.HEADING_UP
                            else -> ComapsDrape.Follow.FREE
                        }
                    val now = SystemClock.elapsedRealtime()
                    if (wantFollow != actual && now > followGraceUntil[0] && (wantFollow == ComapsDrape.Follow.FREE || hasFix)) {
                        followGraceUntil[0] = now + FOLLOW_CMD_GRACE_MS
                        Log.i(TAG, "set follow mode want=$wantFollow actual=$actual hasFix=$hasFix")
                        backend.setFollowMode(wantFollow)
                    }
                    val wantNav = navigatingRef.get() && wantFollow != ComapsDrape.Follow.FREE
                    if (wantNav != navApplied) {
                        navApplied = wantNav
                        Log.i(TAG, "navigation camera on=$wantNav")
                        backend.setNavigation(wantNav, MwmProductionPlanner.lastRouteStyle == 1)
                    }
                    if (rotationRef.get() == MapRotationMode.Compass) {
                        NaviMapTestHooks.magneticHeadingDeg?.let { backend.pushCompass(it) }
                    }
                    if (now - lastCamLog > 5_000L) {
                        lastCamLog = now
                        Log.i(
                            TAG,
                            "camera lat=%.5f lon=%.5f level=%.2f bearing=%.1f pitch=%.1f mode=%d view=%.0fx%.0f gps=%.5f,%.5f".format(
                                cam.lat,
                                cam.lon,
                                cam.level,
                                cam.bearingDeg,
                                cam.pitchDeg,
                                cam.mode,
                                cam.widthPx,
                                cam.heightPx,
                                st.gpsLat,
                                st.gpsLon,
                            ),
                        )
                    }
                    // Telemetry for tests / HUD.
                    NaviMapTestHooks.lastCameraLat = cam.lat
                    NaviMapTestHooks.lastCameraLon = cam.lon
                    NaviMapTestHooks.lastCameraZoom = cam.level
                    NaviMapTestHooks.lastCameraBearing = cam.bearingDeg
                    NaviMapTestHooks.lastCameraPitch = cam.pitchDeg
                    NaviMapTestHooks.drapeMode = cam.mode
                    // Camera idle: report a settled free camera / an observed zoom change.
                    val sig = "%.5f,%.5f,%.2f,%.1f".format(cam.lat, cam.lon, cam.level, cam.bearingDeg)
                    if (sig != lastIdleSig) {
                        lastIdleSig = sig
                        lastCamChange = now
                    } else if (lastCamChange != 0L && now - lastCamChange > 400L) {
                        lastCamChange = 0L
                        onCameraIdleRef.get().invoke(cam.lat, cam.lon, cam.level)
                        if (actual == ComapsDrape.Follow.FREE && abs(cam.bearingDeg - st.cameraBearing) > 0.5) {
                            onUserRotateRef.get().invoke(cam.bearingDeg)
                        }
                    }
                }
            }.onFailure { Log.w(TAG, "poll failed", it) }
            delay(POLL_MS)
        }
    }

    // Instrumented-test hooks: "layer count" / render-settle requests. The drape renders
    // continuously; a requested settle is answered once a few frames have been presented.
    LaunchedEffect(backend) {
        NaviMapTestHooks.lastReportedLayerCount = 1
        while (isActive) {
            delay(100)
            val req = NaviMapTestHooks.renderSettleRequestId
            if (req > NaviMapTestHooks.lastRenderSettleId) {
                delay(1_200)
                NaviMapTestHooks.lastRenderSettleId = req
            }
        }
    }

    // Zoom requested by the HUD (+/-, auto zoom). Following: relative scale keeps the follow
    // mode; free: absolute level at the current centre.
    LaunchedEffect(state.cameraZoom, ready) {
        val z = state.cameraZoom ?: return@LaunchedEffect
        val cam = backend.camera() ?: return@LaunchedEffect
        if (abs(cam.level - z) < 0.05) return@LaunchedEffect
        if (cam.mode == 3 || cam.mode == 4) {
            backend.scaleBy(2.0.pow(z - cam.level), false)
        } else {
            val lat = state.cameraLat ?: cam.lat
            val lon = state.cameraLon ?: cam.lon
            backend.setCamera(lat, lon, z, ComapsDrape.NO_BEARING, false)
        }
        NaviMapTestHooks.lastCameraZoom = z
    }

    // Free camera target (search result, recenter on a marked place, tests).
    LaunchedEffect(state.cameraLat, state.cameraLon, state.followGps, ready) {
        if (state.followGps && !NaviMapTestHooks.disableGpsFollow) return@LaunchedEffect
        val lat = state.cameraLat ?: return@LaunchedEffect
        val lon = state.cameraLon ?: return@LaunchedEffect
        val cam = backend.camera()
        val z = state.cameraZoom ?: -1.0
        if (cam != null &&
            abs(cam.lat - lat) < 1e-6 &&
            abs(cam.lon - lon) < 1e-6 &&
            (z < 0 || abs(cam.level - z) < 0.02)
        ) {
            return@LaunchedEffect
        }
        backend.setCamera(lat, lon, z, ComapsDrape.NO_BEARING, false)
    }

    // Bearing requested by Navi (rotation mode change, snap-back, tests).
    LaunchedEffect(state.cameraBearing, bearingEpoch, ready) {
        if (!NaviMapTestHooks.applyBearingToMap) return@LaunchedEffect
        val cam = backend.camera() ?: return@LaunchedEffect
        if (cam.mode == 4) return@LaunchedEffect // heading-up owns the rotation
        if (abs(angleDiff(cam.bearingDeg, state.cameraBearing)) < 0.5) return@LaunchedEffect
        backend.rotateTo(state.cameraBearing, false)
    }

    // Route overview when a new route arrives without an explicit camera.
    LaunchedEffect(state.polyline, ready) {
        if (state.polyline.isBlank() || state.cameraLat != null) return@LaunchedEffect
        val pts = parsePolylineLatLon(state.polyline)
        if (pts.size < 4) return@LaunchedEffect
        var minLat = Double.MAX_VALUE
        var maxLat = -Double.MAX_VALUE
        var minLon = Double.MAX_VALUE
        var maxLon = -Double.MAX_VALUE
        for (i in pts.indices step 2) {
            minLat = minOf(minLat, pts[i])
            maxLat = maxOf(maxLat, pts[i])
            minLon = minOf(minLon, pts[i + 1])
            maxLon = maxOf(maxLon, pts[i + 1])
        }
        if (state.poiLat != 0.0 || state.poiLon != 0.0) {
            minLat = minOf(minLat, state.poiLat)
            maxLat = maxOf(maxLat, state.poiLat)
            minLon = minOf(minLon, state.poiLon)
            maxLon = maxOf(maxLon, state.poiLon)
        }
        backend.fitRect(minLat, minLon, maxLat, maxLon, false)
    }

    // Route line (CoMaps route renderer: casing, direction arrows at turns, pattern per profile).
    // Not keyed on layerEpoch: that counter moves with every GPS fix, and clearing and re-adding the
    // route at fix rate made the line flicker or vanish while driving.
    LaunchedEffect(state.polyline, state.routeSegmentsJson, ready) {
        backend.clearRoute()
        val (onPoly, offPoly) = splitRouteSegmentPolylines(state.routeSegmentsJson)
        val onPts =
            parsePolylineLatLon(
                if (onPoly.isNotBlank()) {
                    onPoly
                } else if (offPoly.isBlank()) {
                    state.polyline
                } else {
                    ""
                },
            )
        val offPts = parsePolylineLatLon(offPoly)
        val type = drapeRouteType()
        if (onPts.size >= 4) {
            val turns = MwmProductionPlanner.lastTurnPointIndices(state.polyline)
            backend.addSubroute(onPts, type, turns)
        }
        // Off-trail legs (hiking) are drawn dashed like a pedestrian route.
        if (offPts.size >= 4) backend.addSubroute(offPts, 1, null)
    }

    // Start / via / end pins as CoMaps route points.
    LaunchedEffect(
        state.startLat,
        state.startLon,
        state.startName,
        state.viaLat,
        state.viaLon,
        state.viaName,
        state.viaPoints,
        state.endLat,
        state.endLon,
        state.endName,
        ready,
    ) {
        val lat = ArrayList<Double>()
        val names = ArrayList<String>()

        fun add(
            name: String,
            la: Double,
            lo: Double,
        ) {
            if (la == 0.0 && lo == 0.0) return
            lat.add(la)
            lat.add(lo)
            names.add(name)
        }
        add(state.startName, state.startLat, state.startLon)
        if (state.viaPoints.isNotEmpty()) {
            state.viaPoints.forEach { add(it.name, it.lat, it.lon) }
        } else {
            add(state.viaName, state.viaLat, state.viaLon)
        }
        add(state.endName, state.endLat, state.endLon)
        // A single point is not a route; drape draws route points only as start..finish.
        if (names.size < 2) {
            backend.setRoutePoints(DoubleArray(0), emptyArray())
        } else {
            backend.setRoutePoints(lat.toDoubleArray(), names.toTypedArray())
        }
    }

    // Imported GPX: the lines and waypoints as CoMaps tracks / marks; fit the camera when a file arrives.
    val gpx = GpxImport.ui
    LaunchedEffect(gpx.doc, gpx.selected, ready) {
        if (!ready) return@LaunchedEffect
        overlays.lines("navi-gpx", GpxImport.drapeLines(gpx))
        overlays.marks("navi-gpx-wpt", GpxImport.drapeMarks(gpx))
    }
    LaunchedEffect(gpx.fitEpoch, ready) {
        if (!ready || gpx.fitEpoch == 0) return@LaunchedEffect
        val doc = gpx.doc ?: return@LaunchedEffect
        val line = doc.lines.getOrNull(gpx.selected)
        val b = doc.bounds
        var minLat = b?.get(0) ?: 0.0
        var minLon = b?.get(1) ?: 0.0
        var maxLat = b?.get(2) ?: 0.0
        var maxLon = b?.get(3) ?: 0.0
        if (line != null) {
            minLat = 90.0
            minLon = 180.0
            maxLat = -90.0
            maxLon = -180.0
            for (i in 0 until line.pointCount) {
                val la = line.latlon[2 * i]
                val lo = line.latlon[2 * i + 1]
                if (la < minLat) minLat = la
                if (la > maxLat) maxLat = la
                if (lo < minLon) minLon = lo
                if (lo > maxLon) maxLon = lo
            }
        }
        if (maxLat <= minLat && maxLon <= minLon) return@LaunchedEffect
        backend.fitRect(minLat, minLon, maxLat, maxLon, true)
        NaviMapTestHooks.lastGpxFit = doubleArrayOf(minLat, minLon, maxLat, maxLon)
    }

    // Position puck: amber when estimated (IMU dead reckoning), grey when the position is lost.
    LaunchedEffect(state.positionLost, state.gpsEstimated, ready) {
        if (!ready) return@LaunchedEffect
        val puck = puckState(state.positionLost, state.gpsEstimated)
        backend.setPuckState(puck)
        NaviMapTestHooks.lastPuckState = puck
    }

    // Marks: the long-press POI, break stops, camping suggestions, tracks.
    LaunchedEffect(
        state.poiLat,
        state.poiLon,
        state.poiName,
        state.breakPois,
        state.campingPins,
        state.tracks,
        breakRemindersEnabled,
        ready,
    ) {
        val poi = ArrayList<DrapeMark>()
        if (state.poiLat != 0.0 || state.poiLon != 0.0) {
            poi.add(DrapeMark(state.poiLat, state.poiLon, state.poiName, DrapeIcon.NONE, DrapeColor.RED))
        }
        overlays.marks("navi-poi", poi)
        val breaks = ArrayList<DrapeMark>()
        if (breakRemindersEnabled) {
            for (b in state.breakPois) {
                if (b.name.contains("Ramsh", ignoreCase = true)) continue
                val icon = if (b.kind == "tent") DrapeIcon.PARK else DrapeIcon.HOTEL
                breaks.add(DrapeMark(b.lat, b.lon, b.name, icon, DrapeColor.BROWN))
            }
        }
        overlays.marks("navi-breaks", breaks)
        overlays.marks(
            "navi-camping",
            state.campingPins.map { (la, lo) -> DrapeMark(la, lo, "", DrapeIcon.PARK, DrapeColor.PURPLE) },
        )
        overlays.marks(
            "navi-tracks",
            state.tracks.map { DrapeMark(it.lat, it.lon, it.label, DrapeIcon.TRANSPORT, DrapeColor.YELLOW, aprsSymbol(it.symbolKey)) },
        )
        NaviMapTestHooks.lastTrackFeatureCount = state.tracks.size
        NaviMapTestHooks.lastTrackOverlayCount = state.tracks.size
    }

    // DATEX situations as drape bookmark pins (colored pin + road label; phase6b-proven).
    // Do NOT draw DATEX geometry as drape tracks: SolidLineBuilder CapVertex can throw
    // std::length_error on degenerate synth polylines and SIGABRT the process mid-sim.
    LaunchedEffect(datexEpoch, datexHud, ready) {
        val marks = ArrayList<DrapeMark>()
        if (datexHud.overlayEnabled && datexHud.activeCount > 0) {
            val arr = runCatching { JSONArray(datexHud.activeJson) }.getOrNull() ?: JSONArray()
            for (i in 0 until arr.length()) {
                val o = arr.optJSONObject(i) ?: continue
                val label = o.optString("road_number").ifBlank { o.optString("kind") }
                val geom = o.optJSONArray("geometry")
                var lat = o.optDouble("lat", Double.NaN)
                var lon = o.optDouble("lon", Double.NaN)
                if ((!lat.isFinite() || !lon.isFinite()) && geom != null && geom.length() > 0) {
                    val p = geom.optJSONArray(0)
                    if (p != null) {
                        lat = p.optDouble(0)
                        lon = p.optDouble(1)
                    }
                }
                if (lat.isFinite() && lon.isFinite()) {
                    // BookmarkIcon::None still draws the coloured pin + name (see phase6b
                    // r_overlay_datex_z14). INFORMATION can miss the atlas on some builds.
                    marks.add(
                        DrapeMark(
                            lat,
                            lon,
                            label.ifBlank { "DATEX" },
                            DrapeIcon.NONE,
                            DrapeColor.DEEP_ORANGE,
                        ),
                    )
                }
            }
        }
        overlays.marks("navi-datex", marks)
        overlays.lines("navi-datex-lines", emptyList())
        NaviMapTestHooks.lastDatexDrapeMarkCount = marks.size
        if (marks.isNotEmpty()) {
            Log.i(TAG, "datex drape marks=${marks.size}")
        }
    }

    // Weather symbols for the visible area (plugin + toggle gated, same Rust declutter as before).
    LaunchedEffect(
        weatherMapEpoch,
        weatherPluginEnabled,
        weatherMapSymbolsEnabled,
        weatherAppActive,
        ready,
    ) {
        while (isActive) {
            if (!weatherPluginEnabled || !weatherMapSymbolsEnabled) {
                overlays.marks("navi-weather", emptyList())
            } else {
                refreshWeatherDrape(
                    backend,
                    overlays,
                    dataDir.absolutePath,
                    placeIndexDbPath,
                    weatherPluginEnabled,
                    weatherMapSymbolsEnabled,
                    weatherAppActive,
                )
            }
            delay(15_000L)
        }
    }

    // Compose DATEX circles: guaranteed visible even if bookmark atlas pins are decluttered.
    var datexScreenMarks by remember { mutableStateOf<List<Pair<Offset, Boolean>>>(emptyList()) }
    LaunchedEffect(datexEpoch, datexHud, ready, state.cameraLat, state.cameraLon, state.cameraZoom) {
        while (isActive) {
            if (!ready || !datexHud.overlayEnabled || datexHud.activeCount <= 0) {
                datexScreenMarks = emptyList()
                NaviMapTestHooks.lastDatexComposeMarkCount = 0
            } else {
                val arr = runCatching { JSONArray(datexHud.activeJson) }.getOrNull() ?: JSONArray()
                val projected = ArrayList<Pair<Offset, Boolean>>(arr.length())
                for (i in 0 until arr.length()) {
                    val o = arr.optJSONObject(i) ?: continue
                    var lat = o.optDouble("lat", Double.NaN)
                    var lon = o.optDouble("lon", Double.NaN)
                    if ((!lat.isFinite() || !lon.isFinite())) {
                        val geom = o.optJSONArray("geometry")
                        val p = geom?.optJSONArray(0)
                        if (p != null) {
                            lat = p.optDouble(0)
                            lon = p.optDouble(1)
                        }
                    }
                    if (!lat.isFinite() || !lon.isFinite()) continue
                    val screen = latLonToScreenApprox(backend, lat, lon) ?: continue
                    val block =
                        o.optString("impact").equals("block", ignoreCase = true) ||
                            o.optString("kind").contains("block", ignoreCase = true)
                    projected.add(screen to block)
                }
                datexScreenMarks = projected
                NaviMapTestHooks.lastDatexComposeMarkCount = projected.size
            }
            delay(POLL_MS)
        }
    }

    // Surface + gesture overlay. The transparent Canvas forwards touches to the drape and adds
    // Navi's 4 s stationary hold (map-mark menu).
    var holdJob by remember { mutableStateOf<Job?>(null) }
    var holdProgress by remember { mutableFloatStateOf(0f) }
    var holdScreenX by remember { mutableFloatStateOf(0f) }
    var holdScreenY by remember { mutableFloatStateOf(0f) }
    var holdActive by remember { mutableStateOf(false) }

    Box(modifier = modifier) {
        AndroidView(
            factory = {
                // One process-wide surface: detach from a previous composition's parent.
                (view.parent as? ViewGroup)?.removeView(view)
                view.visibility = View.VISIBLE
                view
            },
            modifier = Modifier.fillMaxSize(),
        )
        Canvas(
            modifier =
                Modifier
                    .fillMaxSize()
                    .pointerInteropFilter { event ->
                        when (event.actionMasked) {
                            MotionEvent.ACTION_DOWN -> {
                                holdJob?.cancel()
                                holdActive = true
                                holdScreenX = event.x
                                holdScreenY = event.y
                                holdProgress = 0f
                                val sx = event.x
                                val sy = event.y
                                holdJob =
                                    scope.launch {
                                        val startMs = SystemClock.uptimeMillis()
                                        while (isActive) {
                                            val elapsed = SystemClock.uptimeMillis() - startMs
                                            holdProgress =
                                                (elapsed.toFloat() / MAP_LONG_PRESS_HOLD_MS.toFloat()).coerceIn(0f, 1f)
                                            if (elapsed >= MAP_LONG_PRESS_HOLD_MS) {
                                                backend.screenToLatLon(sx, sy)?.let { (la, lo) ->
                                                    onLongPressRef.get().invoke(la, lo)
                                                }
                                                val now = SystemClock.uptimeMillis()
                                                val cancel = MotionEvent.obtain(now, now, MotionEvent.ACTION_CANCEL, sx, sy, 0)
                                                view.dispatchTouchEvent(cancel)
                                                cancel.recycle()
                                                holdActive = false
                                                holdProgress = 0f
                                                holdJob = null
                                                break
                                            }
                                            delay(16)
                                        }
                                    }
                                view.dispatchTouchEvent(event)
                                true
                            }
                            MotionEvent.ACTION_POINTER_DOWN, MotionEvent.ACTION_POINTER_UP -> {
                                holdJob?.cancel()
                                holdJob = null
                                holdActive = false
                                holdProgress = 0f
                                view.dispatchTouchEvent(event)
                                true
                            }
                            MotionEvent.ACTION_MOVE -> {
                                if (holdActive && holdJob != null) {
                                    val dist = hypot((event.x - holdScreenX).toDouble(), (event.y - holdScreenY).toDouble()).toFloat()
                                    if (dist > MAP_LONG_PRESS_MOVE_SLOP_PX) {
                                        holdJob?.cancel()
                                        holdJob = null
                                        holdActive = false
                                        holdProgress = 0f
                                        view.dispatchTouchEvent(event)
                                    }
                                    true
                                } else {
                                    view.dispatchTouchEvent(event)
                                    true
                                }
                            }
                            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                                holdJob?.cancel()
                                holdJob = null
                                holdActive = false
                                holdProgress = 0f
                                view.dispatchTouchEvent(event)
                                true
                            }
                            else -> {
                                view.dispatchTouchEvent(event)
                                true
                            }
                        }
                    },
        ) {
            for ((pt, block) in datexScreenMarks) {
                val fill = if (block) Color(0xFFC62828) else Color(0xFFE65100)
                // Large high-contrast discs so emulator screenshots prove DATEX render.
                drawCircle(Color(0xAA000000), radius = 34f, center = pt)
                drawCircle(Color.White, radius = 28f, center = pt)
                drawCircle(fill, radius = 22f, center = pt)
                drawCircle(Color.White, radius = 7f, center = pt)
            }
            if (holdProgress > 0f) {
                val center = Offset(holdScreenX, holdScreenY)
                val radius = 42f
                drawCircle(Color(0x55000000), radius = radius + 8f, center = center)
                drawCircle(Color(0x88FFFFFF), radius = radius, center = center, style = Stroke(width = 6f))
                drawArc(
                    color = Color(0xFF1565C0),
                    startAngle = -90f,
                    sweepAngle = 360f * holdProgress.coerceIn(0f, 1f),
                    useCenter = false,
                    topLeft = Offset(center.x - radius, center.y - radius),
                    size =
                        androidx.compose.ui.geometry
                            .Size(radius * 2f, radius * 2f),
                    style = Stroke(width = 8f, cap = androidx.compose.ui.graphics.StrokeCap.Round),
                )
                drawCircle(Color(0xFF1565C0), radius = 8f, center = center)
            }
        }
    }
}

/** Project WGS84 onto the drape view via iterative screenToLatLon Jacobian steps. */
internal fun latLonToScreenApprox(
    backend: ComapsDrape.Backend,
    lat: Double,
    lon: Double,
): Offset? {
    val cam = backend.camera() ?: return null
    val w = cam.widthPx.toFloat()
    val h = cam.heightPx.toFloat()
    if (w < 2f || h < 2f) return null
    var x = w * 0.5f
    var y = h * 0.5f
    repeat(10) {
        val here = backend.screenToLatLon(x, y) ?: return null
        val right = backend.screenToLatLon(x + 8f, y) ?: return null
        val down = backend.screenToLatLon(x, y + 8f) ?: return null
        val dLonDx = (right.second - here.second) / 8.0
        val dLatDx = (right.first - here.first) / 8.0
        val dLonDy = (down.second - here.second) / 8.0
        val dLatDy = (down.first - here.first) / 8.0
        val det = dLonDx * dLatDy - dLatDx * dLonDy
        if (abs(det) < 1e-18) return Offset(x, y)
        val dLon = lon - here.second
        val dLat = lat - here.first
        x = (x + ((dLon * dLatDy - dLat * dLonDy) / det)).toFloat()
        y = (y + ((dLat * dLonDx - dLon * dLatDx) / det)).toFloat()
        if (x !in -w..2f * w || y !in -h..2f * h) return null
    }
    if (x !in 0f..w || y !in 0f..h) return null
    return Offset(x, y)
}

private fun angleDiff(
    a: Double,
    b: Double,
): Double {
    var d = (a - b) % 360.0
    if (d > 180.0) d -= 360.0
    if (d < -180.0) d += 360.0
    return d
}

/** Shown while the drape cannot run: no maps yet, engine starting, or a hard failure. */
@Composable
private fun DrapePlaceholder(
    modifier: Modifier,
    note: String?,
    detail: String?,
    onOpenMapDownloads: () -> Unit,
) {
    Box(modifier = modifier.testTag("drape_placeholder"), contentAlignment = Alignment.Center) {
        Column(
            modifier = Modifier.padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(
                text =
                    when (note) {
                        "failed" -> stringResource(R.string.drape_map_failed)
                        "starting" -> stringResource(R.string.drape_map_starting)
                        else -> stringResource(R.string.drape_no_maps_title)
                    },
                style = MaterialTheme.typography.titleMedium,
            )
            if (note == "nomaps" || note == null || note == "native") {
                Text(
                    text = stringResource(R.string.drape_no_maps_body),
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.padding(top = 8.dp),
                )
                Button(onClick = onOpenMapDownloads, modifier = Modifier.padding(top = 12.dp)) {
                    Text(stringResource(R.string.drape_open_map_downloads))
                }
            } else if (!detail.isNullOrBlank()) {
                Text(text = detail, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(top = 8.dp))
            }
        }
    }
}

private suspend fun refreshWeatherDrape(
    backend: ComapsDrape.Backend,
    overlays: DrapeOverlays,
    dataDir: String,
    placeIndexDb: String,
    pluginEnabled: Boolean,
    symbolsEnabled: Boolean,
    appActive: Boolean,
) {
    val cam = backend.camera() ?: return
    if (cam.level > weatherMapZoomMax()) {
        overlays.marks("navi-weather", emptyList())
        return
    }
    val tl = backend.screenToLatLon(0f, 0f)
    val br = backend.screenToLatLon(cam.widthPx.toFloat(), cam.heightPx.toFloat())
    if (tl == null || br == null) return
    val minLat = minOf(tl.first, br.first)
    val maxLat = maxOf(tl.first, br.first)
    val minLon = minOf(tl.second, br.second)
    val maxLon = maxOf(tl.second, br.second)
    val raw =
        withContext(Dispatchers.IO) {
            runCatching {
                weatherMapSymbolsJson(
                    dataDir = dataDir,
                    indexDbPath = placeIndexDb,
                    minLat = minLat,
                    minLon = minLon,
                    maxLat = maxLat,
                    maxLon = maxLon,
                    zoom = cam.level,
                    weatherPluginEnabled = pluginEnabled,
                    mapSymbolsEnabled = symbolsEnabled,
                    appActive = appActive,
                )
            }.getOrDefault("{}")
        }
    val arr = runCatching { org.json.JSONObject(raw).optJSONArray("symbols") }.getOrNull() ?: JSONArray()
    val marks = ArrayList<DrapeMark>(arr.length())
    for (i in 0 until arr.length()) {
        val s = arr.optJSONObject(i) ?: continue
        val slug = s.optString("icon_slug", "not-available")
        val label = "${s.optString("name")} ${slug.replace('-', ' ')}".trim()
        val color =
            when {
                s.optBoolean("stale", false) -> DrapeColor.GRAY
                slug.contains("clear") || slug.contains("sun") -> DrapeColor.YELLOW
                slug.contains("rain") || slug.contains("drizzle") || slug.contains("snow") -> DrapeColor.LIGHT_BLUE
                slug.contains("thunder") -> DrapeColor.ORANGE
                else -> DrapeColor.CYAN
            }
        marks.add(DrapeMark(s.optDouble("lat"), s.optDouble("lon"), label, DrapeIcon.SIGHTS, color))
    }
    overlays.marks("navi-weather", marks)
}
