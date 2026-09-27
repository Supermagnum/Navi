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
