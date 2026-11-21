package no.navi.app

import android.content.Context
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
import app.organicmaps.sdk.NaviDrape
import app.organicmaps.sdk.OrganicMaps
import app.organicmaps.sdk.location.BaseLocationProvider
import app.organicmaps.sdk.location.LocationProviderFactory
import app.organicmaps.sdk.location.LocationState
import app.organicmaps.sdk.location.NaviLocationBridge
import app.organicmaps.sdk.util.ConnectionState
import uniffi.navi.comapsSetPlatformLocked
import java.io.File

/**
 * CoMaps drape backend (see [ComapsDrape]). Initialises the CoMaps Platform and
 * Framework on top of the headless routing session that
 * [ComapsSessionBootstrap] already opened.
 *
 * Order matters: `navi_comaps_init` points the Platform *resource* directory at
 * [MwmDataDir.mwmRoot] (pack-volume SD when selected); the Framework then
 * re-initialises the Platform with the APK as resource directory (fonts, symbols,
 * drules, classificator are read from the APK assets) and, once built,
 * `Framework.nativeChangeWritableDir` moves the *writable* directory to that same
 * MWM root so the leaf MWMs the router uses are drawn.
 */
object ComapsDrapeBootstrap : ComapsDrape.Backend {
    private const val TAG = "ComapsDrape"
    private const val MWM_ROOT = "mwm"

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
        val root = MwmDataDir.mwmRoot(context)
        if (!File(root, "World.mwm").isFile) return "World.mwm missing under ${root.absolutePath}"
        return try {
            val pm = context.packageManager
            val info = pm.getPackageInfo(context.packageName, 0)

            @Suppress("DEPRECATION")
            val code = info.versionCode
            // Graphics API: CoMaps' own default (Vulkan, GLES fallback) unless a pref / test hook
            // asks for one (read when the render engine is created, i.e. at the first surface).
            (NaviMapTestHooks.drapeGraphicsApi ?: MapHudPrefs.loadDrapeGraphicsApi(context))?.let {
                NaviDrape.nativeSetSetting("PreferredGraphicsAPI", it)
            }
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
            // The Framework now owns the Platform directories and the style: a later session
            // re-open (leaf download) must only rebuild the router.
            comapsSetPlatformLocked(true)
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
        // Application context: the surface outlives any one Activity (recreation on dark-mode switch).
        val view = MapView(context.applicationContext)
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

    override fun setCamera(
        lat: Double,
        lon: Double,
        level: Double,
        bearingDeg: Double,
        animated: Boolean,
    ) = NaviDrape.nativeSetCamera(lat, lon, level, bearingDeg, animated)

    override fun camera(): ComapsDrape.Camera? {
        if (!Map.isEngineCreated()) return null
        val v = runCatching { NaviDrape.nativeGetCamera() }.getOrNull() ?: return null
        if (v.size < 8) return null
        return ComapsDrape.Camera(v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7].toInt())
    }

    override fun rotateTo(
        bearingDeg: Double,
        animated: Boolean,
    ) = NaviDrape.nativeRotateTo(bearingDeg, animated)

    override fun fitRect(
        minLat: Double,
        minLon: Double,
        maxLat: Double,
        maxLon: Double,
        animated: Boolean,
    ) = NaviDrape.nativeFitRect(minLat, minLon, maxLat, maxLon, animated)

    override fun scaleBy(
        factor: Double,
        animated: Boolean,
    ) = NaviDrape.nativeScaleBy(factor, animated)

    override fun setFollowMode(mode: Int) = NaviDrape.nativeSetFollowMode(mode)

    override fun setNavigation(
        on: Boolean,
        pedestrian: Boolean,
    ) = NaviDrape.nativeSetNavigation(on, pedestrian)

    override fun pushLocation(
        timeMs: Long,
        lat: Double,
        lon: Double,
        accuracyM: Float,
        altitudeM: Double,
        speedMps: Float,
        bearingDeg: Float,
    ) {
        if (!Map.isEngineCreated()) return
        NaviLocationBridge.push(timeMs, lat, lon, accuracyM, altitudeM, speedMps, bearingDeg)
    }

    override fun pushCompass(headingDeg: Double) {
        if (!Map.isEngineCreated()) return
        Map.onCompassUpdated(Math.toRadians(headingDeg), false)
    }

    override fun setModeListener(listener: ((Int) -> Unit)?) {
        if (listener == null) {
            runCatching { LocationState.nativeRemoveListener() }
            return
        }
        LocationState.nativeSetListener(
            object : LocationState.ModeChangeListener {
                override fun onMyPositionModeChanged(newMode: Int) = listener(newMode)
            },
        )
    }

    override fun screenToLatLon(
        x: Float,
        y: Float,
    ): Pair<Double, Double>? {
        if (!Map.isEngineCreated()) return null
        val v = runCatching { NaviDrape.nativeScreenToLatLon(x, y) }.getOrNull() ?: return null
        return if (v.size >= 2) v[0] to v[1] else null
    }

    override fun registerMaps() {
        if (!Map.isEngineCreated()) return
        // Incremental AddLocalMaps cannot replace a country file already
        // registered from an older version directory (VersionAlreadyExists).
        // A full rescan is what makes newly downloaded leaves visible.
        runCatching { NaviDrape.nativeReloadMaps() }
            .onFailure { Log.w(TAG, "reload maps failed: ${it.message}") }
    }

    override fun deregisterMap(leafId: String) {
        if (!Map.isEngineCreated()) return
        runCatching { NaviDrape.nativeReloadMaps() }
            .onFailure { Log.w(TAG, "reload maps after deleting $leafId failed: ${it.message}") }
    }

    override fun setNight(night: Boolean) {
        val want = if (night) MapAppearance.Dark else MapAppearance.Light
        if (MapAppearance.get() != want) MapAppearance.set(want)
    }

    override fun isDayAt(
        lat: Double,
        lon: Double,
        utcSeconds: Long,
    ): Boolean = Framework.nativeIsDayTime(utcSeconds, lat, lon)

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

    override fun blockTapEvents(block: Boolean) = NaviDrape.nativeBlockTapEvents(block)

    override fun clearRoute() = NaviDrape.nativeClearRoute()

    override fun addSubroute(
        latlon: DoubleArray,
        type: Int,
        turnIdx: IntArray?,
    ) = NaviDrape.nativeAddSubroute(latlon, type, turnIdx)

    override fun setRoutePoints(
        latlon: DoubleArray,
        titles: Array<String>,
    ) = NaviDrape.nativeSetRoutePoints(latlon, titles)

    override fun createGroup(name: String): Long = NaviDrape.nativeCreateGroup(name)

    override fun setMarks(
        group: Long,
        latlon: DoubleArray,
        names: Array<String>,
        icons: IntArray,
        colors: IntArray,
        symbols: Array<String>?,
    ) = NaviDrape.nativeSetMarks(group, latlon, names, icons, colors, symbols)

    override fun setPuckState(state: Int) {
        if (!Map.isEngineCreated()) return
        runCatching { NaviDrape.nativeSetPuckState(state) }
            .onFailure { Log.w(TAG, "setPuckState failed: ${it.message}") }
    }

    override fun setTracks(
        group: Long,
        counts: IntArray,
        latlon: DoubleArray,
        argb: IntArray,
        widths: FloatArray,
    ) = NaviDrape.nativeSetTracks(group, counts, latlon, argb, widths)
}
