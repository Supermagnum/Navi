package no.navi.app

import android.content.Context
import android.util.Log
import android.view.View
import androidx.lifecycle.LifecycleOwner
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Phase 6 hybrid rendering: when the MWM client flag is ON and the pinned CoMaps
 * Java SDK (`:comaps-sdk`) is packaged, the CoMaps drape engine draws the basemap
 * behind a transparent MapLibre view that keeps drawing Navi's own overlays
 * (route, waypoints, DATEX, repeaters, GPS puck ...). MapLibre stays the camera
 * master; drape follows it ([ComapsDrapeMap]).
 *
 * This facade lives in the main source set and never references `app.organicmaps.sdk`
 * classes, so the app still compiles when the (gitignored) CoMaps checkout is not
 * present. The real implementation, `no.navi.app.ComapsDrapeBootstrap`, lives in
 * `src/drape/java` and is looked up by name; if it is absent or initialisation fails
 * the app keeps the MapLibre basemap and records the reason in [lastError].
 */
object ComapsDrape {
    private const val TAG = "ComapsDrape"
    private const val BACKEND_CLASS = "no.navi.app.ComapsDrapeBootstrap"

    enum class State { UNAVAILABLE, IDLE, INITIALIZING, READY, FAILED }

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

        /**
         * Centre + CoMaps scale level. The CoMaps API only takes an integer level (animated
         * by the engine); the fractional part is applied afterwards as a relative scale.
         */
        fun setViewport(
            lat: Double,
            lon: Double,
            zoomLevel: Double,
        )

        /**
         * MapLibre-zoom to CoMaps-level offset for this device (see
         * [DrapeCameraSync.zoomOffset]); null until the drape surface has a size.
         */
        fun zoomOffset(): Double?

        fun setNight(night: Boolean)

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

        fun drawScale(): Int
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
     * Cheap synchronous pre-check used before the MapLibre view is created (the view must
     * be built translucent up front): the SDK is packaged and an MWM release with World
     * maps is installed. Does not guarantee the Framework will start.
     */
    fun likelyAvailable(context: Context): Boolean {
        if (!MwmClientBootstrap.isEnabled() || !sdkPackaged()) return false
        val root = java.io.File(NaviAppData.resolve(context), "mwm")
        return java.io.File(root, "active.json").isFile && java.io.File(root, "World.mwm").exists()
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
     */
    fun ensureInitialized(context: Context): State {
        if (state == State.READY || state == State.FAILED) return state
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
            Log.w(TAG, "drape unavailable (MapLibre fallback): $err init_ms=$ms")
        }
        NaviMapTestHooks.drapeState = state.name
        NaviMapTestHooks.drapeError = lastError
        return state
    }
}
