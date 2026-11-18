package no.navi.app

import android.content.Context

/**
 * Match-rate threshold for treating a leaf's vehicle_limits as fully applied.
 * Below the threshold (or when coverage is missing), restrictions that matched
 * are still used, but the UI warns that coverage is only partly available.
 */
object VehicleLimitsCoverageConfig {
    const val PREFS = "navi_vehicle_limits_coverage"
    const val KEY_THRESHOLD = "match_rate_threshold"
    const val DEFAULT_THRESHOLD = 0.90

    fun threshold(context: Context): Double {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        return prefs
            .getFloat(KEY_THRESHOLD, DEFAULT_THRESHOLD.toFloat())
            .toDouble()
            .coerceIn(0.0, 1.0)
    }

    fun setThreshold(
        context: Context,
        value: Double,
    ) {
        context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putFloat(KEY_THRESHOLD, value.toFloat().coerceIn(0f, 1f))
            .apply()
    }
}
