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
    private const val KEY_SPEED_CAMERA_OPT_IN = "speed_camera_opt_in"
    private const val KEY_SPEED_CAMERA_PROMPT_SHOWN = "speed_camera_prompt_shown"
    private const val KEY_CAMERA_TILT = "camera_tilt_deg"
    private const val KEY_PMTILES_BASE_URL = "pmtiles_base_url"
    private const val KEY_GEOFABRIK_PATH = "geofabrik_path"
    private const val KEY_DIAGNOSTIC_LOGGING = "diagnostic_logging"
    private const val KEY_DOWNLOADED_PMTILES_REGIONS = "downloaded_pmtiles_regions"
    private const val KEY_SNAP_ROTATION_BACK = "snap_rotation_back"
    const val DEFAULT_AUTO_ZOOM_LEVEL = 16.5
    const val MIN_ZOOM = 3.0
    const val MAX_ZOOM = 20.0

    /** Assumed cruise speed used only to present break time as distance. */
    const val BREAK_DISPLAY_SPEED_KMH = 80.0

    /**
     * MapLibre Native clamps camera tilt to
     * `[0, MAPLIBRE_MAX_TILT_DEG]` (`CameraPosition.Builder.tilt`). Presets must
     * stay inside that range — a preset above the clamp (e.g. former 65°) never
     * matches the live camera, so idle style/tilt re-apply fights HUD zoom.
     */
    const val MAPLIBRE_MAX_TILT_DEG = 60.0

    /** Camera tilt presets (degrees). Slider snaps to these only. */
    val CAMERA_TILT_PRESETS: DoubleArray = doubleArrayOf(0.0, 35.0, 45.0, MAPLIBRE_MAX_TILT_DEG)
    const val DEFAULT_CAMERA_TILT_DEG = 0.0

    /**
     * 3D / tilt gate. Name retained for less churn: APK now links GLES
     * (`android-sdk`), not `android-sdk-vulkan`. Still returns true so 3D is
     * not gated on the SDK artifact.
     */
    fun vulkanRendererAvailable(): Boolean = true

    fun clampZoom(level: Double): Double = level.coerceIn(MIN_ZOOM, MAX_ZOOM)

    /** Snap to the nearest [CAMERA_TILT_PRESETS] value (within MapLibre max). */
    fun snapTilt(deg: Double): Double {
        val clamped = deg.coerceIn(0.0, MAPLIBRE_MAX_TILT_DEG)
        var best = CAMERA_TILT_PRESETS[0]
        var bestD = kotlin.math.abs(clamped - best)
        for (p in CAMERA_TILT_PRESETS) {
            val d = kotlin.math.abs(clamped - p)
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

    /**
     * Speed-camera warnings (display only). Default off; first-run prompt in
     * allowed jurisdictions (NO/UK). Never enable silently.
     */
    fun loadSpeedCameraOptIn(context: Context): Boolean =
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean(KEY_SPEED_CAMERA_OPT_IN, false)

    fun saveSpeedCameraOptIn(
        context: Context,
        enabled: Boolean,
    ) {
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putBoolean(KEY_SPEED_CAMERA_OPT_IN, enabled)
            .apply()
    }

    fun loadSpeedCameraPromptShown(context: Context): Boolean =
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean(KEY_SPEED_CAMERA_PROMPT_SHOWN, false)

    fun saveSpeedCameraPromptShown(
        context: Context,
        shown: Boolean,
    ) {
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putBoolean(KEY_SPEED_CAMERA_PROMPT_SHOWN, shown)
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

    /**
     * Diagnostic session log toggle (default **off**). Persisted with other map HUD
     * prefs so it survives restarts; [DiagnosticLog] is a no-op while false.
     */
    fun loadDiagnosticLogging(context: Context): Boolean =
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean(KEY_DIAGNOSTIC_LOGGING, false)

    fun saveDiagnosticLogging(
        context: Context,
        enabled: Boolean,
    ) {
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putBoolean(KEY_DIAGNOSTIC_LOGGING, enabled)
            .apply()
    }

    /**
     * Region keys for basemap PMTiles the user successfully downloaded.
     * Survives `install -r` (same UID); wiped on uninstall — pair with
     * [OfflineDataIntegrity] staging detection for full reinstall cases.
     */
    fun loadDownloadedPmtilesRegions(context: Context): Set<String> =
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getStringSet(KEY_DOWNLOADED_PMTILES_REGIONS, emptySet())
            ?.toSet()
            .orEmpty()

    /**
     * When true (default), a manual rotate gesture is temporary: after a short
     * pause the active Compass/Travel/N-up mode reasserts its bearing. When
     * false, manual bearing persists until the user picks a rotation mode chip.
     */
    fun loadSnapRotationBack(context: Context): Boolean =
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean(KEY_SNAP_ROTATION_BACK, true)

    fun saveSnapRotationBack(
        context: Context,
        enabled: Boolean,
    ) {
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putBoolean(KEY_SNAP_ROTATION_BACK, enabled)
            .apply()
    }

    fun rememberDownloadedPmtilesRegion(
        context: Context,
        regionKey: String,
    ) {
        val key = regionKey.trim()
        if (key.isEmpty()) return
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val next = prefs.getStringSet(KEY_DOWNLOADED_PMTILES_REGIONS, emptySet())!!.toMutableSet()
        next.add(key)
        prefs.edit().putStringSet(KEY_DOWNLOADED_PMTILES_REGIONS, next).apply()
    }
}
