package no.navi.app

import android.content.Context

/** Session map-HUD preferences persisted on device. */
object MapHudPrefs {
    private const val PREFS = "navi_map_hud"
    private const val KEY_AUTO_ZOOM_LEVEL = "auto_zoom_level"
    private const val KEY_AUTO_ZOOM_ON = "auto_zoom_on"
    private const val KEY_BREAK_AS_DISTANCE = "break_as_distance"
    private const val KEY_PREFER_METRIC = "prefer_metric"
    private const val KEY_OPT_IN_3D = "opt_in_3d"
    private const val KEY_CAMERA_TILT = "camera_tilt_deg"
    private const val KEY_PMTILES_BASE_URL = "pmtiles_base_url"
    private const val KEY_GEOFABRIK_PATH = "geofabrik_path"
    const val DEFAULT_AUTO_ZOOM_LEVEL = 16.5
    const val MIN_ZOOM = 3.0
    const val MAX_ZOOM = 20.0

    /** Assumed cruise speed used only to present break time as distance. */
    const val BREAK_DISPLAY_SPEED_KMH = 80.0

    /** Camera tilt presets (degrees). Slider snaps to these only. */
    val CAMERA_TILT_PRESETS: DoubleArray = doubleArrayOf(0.0, 35.0, 45.0, 65.0)
    const val DEFAULT_CAMERA_TILT_DEG = 0.0

    /**
     * This APK links `android-sdk-vulkan`. Treat Vulkan as available for the 3D
     * style gate; GLES-only builds would return false.
     */
    fun vulkanRendererAvailable(): Boolean = true

    fun clampZoom(level: Double): Double = level.coerceIn(MIN_ZOOM, MAX_ZOOM)

    /** Snap to the nearest [CAMERA_TILT_PRESETS] value. */
    fun snapTilt(deg: Double): Double {
        var best = CAMERA_TILT_PRESETS[0]
        var bestD = kotlin.math.abs(deg - best)
        for (p in CAMERA_TILT_PRESETS) {
            val d = kotlin.math.abs(deg - p)
            if (d < bestD) {
                best = p
                bestD = d
            }
        }
        return best
    }

    fun loadCameraTiltDeg(context: Context): Double {
        val raw =
            context
                .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                .getFloat(KEY_CAMERA_TILT, DEFAULT_CAMERA_TILT_DEG.toFloat())
                .toDouble()
        return snapTilt(raw)
    }

    fun saveCameraTiltDeg(
        context: Context,
        deg: Double,
    ) {
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putFloat(KEY_CAMERA_TILT, snapTilt(deg).toFloat())
            .apply()
    }

    fun loadAutoZoomLevel(context: Context): Double {
        val raw =
            context
                .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                .getFloat(KEY_AUTO_ZOOM_LEVEL, DEFAULT_AUTO_ZOOM_LEVEL.toFloat())
                .toDouble()
        return clampZoom(raw)
    }

    fun loadAutoZoomOn(context: Context): Boolean =
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean(KEY_AUTO_ZOOM_ON, false)

    fun saveAutoZoom(
        context: Context,
        level: Double,
        enabled: Boolean? = null,
    ) {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
        prefs.putFloat(KEY_AUTO_ZOOM_LEVEL, clampZoom(level).toFloat())
        if (enabled != null) {
            prefs.putBoolean(KEY_AUTO_ZOOM_ON, enabled)
        }
        prefs.apply()
    }

    /** When true, bottom HUD shows break remaining as distance; otherwise as time. */
    fun loadBreakAsDistance(context: Context): Boolean =
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean(KEY_BREAK_AS_DISTANCE, false)

    fun saveBreakAsDistance(
        context: Context,
        asDistance: Boolean,
    ) {
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putBoolean(KEY_BREAK_AS_DISTANCE, asDistance)
            .apply()
    }

    fun loadPreferMetric(context: Context): Boolean =
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean(KEY_PREFER_METRIC, true)

    fun savePreferMetric(
        context: Context,
        preferMetric: Boolean,
    ) {
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putBoolean(KEY_PREFER_METRIC, preferMetric)
            .apply()
    }

    /** Opt-in experimental OpenFreeMap 3D (online only). Never the default. */
    fun loadOptIn3d(context: Context): Boolean =
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean(KEY_OPT_IN_3D, false)

    fun saveOptIn3d(
        context: Context,
        enabled: Boolean,
    ) {
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putBoolean(KEY_OPT_IN_3D, enabled)
            .apply()
    }

    fun loadPmtilesBaseUrl(context: Context): String =
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getString(KEY_PMTILES_BASE_URL, "")
            .orEmpty()

    fun savePmtilesBaseUrl(
        context: Context,
        url: String,
    ) {
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putString(KEY_PMTILES_BASE_URL, url.trim())
            .apply()
    }

    fun loadGeofabrikPath(context: Context): String =
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getString(KEY_GEOFABRIK_PATH, "europe/norway/ostlandet")
            .orEmpty()
            .ifBlank { "europe/norway/ostlandet" }

    fun saveGeofabrikPath(
        context: Context,
        path: String,
    ) {
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putString(KEY_GEOFABRIK_PATH, path.trim().trim('/'))
            .apply()
    }
}
