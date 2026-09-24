package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiCarRestSettings
import uniffi.navi.FfiFuelConfig
import uniffi.navi.FfiTollPolicy
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.loadVehicleLimits
import uniffi.navi.planCarRouteAt
import uniffi.navi.saveCarRestSettings
import uniffi.navi.saveFuelConfig
import uniffi.navi.saveVehicleLimits
import java.io.File
import java.util.concurrent.TimeUnit

/**
 * Live MobileHome long-trip: Bad Bevensen (Kurpark Stellplatz) → Norway coords.
 * Evidence written under app data + /data/local/tmp for adb pull.
 */
@RunWith(AndroidJUnit4::class)
class LongTripMobileHomeBevensenLiveTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    companion object {
        private const val TAG = "LongTripMHBevensen"

        // Wohnmobilstellplatz am Kurpark Bad Bevensen (prior Nominatim)
        private const val ORIGIN_LAT = 53.079686
        private const val ORIGIN_LON = 10.587198
        private const val DEST_LAT = 61.6170857
        private const val DEST_LON = 8.0438639
        private const val WIDTH_INCL_MIRRORS_M = 2.297
        private const val WIDTH_EXCL_MIRRORS_M = 1.904
        private const val LENGTH_M = 5.304
        private const val TANK_L = 70.0
        private const val DEPARTURE_ISO = "2026-06-01T08:00:00"
        private val DOWNLOAD_DEADLINE_MS = TimeUnit.HOURS.toMillis(16)
        private val PLAN_DEADLINE_MS = TimeUnit.HOURS.toMillis(4)
    }

    @Test
    fun live_mobilehome_bevensen_to_norway() {
        val dataDir = NaviAppData.resolve(context)
        val outDir = File(dataDir, "long-trip-bevensen-live").also { it.mkdirs() }
        val report = JSONObject()
        report.put("started_unix", System.currentTimeMillis() / 1000)
        report.put("head_note", "long-trip branch live MobileHome Bevensen→Norway")
        report.put("origin", "$ORIGIN_LAT,$ORIGIN_LON")
        report.put("dest", "$DEST_LAT,$DEST_LON")
        report.put("departure_iso", DEPARTURE_ISO)

        // --- Vehicle: accept length + width (incl mirrors). Height/weight FLAG missing. ---
        val heightFlag =
            "FLAG missing input: normal-roof T6 body height not provided; " +
                "low-clearance height_m left null (no guess)."
        val weightFlag =
            "FLAG missing input: loaded kg for 2–3 people + 4–5 days consumables " +
                "not quantified; totalWeightKg/axleWeightKg left null."
        report.put("height_flag", heightFlag)
        report.put("weight_flag", weightFlag)
        report.put("width_m_used", WIDTH_INCL_MIRRORS_M)
        report.put("width_excl_mirrors_m_on_file", WIDTH_EXCL_MIRRORS_M)
        report.put("length_m_used", LENGTH_M)
        report.put(
            "vehicle_acceptance",
            "accepted length_m=$LENGTH_M width_m=$WIDTH_INCL_MIRRORS_M (incl mirrors); " +
                "height/weight omitted per missing input flags",
        )
        assertTrue(
            saveVehicleLimits(
                dataDir.absolutePath,
                FfiVehicleLimits(
                    axleWeightKg = null,
                    bogieWeightKg = null,
                    heightM = null,
                    widthM = WIDTH_INCL_MIRRORS_M,
                    lengthM = LENGTH_M,
                    totalWeightKg = null,
                ),
            ),
        )
        val loaded = loadVehicleLimits(dataDir.absolutePath)
        report.put(
            "vehicle_limits_loaded",
            JSONObject()
                .put("height_m", loaded.heightM)
                .put("width_m", loaded.widthM)
                .put("length_m", loaded.lengthM)
                .put("total_weight_kg", loaded.totalWeightKg)
                .put("axle_weight_kg", loaded.axleWeightKg),
        )

        assertTrue(
            saveFuelConfig(
                dataDir.absolutePath,
                FfiFuelConfig(tankCapacityL = TANK_L, fuelAddedL = TANK_L, preferLiters = true),
            ),
        )
        // Soft break 1.5 h + eco; daily max_hours via FfiCarRestSettings.
        assertTrue(
            saveCarRestSettings(
                dataDir.absolutePath,
                FfiCarRestSettings(1.5, 15u, ecoModeEnabled = true, maxHours = 6.0),
            ),
        )
        report.put("car_rest_break_interval_h", 1.5)
        report.put("car_rest_eco", true)
        report.put("car_max_hours_target", 6.0)
        report.put("car_max_hours_saved", 6.0)
        report.put(
            "daily_limit_capability",
            "MobileHome uses CarRestParams; FfiCarRestSettings.maxHours persists rest_config.car.max_hours",
        )

        // Prefer removable SD for long-trip packs
        val volumes = NaviStorageVolumes.list(context)
        report.put(
            "volumes",
            JSONArray().also { arr ->
                for (v in volumes) {
                    arr.put(
                        JSONObject()
                            .put("id", v.id)
                            .put("label", v.label)
                            .put("removable", v.removable)
                            .put("mounted", v.mounted)
                            .put("total_bytes", v.totalBytes)
                            .put("free_bytes", v.freeBytes)
                            .put("app_files", v.appFilesDir?.absolutePath),
                    )
                }
            },
        )
        val sd =
            volumes.firstOrNull {
                it.removable && it.mounted && it.id != NaviStorageVolumes.INTERNAL_ID
            }
        if (sd != null) {
            MapHudPrefs.saveLongTripPackVolumeId(context, sd.id)
            report.put("sd_volume_id", sd.id)
            report.put("sd_label", sd.label)
            report.put("sd_total_bytes", sd.totalBytes)
            report.put("sd_total_gib", sd.totalBytes / (1024.0 * 1024.0 * 1024.0))
            report.put("sd_app_files", sd.appFilesDir?.absolutePath)
        } else {
            report.put("sd_volume_id", JSONObject.NULL)
            report.put(
                "sd_warning",
                "No removable volume visible to NaviStorageVolumes; packs may land internal",
            )
        }
        MapHudPrefs.saveLongTripEnabled(context, true)

        val packDirBefore = LongTripPackStorage.packDownloadDir(context)
        report.put("pack_dir", packDirBefore.absolutePath)
        report.put(
            "pack_on_removable",
            sd != null &&
                packDirBefore.absolutePath.contains(
                    sd.appFilesDir?.absolutePath ?: "___",
                ),
        )

        val waypoints = listOf(ORIGIN_LAT to ORIGIN_LON, DEST_LAT to DEST_LON)
        LongTripCoordinator.resetForTests()
        val status = LongTripCoordinator.enable(context, waypoints)
        Log.i(TAG, "longTrip enable=$status")
        report.put("enable_status", status)
        val plan = LongTripCoordinator.currentPlan()
        assertTrue("corridor must exist: $status", plan != null)
        report.put("corridor", JSONArray(plan!!.regionsInOrder))
        report.put("corridor_start_region", plan.regionsInOrder.firstOrNull())
        writeReport(outDir, report)

        val t0 = System.currentTimeMillis()
        var lastStates: Map<String, String> = emptyMap()
        while (System.currentTimeMillis() - t0 < DOWNLOAD_DEADLINE_MS) {
            val p = LongTripCoordinator.currentPlan() ?: break
            lastStates = p.regionsInOrder.associateWith { p.states[it]?.name ?: "?" }
            val line = LongTripCoordinator.statusLine()
            Log.i(TAG, "states=$lastStates line=$line")
            report.put(
                "progress",
                JSONObject().also { o -> for ((k, v) in lastStates) o.put(k, v) },
            )
            report.put("status_line", line)
            report.put(
                "region_order_states",
                JSONArray().also { arr ->
                    for (r in p.regionsInOrder) {
                        arr.put(
                            JSONObject()
                                .put("region", r)
                                .put("state", p.states[r]?.name ?: "?"),
                        )
                    }
                },
            )
            writeReport(outDir, report)
            if (lastStates.values.all { it == "Indexed" }) break
            if (lastStates.values.any { it == "Failed" } &&
                lastStates.values.all { it == "Indexed" || it == "Failed" }
            ) {
                break
            }
            Thread.sleep(20_000)
        }
        report.put(
            "final_states",
            JSONObject().also { o -> for ((k, v) in lastStates) o.put(k, v) },
        )
        val packDir = LongTripPackStorage.packDownloadDir(context)
        report.put("pack_dir_final", packDir.absolutePath)
        report.put(
            "pack_files",
            JSONArray(
                packDir.listFiles()?.map { "${it.name}:${it.length()}" }.orEmpty(),
            ),
        )
        writeReport(outDir, report)

        if (!lastStates.values.all { it == "Indexed" }) {
            report.put("plan_skipped", "not all regions Indexed")
            report.put("finished_unix", System.currentTimeMillis() / 1000)
            writeReport(outDir, report)
            mirrorTmp(report)
            return
        }

        // Keep place index out of page cache on 4 GB Automotive (same as MH AVD test).
        val placeDb = File(dataDir, "place_index.db")
        val placeAside = File(dataDir, "place_index.db.aside-bevensen")
        if (placeDb.isFile) placeDb.renameTo(placeAside)

        val pbfCandidates =
            packDir
                .listFiles()
                ?.filter {
                    it.isFile && it.name.endsWith(".osm.pbf") && !it.name.endsWith(".partial")
                }.orEmpty()
        val startStem =
            plan.regionsInOrder
                .firstOrNull()
                ?.substringAfterLast('/')
                ?.removeSuffix("-latest")
                .orEmpty()
        val pbf =
            pbfCandidates.firstOrNull { it.name.contains(startStem) }
                ?: pbfCandidates.firstOrNull()
                ?: File(dataDir, "$startStem-latest.osm.pbf")
        report.put("plan_pbf", pbf.absolutePath)
        report.put("plan_started_unix", System.currentTimeMillis() / 1000)
        writeReport(outDir, report)

        try {
            Log.i(TAG, "planning MobileHome avoidFerries+tolls eco packDir=$packDir pbf=$pbf")
            val planStart = System.currentTimeMillis()
            val result =
                planCarRouteAt(
                    pbfPath = pbf.absolutePath,
                    elevDir = File(dataDir, "elevation").also { it.mkdirs() }.absolutePath,
                    cacheDir = File(dataDir, "graph-cache-lt-mh-bev").also { it.mkdirs() }.absolutePath,
                    startLat = ORIGIN_LAT,
                    startLon = ORIGIN_LON,
                    endLat = DEST_LAT,
                    endLon = DEST_LON,
                    useEco = true,
                    profile = TravelProfile.MOBILE_HOME,
                    avoidMotorways = false,
                    tollPolicy = FfiTollPolicy.PENALIZE,
                    avoidFerries = true,
                    avoidTunnels = false,
                    vehicle = loadVehicleLimits(dataDir.absolutePath),
                    preferOfficialNetworks = false,
                    departureLocalIso = DEPARTURE_ISO,
                    dataDir = dataDir.absolutePath,
                    packDir = packDir.absolutePath,
                    longTripEnabled = true,
                    viaPoints = emptyList(),
                )
            val elapsed = System.currentTimeMillis() - planStart
            report.put("plan_elapsed_ms", elapsed)
            if (elapsed > PLAN_DEADLINE_MS) {
                report.put("plan_deadline_note", "exceeded ${PLAN_DEADLINE_MS}ms soft deadline")
            }
            val breakPois = result.breakPoisJson
            val days = result.daysJson
            report.put(
                "full_plan",
                JSONObject()
                    .put("distance_km", result.distanceKm)
                    .put("eta_minutes", result.etaMinutes)
                    .put("search_terminate_reason", result.searchTerminateReason)
                    .put("days_json", days)
                    .put("break_pois_json", breakPois)
                    .put("report", result.report),
            )
            report.put("break_poi_count", breakPoiCount(breakPois))
            report.put(
                "poi_criteria",
                "Motor pause POIs: RestArea, General, CraftBrewery, Fishing, Restroom, " +
                    "OvernightFacility, Cabin (prefer road-linked; no ferry/lake detour). " +
                    "Spacing from soft break_interval_hours via trip speed.",
            )
            report.put(
                "options_applied",
                JSONObject()
                    .put("eco", true)
                    .put("toll_policy", "PENALIZE")
                    .put("avoid_ferries", true)
                    .put("break_interval_h", 1.5)
                    .put("max_hours_patched", 6.0)
                    .put("profile", "MOBILE_HOME"),
            )
            // Parse overnight / short breaks from days_json if present
            report.put("days_parsed", parseDaysSummary(days))
            report.put("breaks_parsed", parseBreaksSummary(breakPois, days))
            writeReport(outDir, report)
            assertTrue(
                "must not snap_failed: ${result.searchTerminateReason}",
                result.searchTerminateReason != "snap_failed",
            )
        } catch (t: Throwable) {
            report.put("plan_error", t.toString())
            writeReport(outDir, report)
            throw t
        } finally {
            placeAside.takeIf { it.isFile }?.renameTo(placeDb)
        }
        report.put("finished_unix", System.currentTimeMillis() / 1000)
        writeReport(outDir, report)
        mirrorTmp(report)
        Log.i(TAG, "report ${File(outDir, "report.json").absolutePath}")
    }

    private fun breakPoiCount(raw: String): Int =
        try {
            JSONArray(raw).length()
        } catch (_: Throwable) {
            0
        }

    private fun parseDaysSummary(raw: String): JSONArray {
        val out = JSONArray()
        try {
            val arr = JSONArray(raw)
            for (i in 0 until arr.length()) {
                val d = arr.optJSONObject(i) ?: continue
                val keys = ArrayList<String>()
                val it = d.keys()
                while (it.hasNext()) keys.add(it.next())
                out.put(
                    JSONObject()
                        .put("day_index", i)
                        .put("snippet", d.toString().take(400))
                        .put("raw_keys", JSONArray(keys)),
                )
            }
        } catch (_: Throwable) {
            out.put(JSONObject().put("parse_error", true).put("raw_len", raw.length))
        }
        return out
    }

    private fun parseBreaksSummary(
        breakPois: String,
        days: String,
    ): JSONObject {
        val o = JSONObject()
        o.put("break_pois_n", breakPoiCount(breakPois))
        try {
            val arr = JSONArray(breakPois)
            o.put(
                "break_pois",
                JSONArray().also { out ->
                    for (i in 0 until arr.length()) {
                        val b = arr.optJSONObject(i) ?: continue
                        out.put(
                            JSONObject()
                                .put("name", b.optString("name"))
                                .put("lat", b.optDouble("lat"))
                                .put("lon", b.optDouble("lon"))
                                .put("kind", b.optString("kind"))
                                .put("icon", b.optString("icon"))
                                .put("at_local", b.optString("at_local", b.optString("time", ""))),
                        )
                    }
                },
            )
        } catch (_: Throwable) {
            o.put("break_pois_parse_error", true)
        }
        o.put("days_json_len", days.length)
        return o
    }

    private fun writeReport(
        outDir: File,
        obj: JSONObject,
    ) {
        // Prefer removable pack volume (512G SD) — Automotive /data is tiny and
        // fills with pmtiles/place-index during multi-region long-trip.
        val text = obj.toString(2)
        val sdRoot =
            NaviStorageVolumes
                .list(context)
                .firstOrNull { it.removable && it.mounted && it.appFilesDir != null }
                ?.appFilesDir
        if (sdRoot != null) {
            runCatching {
                val dir = File(sdRoot, "long-trip-bevensen-live").also { it.mkdirs() }
                File(dir, "report.json").writeText(text)
            }
        }
        runCatching { File(outDir, "report.json").writeText(text) }
        runCatching {
            context.getExternalFilesDir(null)?.let {
                File(it, "long-trip-bevensen-live.json").writeText(text)
            }
        }
        mirrorTmp(obj)
    }

    private fun mirrorTmp(obj: JSONObject) {
        runCatching {
            File("/data/local/tmp/long-trip-bevensen-live.json").writeText(obj.toString(2))
        }
    }
}
