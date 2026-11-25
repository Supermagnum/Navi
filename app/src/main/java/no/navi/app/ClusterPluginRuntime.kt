package no.navi.app

import android.content.Context
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.LifecycleOwner
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.navi.ClusterCallKind
import uniffi.navi.ClusterEndpointsFfi
import uniffi.navi.ClusterSinkKindFfi
import uniffi.navi.clusterAssembleGuidanceJson
import uniffi.navi.clusterPluginConfigure
import uniffi.navi.clusterPluginDrainUsbNdjson
import uniffi.navi.clusterPluginGetEndpoints
import uniffi.navi.clusterPluginGetSinkKind
import uniffi.navi.clusterPluginInstallGuest
import uniffi.navi.clusterPluginIsEnabled
import uniffi.navi.clusterPluginSetEnabled
import uniffi.navi.clusterPluginSetEndpoints
import uniffi.navi.clusterPluginSetGuidanceJson
import uniffi.navi.clusterPluginSetSinkKind
import uniffi.navi.clusterPluginTick
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/** Map Navit-style approach icon keys to cluster `maneuver.type` wire ids. */
fun clusterManeuverTypeFromIcon(iconKey: String?): String? {
    if (iconKey.isNullOrBlank()) return null
    return when {
        iconKey.contains("turnaround") || iconKey.contains("uturn") -> "u_turn"
        iconKey.contains("roundabout") -> "roundabout"
        iconKey.contains("destination") -> "destination"
        iconKey.contains("keep_left") -> "keep_left"
        iconKey.contains("keep_right") -> "keep_right"
        iconKey.contains("exit_left") -> "exit_left"
        iconKey.contains("exit_right") -> "exit_right"
        iconKey.contains("merge_left") -> "merge_left"
        iconKey.contains("merge_right") -> "merge_right"
        iconKey.contains("left") -> "turn_left"
        iconKey.contains("right") -> "turn_right"
        iconKey.contains("straight") -> "straight"
        else -> iconKey
    }
}

/** Latest HUD pieces for cluster export (filled by the map host each fix). */
data class ClusterHudSnapshot(
    val routeActive: Boolean = false,
    val lat: Double? = null,
    val lon: Double? = null,
    val headingDeg: Double? = null,
    val speedKmh: Double? = null,
    val hAccM: Double? = null,
    val speedLimitKmh: Double? = null,
    val overspeed: Boolean = false,
    val maneuverType: String? = null,
    val maneuverIconKey: String? = null,
    val maneuverDistanceM: Double? = null,
    val maneuverStreetName: String? = null,
    val ecoEnabled: Boolean = false,
    val ecoProfile: String? = null,
    val tripEta: String? = null,
    val tripRemainingDurationS: Double? = null,
    val tripRemainingDistanceM: Double? = null,
    val restActive: Boolean = false,
    val restMinutesToBreak: Double? = null,
    val restDistanceToBreakM: Double? = null,
    val attitudeYawDeg: Double? = null,
    val attitudeRollDeg: Double? = null,
    val attitudePitchDeg: Double? = null,
    val attitudeHeadingDeg: Double? = null,
    val imuCalibrated: Boolean? = null,
    val routePolyline: String = "",
    val roadSignWarningJson: String = "{}",
    val childrenWarningJson: String = "{}",
    val cameraWarningJson: String = "{}",
    val camerasOptedIn: Boolean = false,
)

data class ClusterPluginUiState(
    val enabled: Boolean = false,
    val sinkKind: ClusterSinkKindFfi = ClusterSinkKindFfi.LOG,
    val jsonUdpHost: String = "127.0.0.1",
    val jsonUdpPort: Int = 8765,
    val kuksaHost: String = "127.0.0.1",
    val kuksaPort: Int = 55556,
    val message: String? = null,
    val lastPublishPreview: String? = null,
    val usbStatus: String? = null,
    val initialized: Boolean = false,
)

/**
 * Process-wide instrument-cluster session: installs the staged wasm guest and
 * ticks it while enabled and the app is in the foreground.
 */
object ClusterPluginRuntime {
    private const val PLUGIN_NAME = "instrument-cluster"
    private const val TICK_MS = 400L

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val initStarted = AtomicBoolean(false)
    private val lock = Any()
    private var loop: Job? = null
    private var startedActivities = 0
    private var foreground = false

    private val hud = AtomicReference(ClusterHudSnapshot())
    private val _ui = MutableStateFlow(ClusterPluginUiState())
    val ui: StateFlow<ClusterPluginUiState> = _ui.asStateFlow()

    fun attach(
        context: Context,
        owner: LifecycleOwner,
    ) {
        val app = context.applicationContext
        ClusterUsbExporter.attach(app)
        owner.lifecycle.addObserver(
            LifecycleEventObserver { _, event ->
                when (event) {
                    Lifecycle.Event.ON_START -> setStarted(true)
                    Lifecycle.Event.ON_STOP -> setStarted(false)
                    else -> Unit
                }
            },
        )
        scope.launch { ensureInitialized(app) }
    }

    fun ensureInitialized(context: Context) {
        if (!initStarted.compareAndSet(false, true)) return
        try {
            val status = clusterPluginConfigure(context.filesDir.absolutePath)
            check(status == "OK") { "cluster configure: $status" }
            installGuestFromAssets(context)
            val enabled = MapHudPrefs.loadClusterPluginEnabled(context)
            clusterPluginSetEnabled(enabled)
            val sink = MapHudPrefs.loadClusterSinkKind(context)
            clusterPluginSetSinkKind(sink)
            val endpoints = MapHudPrefs.loadClusterEndpoints(context)
            clusterPluginSetEndpoints(
                endpoints.jsonUdpHost,
                endpoints.jsonUdpPort.toUInt(),
                endpoints.kuksaHost,
                endpoints.kuksaPort.toUInt(),
            )
            refreshUiFromNative()
            reconcile()
        } catch (e: Exception) {
            initFailed(e)
        } catch (e: LinkageError) {
            initFailed(e)
        }
    }

    private fun initFailed(e: Throwable) {
        initStarted.set(false)
        _ui.value =
            _ui.value.copy(
                initialized = false,
                message = "Instrument cluster unavailable: ${e.message}",
            )
    }

    fun publishHud(snapshot: ClusterHudSnapshot) {
        hud.set(snapshot)
    }

    fun setEnabled(
        context: Context,
        enabled: Boolean,
    ): String {
        MapHudPrefs.saveClusterPluginEnabled(context, enabled)
        val result = clusterPluginSetEnabled(enabled)
        refreshUiFromNative(message = result)
        reconcile()
        return result
    }

    fun setSinkKind(
        context: Context,
        kind: ClusterSinkKindFfi,
    ): String {
        MapHudPrefs.saveClusterSinkKind(context, kind)
        val result = clusterPluginSetSinkKind(kind)
        refreshUiFromNative(message = result)
        return result
    }

    fun setEndpoints(
        context: Context,
        jsonUdpHost: String,
        jsonUdpPort: Int,
        kuksaHost: String,
        kuksaPort: Int,
    ): String {
        MapHudPrefs.saveClusterEndpoints(
            context,
            ClusterEndpointsFfi(
                jsonUdpHost = jsonUdpHost,
                jsonUdpPort = jsonUdpPort.toUInt(),
                kuksaHost = kuksaHost,
                kuksaPort = kuksaPort.toUInt(),
            ),
        )
        val result =
            clusterPluginSetEndpoints(
                jsonUdpHost,
                jsonUdpPort.toUInt(),
                kuksaHost,
                kuksaPort.toUInt(),
            )
        refreshUiFromNative(message = result)
        return result
    }

    private fun refreshUiFromNative(message: String? = _ui.value.message) {
        val ep = clusterPluginGetEndpoints()
        _ui.value =
            ClusterPluginUiState(
                enabled = clusterPluginIsEnabled(),
                sinkKind = clusterPluginGetSinkKind(),
                jsonUdpHost = ep.jsonUdpHost,
                jsonUdpPort = ep.jsonUdpPort.toInt(),
                kuksaHost = ep.kuksaHost,
                kuksaPort = ep.kuksaPort.toInt(),
                message = message,
                lastPublishPreview = _ui.value.lastPublishPreview,
                usbStatus = ClusterUsbExporter.statusLine(),
                initialized = true,
            )
    }

    private fun setStarted(started: Boolean) {
        synchronized(lock) {
            startedActivities = (startedActivities + if (started) 1 else -1).coerceAtLeast(0)
            foreground = startedActivities > 0
        }
        reconcile()
    }

    private fun shouldRun(): Boolean = _ui.value.initialized && _ui.value.enabled && foreground

    private fun reconcile() {
        synchronized(lock) {
            val running = loop?.isActive == true
            if (shouldRun()) {
                if (!running) loop = scope.launch { runLoop() }
            } else if (running) {
                loop?.cancel()
                loop = null
            }
        }
    }

    private suspend fun runLoop() {
        while (scope.isActive && shouldRun()) {
            val snap = hud.get()
            val guidance =
                clusterAssembleGuidanceJson(
                    routeActive = snap.routeActive,
                    lat = snap.lat,
                    lon = snap.lon,
                    headingDeg = snap.headingDeg,
                    speedKmh = snap.speedKmh,
                    hAccM = snap.hAccM,
                    speedLimitKmh = snap.speedLimitKmh,
                    overspeed = snap.overspeed,
                    maneuverType = snap.maneuverType,
                    maneuverIconKey = snap.maneuverIconKey,
                    maneuverDistanceM = snap.maneuverDistanceM,
                    maneuverStreetName = snap.maneuverStreetName,
                    ecoEnabled = snap.ecoEnabled,
                    ecoProfile = snap.ecoProfile,
                    tripEta = snap.tripEta,
                    tripRemainingDurationS = snap.tripRemainingDurationS,
                    tripRemainingDistanceM = snap.tripRemainingDistanceM,
                    restActive = snap.restActive,
                    restMinutesToBreak = snap.restMinutesToBreak,
                    restDistanceToBreakM = snap.restDistanceToBreakM,
                    attitudeYawDeg = snap.attitudeYawDeg,
                    attitudeRollDeg = snap.attitudeRollDeg,
                    attitudePitchDeg = snap.attitudePitchDeg,
                    attitudeHeadingDeg = snap.attitudeHeadingDeg,
                    imuCalibrated = snap.imuCalibrated,
                    routePolyline = snap.routePolyline,
                    roadSignWarningJson = snap.roadSignWarningJson,
                    childrenWarningJson = snap.childrenWarningJson,
                    cameraWarningJson = snap.cameraWarningJson,
                    camerasOptedIn = snap.camerasOptedIn,
                    ts = null,
                )
            clusterPluginSetGuidanceJson(guidance)
            val tick = clusterPluginTick()
            val preview = tick.lastPublishJson?.take(120)
            val transportMsg = tick.sinkError
            val msg =
                when {
                    tick.kind != ClusterCallKind.OK -> tick.message
                    transportMsg != null -> transportMsg
                    else -> null
                }
            if (clusterPluginGetSinkKind() == ClusterSinkKindFfi.USB_NDJSON) {
                val lines = clusterPluginDrainUsbNdjson()
                ClusterUsbExporter.writeLines(lines)
            }
            val ep = clusterPluginGetEndpoints()
            _ui.value =
                _ui.value.copy(
                    message = msg,
                    lastPublishPreview = preview,
                    enabled = clusterPluginIsEnabled(),
                    sinkKind = clusterPluginGetSinkKind(),
                    jsonUdpHost = ep.jsonUdpHost,
                    jsonUdpPort = ep.jsonUdpPort.toInt(),
                    kuksaHost = ep.kuksaHost,
                    kuksaPort = ep.kuksaPort.toInt(),
                    usbStatus = ClusterUsbExporter.statusLine(),
                )
            delay(TICK_MS)
        }
    }

    private fun installGuestFromAssets(context: Context) {
        val marker = File(context.filesDir, "plugins/$PLUGIN_NAME/.installed")
        val wasm = File(context.filesDir, "plugins/$PLUGIN_NAME/plugin.wasm")
        val version = BuildConfig.VERSION_CODE.toString()
        if (marker.isFile && wasm.isFile && marker.readText() == version) return
        val assets = context.assets
        val manifest =
            assets.open("plugins/$PLUGIN_NAME/plugin.json").use { it.readBytes().decodeToString() }
        val bytes = assets.open("plugins/$PLUGIN_NAME/plugin.wasm").use { it.readBytes() }
        val status = clusterPluginInstallGuest(manifest, bytes)
        check(status.startsWith("OK")) { "cluster install: $status" }
        marker.parentFile?.mkdirs()
        marker.writeText(version)
    }
}
