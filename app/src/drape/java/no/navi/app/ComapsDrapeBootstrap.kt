package no.navi.app

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.view.View
import androidx.lifecycle.LifecycleOwner
import app.organicmaps.sdk.Framework
import app.organicmaps.sdk.Map
import app.organicmaps.sdk.MapAppearance
import app.organicmaps.sdk.MapController
import app.organicmaps.sdk.MapRenderingListener
import app.organicmaps.sdk.MapView
import app.organicmaps.sdk.OrganicMaps
import app.organicmaps.sdk.location.BaseLocationProvider
import app.organicmaps.sdk.location.LocationProviderFactory
import app.organicmaps.sdk.util.ConnectionState
import java.io.File
import kotlin.math.abs
import kotlin.math.pow
import kotlin.math.roundToInt

/**
 * CoMaps drape backend (see [ComapsDrape]). Initialises the CoMaps Platform and
 * Framework on top of the headless routing session that
 * [ComapsSessionBootstrap] already opened.
 *
 * Order matters: `navi_comaps_init` points the Platform *resource* directory at
 * `{filesDir}/mwm`; the Framework then re-initialises the Platform with the APK as
 * resource directory (fonts, symbols, drules, classificator are read from the APK
 * assets) and, once built, `Framework.nativeChangeWritableDir` moves the *writable*
 * directory to `{filesDir}/mwm` so the same leaf MWMs the router uses are drawn.
 */
object ComapsDrapeBootstrap : ComapsDrape.Backend {
    private const val TAG = "ComapsDrape"
    private const val MWM_ROOT = "mwm"
    private const val MIN_LEVEL = 1
    private const val MAX_LEVEL = 19
    private const val MIN_FRACTION = 0.02
    private const val FRACTION_DELAY_MS = 1500L

    private var organicMaps: OrganicMaps? = null
    private var controller: MapController? = null
    private var mapView: MapView? = null

    private val noopProviders =
        object : LocationProviderFactory {
            override fun isGoogleLocationAvailable(context: Context): Boolean = false

            override fun getProvider(
                context: Context,
                listener: BaseLocationProvider.Listener,
            ): BaseLocationProvider = NaviNoopLocationProvider(listener)
        }

    /** Navi owns GPS; the drape never starts a location provider. */
    private class NaviNoopLocationProvider(
        listener: BaseLocationProvider.Listener,
    ) : BaseLocationProvider(listener) {
        override fun start(interval: Long) = Unit

        override fun stop() = Unit
    }

    override fun initialize(context: Context): String? {
        if (Looper.myLooper() != Looper.getMainLooper()) return "drape init must run on the main thread"
        val root = File(NaviAppData.resolve(context), MWM_ROOT)
        if (!File(root, "World.mwm").isFile) return "World.mwm missing under ${root.absolutePath}"
        return try {
            val pm = context.packageManager
            val info = pm.getPackageInfo(context.packageName, 0)

            @Suppress("DEPRECATION")
            val code = info.versionCode
            val om =
                OrganicMaps(
                    context,
                    "navi",
                    context.packageName,
                    code,
                    info.versionName ?: "0",
                    "${context.packageName}.fileprovider",
                    noopProviders,
                )
            // Platform init reads the connection state through this singleton.
            ConnectionState.INSTANCE.initialize(context)
            var frameworkReady = false
            if (!om.init { frameworkReady = true }) return "OrganicMaps.init returned false"
            organicMaps = om
            Framework.nativeChangeWritableDir(root.absolutePath.trimEnd('/') + "/")
            applyNavi3d(context)
            Log.i(
                TAG,
                "framework ready=$frameworkReady writable=${Framework.nativeGetWritableDir()} " +
                    "dataExt=${Framework.nativeGetDataFileExt()}",
            )
            null
        } catch (t: Throwable) {
            Log.w(TAG, "OrganicMaps init failed", t)
            "${t.javaClass.simpleName}: ${t.message}"
        }
    }

    private fun applyNavi3d(context: Context) {
        val on = MapHudPrefs.loadOptIn3d(context)
        Framework.nativeSet3dMode(on, on)
    }

    override fun createMapView(context: Context): View {
        mapView?.let { return it }
        val om = checkNotNull(organicMaps) { "drape not initialised" }
        val view = MapView(context)
        val ctl =
            MapController(
                view,
                om.locationHelper,
                object : MapRenderingListener {
                    override fun onRenderingCreated() {
                        Log.i(TAG, "rendering created")
                    }

                    override fun onRenderingRestored() {
                        Log.i(TAG, "rendering restored")
                    }

                    override fun onRenderingInitializationFinished() {
                        Log.i(TAG, "rendering initialisation finished")
                    }
                },
                { Log.w(TAG, "drape reports unsupported graphics (no GLES3 / Vulkan)") },
                false,
            )
        controller = ctl
        mapView = view
        return view
    }

    override fun attachLifecycle(owner: LifecycleOwner) {
        controller?.let { owner.lifecycle.addObserver(it) }
    }

    override fun detachLifecycle(owner: LifecycleOwner) {
        controller?.let { owner.lifecycle.removeObserver(it) }
    }

    @Volatile
    private var lockedZoomOffset: Double? = null

    /** Computed once the engine exists: the drape fixes its tile size from the first surface. */
    override fun zoomOffset(): Double? {
        lockedZoomOffset?.let { return it }
        val view = mapView ?: return null
        if (view.width == 0 || view.height == 0 || controller?.isRenderingActive() != true) return null
        val res = view.resources
        return DrapeCameraSync
            .zoomOffset(res.configuration.densityDpi, res.displayMetrics.density, maxOf(view.width, view.height))
            .also {
                lockedZoomOffset = it
                Log.i(TAG, "zoom offset=$it dpi=${res.configuration.densityDpi} surface=${view.width}x${view.height}")
            }
    }

    private val handler = Handler(Looper.getMainLooper())
    private var pendingFraction: Runnable? = null

    override fun setViewport(
        lat: Double,
        lon: Double,
        zoomLevel: Double,
    ) {
        val level = zoomLevel.roundToInt().coerceIn(MIN_LEVEL, MAX_LEVEL)
        Framework.nativeSetViewportCenter(lat, lon, level)
        pendingFraction?.let { handler.removeCallbacks(it) }
        pendingFraction = null
        val fraction = zoomLevel - level
        val view = mapView
        if (abs(fraction) < MIN_FRACTION || view == null) return
        // SetViewportCenter animates to the integer level; scale relatively once it landed.
        val task =
            Runnable {
                pendingFraction = null
                Map.onScale(2.0.pow(fraction), view.width / 2.0, view.height / 2.0, false)
            }
        pendingFraction = task
        handler.postDelayed(task, FRACTION_DELAY_MS)
    }

    override fun setNight(night: Boolean) {
        val want = if (night) MapAppearance.Dark else MapAppearance.Light
        if (MapAppearance.get() != want) MapAppearance.set(want)
    }

    override fun isNight(): Boolean = MapAppearance.get() == MapAppearance.Dark

    override fun setContours(enabled: Boolean) {
        if (Framework.nativeIsIsolinesLayerEnabled() != enabled) {
            Framework.nativeSetIsolinesLayerEnabled(enabled)
        }
    }

    override fun contoursEnabled(): Boolean = Framework.nativeIsIsolinesLayerEnabled()

    override fun set3d(
        allow3d: Boolean,
        allow3dBuildings: Boolean,
    ) {
        val cur = Framework.Params3dMode().also { Framework.nativeGet3dMode(it) }
        if (cur.enabled != allow3d || cur.buildings != allow3dBuildings) {
            Framework.nativeSet3dMode(allow3d, allow3dBuildings)
        }
    }

    override fun get3d(): Pair<Boolean, Boolean> {
        val cur = Framework.Params3dMode().also { Framework.nativeGet3dMode(it) }
        return cur.enabled to cur.buildings
    }

    override fun renderingActive(): Boolean = controller?.isRenderingActive() == true

    override fun drawScale(): Int = Framework.nativeGetDrawScale()
}
