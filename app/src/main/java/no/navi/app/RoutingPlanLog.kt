package no.navi.app

import android.util.Log
import org.json.JSONArray
import uniffi.navi.CorridorRouteResult

/**
 * Planning progress visible via `adb logcat -s NaviRouting:I`.
 *
 * Lines cover: start, percent progress, eco on/off, completion timing, and
 * pause/POI names discovered along the finished route.
 */
object RoutingPlanLog {
    const val TAG = "NaviRouting"

    fun start(
        profile: String,
        ecoEnabled: Boolean,
        legCount: Int,
        waypointNames: List<String>,
    ) {
        Log.i(
            TAG,
            "planning_start profile=$profile eco=$ecoEnabled legs=$legCount " +
                "waypoints=${waypointNames.joinToString("|")}",
        )
        progress(0, ecoEnabled, detail = "queued")
    }

    fun progress(
        pct: Int,
        ecoEnabled: Boolean,
        detail: String = "",
    ) {
        val clamped = pct.coerceIn(0, 100)
        val extra = if (detail.isBlank()) "" else " detail=$detail"
        Log.i(TAG, "planning_progress pct=$clamped eco=$ecoEnabled$extra")
    }

    fun complete(
        result: CorridorRouteResult,
        ecoEnabled: Boolean,
        durationMs: Long,
    ) {
        progress(100, ecoEnabled, detail = "done")
        Log.i(
            TAG,
            "planning_done eco=$ecoEnabled duration_ms=$durationMs " +
                "distance_km=${"%.3f".format(result.distanceKm)} " +
                "polyline_chars=${result.routePolyline.length}",
        )
        logPois(result.breakPoisJson)
    }

    fun failed(
        ecoEnabled: Boolean,
        durationMs: Long,
        reason: String,
    ) {
        Log.w(
            TAG,
            "planning_failed eco=$ecoEnabled duration_ms=$durationMs reason=$reason",
        )
    }

    fun logPois(breakPoisJson: String) {
        val names = mutableListOf<String>()
        val kinds = mutableListOf<String>()
        runCatching {
            val arr = JSONArray(breakPoisJson.ifBlank { "[]" })
            for (i in 0 until arr.length()) {
                val o = arr.optJSONObject(i) ?: continue
                val name = o.optString("name").trim()
                if (name.isEmpty()) continue
                names.add(name)
                kinds.add(o.optString("kind").ifBlank { "poi" })
            }
        }
        Log.i(
            TAG,
            "planning_pois count=${names.size} names=${names.joinToString("|")} " +
                "kinds=${kinds.joinToString("|")}",
        )
    }
}
