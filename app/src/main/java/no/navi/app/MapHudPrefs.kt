package no.navi.app

import android.content.Context

/** Session map-HUD preferences (auto-zoom level) persisted on device. */
object MapHudPrefs {
    private const val PREFS = "navi_map_hud"
    private const val KEY_AUTO_ZOOM_LEVEL = "auto_zoom_level"
    private const val KEY_AUTO_ZOOM_ON = "auto_zoom_on"
    const val DEFAULT_AUTO_ZOOM_LEVEL = 16.5
    const val MIN_ZOOM = 3.0
    const val MAX_ZOOM = 20.0

    fun clampZoom(level: Double): Double = level.coerceIn(MIN_ZOOM, MAX_ZOOM)

    fun loadAutoZoomLevel(context: Context): Double {
        val raw = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getFloat(KEY_AUTO_ZOOM_LEVEL, DEFAULT_AUTO_ZOOM_LEVEL.toFloat())
            .toDouble()
        return clampZoom(raw)
    }

    fun loadAutoZoomOn(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean(KEY_AUTO_ZOOM_ON, false)

    fun saveAutoZoom(context: Context, level: Double, enabled: Boolean? = null) {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
        prefs.putFloat(KEY_AUTO_ZOOM_LEVEL, clampZoom(level).toFloat())
        if (enabled != null) {
            prefs.putBoolean(KEY_AUTO_ZOOM_ON, enabled)
        }
        prefs.apply()
    }
}
