package no.navi.app

import android.content.Context
import android.content.pm.ApplicationInfo
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import uniffi.navi.CorridorRouteResult
import java.security.MessageDigest
import java.util.concurrent.atomic.AtomicReference

/**
 * Debug-only logcat capture of each completed route plan for offline compare
 * with the host maneuver harness (`distance_km`, `eta_minutes`, `pack_hit`,
 * `edge_hash`, `maneuvers`).
 *
 * `edge_hash` is SHA-256 (lowercase hex) of the UTF-8 `route_polyline` string —
 * the same bytes the `/tmp/navi-maneuver-dump` harness hashes. The polyline is
 * built from the joined A* edge sequence (including via-leg joins), so it is the
 * stable edge-sequence fingerprint available on both host and device without a
 * UniFFI field.
 *
 * Watch with: `adb logcat -s NaviManeuverDump:I`
 *
 * Long JSON is split across numbered lines because logcat truncates ~4 KiB.
 * Release / non-debuggable installs never emit (see [debugBuild]).
 */
object NaviManeuverDump {
    const val TAG = "NaviManeuverDump"

    /** Stay under typical logcat line limit (~4 KiB) with room for the prefix. */
    private const val CHUNK_CHARS = 3_500

    private val lastProfile = AtomicReference("unknown")
    private val lastDebugContext = AtomicReference<NaviDebugIntent.AppliedContext?>(null)

    fun noteProfile(profile: String) {
        if (!debugBuild()) return
        lastProfile.set(profile.ifBlank { "unknown" })
    }

    fun noteDebugContext(ctx: NaviDebugIntent.AppliedContext) {
        if (!debugBuild()) return
        lastDebugContext.set(ctx)
        if (ctx.profile.isNotBlank() && ctx.profile != "default") {
            lastProfile.set(ctx.profile)
        }
    }

    fun dump(result: CorridorRouteResult) {
        if (!debugBuild()) return
        val payload =
            runCatching { buildPayload(result) }.getOrElse { err ->
                Log.w(TAG, "build_failed: ${err.message}")
                return
            }
        emitChunked(payload)
    }

    /** SHA-256 hex of [routePolyline] — shared with the host dump harness. */
    fun edgeHash(routePolyline: String): String = sha256Hex(routePolyline)

    private fun buildPayload(result: CorridorRouteResult): String {
        val maneuvers =
            runCatching {
                JSONArray(result.maneuversJson.ifBlank { "[]" })
            }.getOrElse { JSONArray() }
        val packHit =
            Regex("""(?:^|[;\s])pack_hit=([^\s;]+)""")
                .find(result.report)
                ?.groupValues
                ?.getOrNull(1)
                ?.equals("true", ignoreCase = true) == true
        val dbg = lastDebugContext.get()
        val overnightStops = overnightStopsFromResult(result)
        val ferryDiag = ferryDiagnostics(result, dbg)
        val obj =
            JSONObject().apply {
                put("profile", lastProfile.get())
                dbg?.bikeCapability?.let { put("bike_capability", it) }
                put("pack_hit", packHit)
                put(
                    "graph",
                    dbg?.graph
                        ?: if (packHit) {
                            "pack-hit"
                        } else {
                            "local-pbf"
                        },
                )
                put("distance_km", result.distanceKm)
                put("eta_minutes", result.etaMinutes)
                put("edge_hash", edgeHash(result.routePolyline))
                put("maneuvers", maneuvers)
                put("overnight_stops", overnightStops)
                put("route_uses_ferry", ferryDiag.opt("route_uses_ferry"))
                put("no_route_without_ferry", ferryDiag.opt("no_route_without_ferry"))
                if (dbg != null) {
                    val viasArr = JSONArray()
                    for ((lat, lon) in dbg.vias) {
                        viasArr.put(
                            JSONObject().apply {
                                put("lat", lat)
                                put("lon", lon)
                            },
                        )
                    }
                    put("vias", viasArr)
                    dbg.avoidFerries?.let { put("avoid_ferries", it) }
                    val settingsObj = JSONObject()
                    for ((k, v) in dbg.appliedSettings) {
                        settingsObj.put(k, v)
                    }
                    put("settings_applied", settingsObj)
                    if (dbg.ignoredSettings.isNotEmpty()) {
                        put("settings_ignored", JSONArray(dbg.ignoredSettings))
                    }
                }
            }
        return obj.toString()
    }

    /**
     * Hiking overnight stops for the dump. Missing baseline fields become JSON null.
     */
    private fun overnightStopsFromResult(result: CorridorRouteResult): JSONArray {
        val out = JSONArray()
        val days =
            runCatching { JSONArray(result.daysJson.ifBlank { "[]" }) }.getOrElse { JSONArray() }
        for (i in 0 until days.length()) {
            val d = days.optJSONObject(i) ?: continue
            val name = d.optString("overnight_name").takeIf { it.isNotBlank() } ?: continue
            out.put(
                JSONObject().apply {
                    put("osm_id", if (d.has("osm_id") && !d.isNull("osm_id")) d.opt("osm_id") else JSONObject.NULL)
                    put("name", name)
                    put(
                        "category",
                        when {
                            d.has("category") && !d.isNull("category") -> d.opt("category")
                            d.has("rest_kind") && d.optString("rest_kind").isNotBlank() ->
                                d.optString("rest_kind")
                            else -> JSONObject.NULL
                        },
                    )
                    put(
                        "membership_required",
                        if (d.has("membership_required") && !d.isNull("membership_required")) {
                            d.opt("membership_required")
                        } else {
                            JSONObject.NULL
                        },
                    )
                    put(
                        "cabin_class",
                        if (d.has("cabin_class") && !d.isNull("cabin_class")) {
                            d.optString("cabin_class")
                        } else {
                            JSONObject.NULL
                        },
                    )
                    put("lat", if (d.has("lat")) d.opt("lat") else JSONObject.NULL)
                    put("lon", if (d.has("lon")) d.opt("lon") else JSONObject.NULL)
                    put("day_index", d.opt("day_index"))
                },
            )
        }
        // Also harvest overnight pins from break_pois when days_json lacked them.
        if (out.length() == 0) {
            val breaks =
                runCatching { JSONArray(result.breakPoisJson.ifBlank { "[]" }) }
                    .getOrElse { JSONArray() }
            for (i in 0 until breaks.length()) {
                val b = breaks.optJSONObject(i) ?: continue
                if (!b.optBoolean("overnight", false)) continue
                out.put(
                    JSONObject().apply {
                        put(
                            "osm_id",
                            if (b.has("osm_id") && !b.isNull("osm_id")) b.opt("osm_id") else JSONObject.NULL,
                        )
                        put("name", b.optString("name"))
                        put(
                            "category",
                            b.optString("kind").takeIf { it.isNotBlank() } ?: JSONObject.NULL,
                        )
                        put(
                            "membership_required",
                            if (b.has("membership_required")) b.opt("membership_required") else JSONObject.NULL,
                        )
                        put(
                            "cabin_class",
                            if (b.has("cabin_class")) b.optString("cabin_class") else JSONObject.NULL,
                        )
                        put("lat", b.opt("lat"))
                        put("lon", b.opt("lon"))
                    },
                )
            }
        }
        return out
    }

    /**
     * Ferry diagnostics. Absent tokens on older (baseline) reports → JSON null.
     */
    private fun ferryDiagnostics(
        result: CorridorRouteResult,
        dbg: NaviDebugIntent.AppliedContext?,
    ): JSONObject {
        val report = result.report
        val uses =
            Regex("""(?:^|[;\s])route_uses_ferry=(true|false)""")
                .find(report)
                ?.groupValues
                ?.getOrNull(1)
                ?.toBooleanStrictOrNull()
        val avoid = dbg?.avoidFerries
        val failed =
            result.distanceKm <= 0.0 ||
                report.lineSequence().any { it.startsWith("FAIL") } ||
                report.contains("terminate=disconnected") ||
                report.contains("search_terminate_reason=disconnected")
        val noWithout =
            when {
                avoid == true && failed &&
                    (
                        report.contains("no route without ferry", ignoreCase = true) ||
                            report.contains("no_route_without_ferry=true") ||
                            report.contains("ferry", ignoreCase = true)
                    ) -> true
                avoid == true && failed && report.contains("disconnected") -> true
                avoid == null && !report.contains("no_route_without_ferry=") -> null
                else -> false
            }
        return JSONObject().apply {
            put("route_uses_ferry", uses ?: JSONObject.NULL)
            put(
                "no_route_without_ferry",
                when (noWithout) {
                    null -> JSONObject.NULL
                    else -> noWithout
                },
            )
        }
    }

    private fun emitChunked(payload: String) {
        val total = (payload.length + CHUNK_CHARS - 1) / CHUNK_CHARS
        if (total <= 1) {
            Log.i(TAG, "1/1 $payload")
            return
        }
        var part = 1
        var offset = 0
        while (offset < payload.length) {
            val end = minOf(offset + CHUNK_CHARS, payload.length)
            Log.i(TAG, "$part/$total ${payload.substring(offset, end)}")
            offset = end
            part++
        }
    }

    private fun sha256Hex(input: String): String {
        val digest =
            MessageDigest
                .getInstance("SHA-256")
                .digest(input.toByteArray(Charsets.UTF_8))
        val sb = StringBuilder(digest.size * 2)
        for (b in digest) {
            sb.append("%02x".format(b))
        }
        return sb.toString()
    }

    /**
     * True only for debuggable APKs (debug builds). Release installs clear
     * [ApplicationInfo.FLAG_DEBUGGABLE], so this never logs in release.
     */
    private fun debugBuild(): Boolean {
        return runCatching {
            val at = Class.forName("android.app.ActivityThread")
            val app = at.getMethod("currentApplication").invoke(null) as? Context ?: return false
            (app.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) != 0
        }.getOrDefault(false)
    }
}
