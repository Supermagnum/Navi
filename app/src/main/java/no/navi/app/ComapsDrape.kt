package no.navi.app

import android.content.Context
import android.util.Log
import android.view.View
import androidx.lifecycle.LifecycleOwner
import java.util.concurrent.atomic.AtomicBoolean

/** Map day/night source: the system setting, or sunrise / sunset at the current position. */
enum class DrapeDayNight { SYSTEM, AUTO_SUN }

/**
 * Phase 6b: with the MWM client flag ON and the pinned CoMaps Java SDK (`:comaps-sdk`)
 * packaged, the CoMaps drape engine is the only map renderer. It owns the camera
 * (centre, zoom, rotation, perspective, heading-up and follow mode, gestures) and draws
 * Navi's overlays through CoMaps route rendering, route points, user marks and tracks
 * ([ComapsDrapeMap]). MapLibre and PMTiles are not used on this path.
 *
 * This facade lives in the main source set and never references `app.organicmaps.sdk`
 * classes, so the app still compiles when the (gitignored) CoMaps checkout is not
 * present. The real implementation, `no.navi.app.ComapsDrapeBootstrap`, lives in
 * `src/drape/java` and is looked up by name; if it is absent or initialisation fails
 * the reason is kept in [lastError] and the map area shows it.
 */

object ComapsDrape {
    private const val TAG = "ComapsDrape"
    private const val BACKEND_CLASS = "no.navi.app.ComapsDrapeBootstrap"

    enum class State { UNAVAILABLE, IDLE, INITIALIZING, READY, FAILED }

    /** Camera as read back from the engine. [mode] uses the CoMaps my-position constants (3 = follow, 4 = heading up). */
    data class Camera(
        val lat: Double,
        val lon: Double,
        val level: Double,
        val bearingDeg: Double,
        val pitchDeg: Double,
        val widthPx: Double,
        val heightPx: Double,
        val mode: Int,
    )

    /** Bearing value that keeps the current rotation (the native build uses -ffast-math, so no NaN). */
    const val NO_BEARING = 1000.0

    /** Follow modes Navi asks for. */
    object Follow {
        const val FREE = 0
        const val NORTH_UP = 1
        const val HEADING_UP = 2
    }

    /** Everything the Compose host needs from the SDK-backed implementation. */
    interface Backend {
        /**
         * Platform + Framework init on top of the already open headless session.
         * Must run on the main thread. Returns null on success, else a reason.
         */
        fun initialize(context: Context): String?

        /** The drape surface (an `app.organicmaps.sdk.MapView`). One instance per process. */
        fun createMapView(context: Context): View

        fun attachLifecycle(owner: LifecycleOwner)

        fun detachLifecycle(owner: LifecycleOwner)

        /** Centre, CoMaps scale level (fractional) and map bearing; level < 0 / [NO_BEARING] keep the current value. */
        fun setCamera(
            lat: Double,
            lon: Double,
            level: Double,
            bearingDeg: Double,
            animated: Boolean,
        )

        fun camera(): Camera?

        /** Rotate to a map bearing about the visible centre; keeps a follow mode. */
        fun rotateTo(
            bearingDeg: Double,
            animated: Boolean,
        )

        /** Fit a lat/lon box into the viewport (route overview). */
        fun fitRect(
            minLat: Double,
            minLon: Double,
            maxLat: Double,
            maxLon: Double,
            animated: Boolean,
        )

        /** Relative zoom around the visible centre; keeps a follow mode. */
        fun scaleBy(
            factor: Double,
            animated: Boolean,
        )

        /** [Follow.FREE], [Follow.NORTH_UP] or [Follow.HEADING_UP]. */
        fun setFollowMode(mode: Int)

        /** Navigation camera (puck low on screen, perspective when 3D is allowed). */
        fun setNavigation(
            on: Boolean,
            pedestrian: Boolean,
        )

        /** Feeds a position into the drape (puck, follow mode, heading from [bearing] when moving). */
        fun pushLocation(
            timeMs: Long,
            lat: Double,
            lon: Double,
            accuracyM: Float,
            altitudeM: Double,
            speedMps: Float,
            bearingDeg: Float,
        )

        /** Compass heading in degrees (heading-up when no GPS bearing is fresh). */
        fun pushCompass(headingDeg: Double)

        /** Called with the CoMaps my-position mode whenever it changes (user pan -> not follow). */
        fun setModeListener(listener: ((Int) -> Unit)?)

        fun screenToLatLon(
            x: Float,
            y: Float,
        ): Pair<Double, Double>?

        /** Register leaf MWMs that appeared after the Framework was created (live download). */
        fun registerMaps()

        /** Re-scan the leaf MWMs after the user deleted one (releases the file); [leafId] is for logging. */
        fun deregisterMap(leafId: String)

        fun setNight(night: Boolean)

        /** CoMaps' own sunrise / sunset test (polar day counts as day). */
        fun isDayAt(
            lat: Double,
            lon: Double,
            utcSeconds: Long,
        ): Boolean

        fun isNight(): Boolean

        fun setContours(enabled: Boolean)

        fun contoursEnabled(): Boolean

        fun set3d(
            allow3d: Boolean,
            allow3dBuildings: Boolean,
        )

        /** Result of the last [set3d] as read back from the Framework: allow3d to buildings. */
        fun get3d(): Pair<Boolean, Boolean>

        fun renderingActive(): Boolean

        /** Navi has no CoMaps place page: block the drape's own tap selection. */
        fun blockTapEvents(block: Boolean)

        fun drawScale(): Int

        // Overlays (main thread only). `type`: 0 car, 1 pedestrian, 2 bicycle.
        fun clearRoute()

        fun addSubroute(
            latlon: DoubleArray,
            type: Int,
            turnIdx: IntArray?,
        )

        fun setRoutePoints(
            latlon: DoubleArray,
            titles: Array<String>,
        )

        fun createGroup(name: String): Long

        fun setMarks(
            group: Long,
            latlon: DoubleArray,
            names: Array<String>,
            icons: IntArray,
            colors: IntArray,
            symbols: Array<String>?,
        )

        /** Navi position puck: 0 = GPS (blue), 1 = estimated (amber), 2 = lost (grey). */
        fun setPuckState(state: Int)

        fun setTracks(
            group: Long,
            counts: IntArray,
            latlon: DoubleArray,
            argb: IntArray,
            widths: FloatArray,
        )
    }

    @Volatile
    var state: State = State.IDLE
        private set

    @Volatile
    var lastError: String? = null
        private set

    private val backendRef: Backend? by lazy(LazyThreadSafetyMode.PUBLICATION) {
        runCatching {
            Class
                .forName(BACKEND_CLASS)
                .getField("INSTANCE")
                .get(null) as Backend
        }.onFailure { Log.i(TAG, "drape backend not packaged: ${it.javaClass.simpleName}") }
            .getOrNull()
    }

    /** True when the `:comaps-sdk` classes are in this APK. */
    fun sdkPackaged(): Boolean = backendRef != null

    val backend: Backend? get() = backendRef.takeIf { state == State.READY }

    private val started = AtomicBoolean(false)

    /**
     * True when the live map is the CoMaps drape (MWM flag ON, SDK packaged, not disabled by a
     * test hook). With the flag OFF, or without the SDK in the APK, the classic MapLibre /
     * PMTiles map is used unchanged.
     */
    fun isMwmMapPath(): Boolean = MwmClientBootstrap.isEnabled() && sdkPackaged() && !NaviMapTestHooks.drapeDisabled

    /** Forward a fix to the drape (no-op until it is ready). Safe from any thread that owns the fix. */
    fun pushLocation(loc: android.location.Location) {
        val b = backend ?: return
        b.pushLocation(
            timeMs = if (loc.time > 0L) loc.time else System.currentTimeMillis(),
            lat = loc.latitude,
            lon = loc.longitude,
            accuracyM = if (loc.hasAccuracy()) loc.accuracy else 0f,
            altitudeM = if (loc.hasAltitude()) loc.altitude else 0.0,
            speedMps = if (loc.hasSpeed()) loc.speed else 0f,
            bearingDeg = if (loc.hasBearing()) loc.bearing else 0f,
        )
    }

    /**
     * Whether this process should even try the drape: MWM flag ON, native core
     * loaded, SDK packaged, headless session open (so the Platform resource
     * directory is already set and will not be clobbered later).
     */
    fun eligible(): Boolean =
        MwmClientBootstrap.isEnabled() &&
            MwmClientBootstrap.nativeLoaded &&
            sdkPackaged() &&
            ComapsSessionBootstrap.isOpen()

    /**
     * Initialise once, on the main thread. Safe to call repeatedly (e.g. from a
     * Compose effect); returns the state after the attempt.
     *
     * [State.FAILED] is not terminal: when `World.mwm` becomes visible later
     * (SD pack volume mounts after a cold-start fallback to empty internal),
     * the next call retries.
     */
    fun ensureInitialized(context: Context): State {
        if (state == State.READY) return state
        if (state == State.FAILED) {
            val world = java.io.File(MwmDataDir.mwmRoot(context), "World.mwm")
            if (!world.isFile) return state
            Log.i(TAG, "retrying drape init; World.mwm now at ${world.absolutePath}")
            started.set(false)
            state = State.IDLE
            lastError = null
        }
        val impl = backendRef
        if (impl == null) {
            state = State.UNAVAILABLE
            lastError = "comaps-sdk not packaged"
            return state
        }
        if (!eligible()) return state
        if (!started.compareAndSet(false, true)) return state
        state = State.INITIALIZING
        val t0 = System.currentTimeMillis()
        val err =
            try {
                impl.initialize(context.applicationContext)
            } catch (t: Throwable) {
                Log.w(TAG, "drape init threw", t)
                "${t.javaClass.simpleName}: ${t.message}"
            }
        val ms = System.currentTimeMillis() - t0
        if (err == null) {
            state = State.READY
            lastError = null
            Log.i(TAG, "drape ready init_ms=$ms")
        } else {
            state = State.FAILED
            lastError = err
            // Allow another attempt once World / volume state changes.
            started.set(false)
            Log.w(TAG, "drape unavailable: $err init_ms=$ms")
        }
        NaviMapTestHooks.drapeState = state.name
        NaviMapTestHooks.drapeError = lastError
        return state
    }
}
