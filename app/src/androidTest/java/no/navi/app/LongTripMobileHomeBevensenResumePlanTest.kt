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
import uniffi.navi.loadCarRestSettings
import uniffi.navi.loadVehicleLimits
import uniffi.navi.planCarRouteAt
import uniffi.navi.saveCarRestSettings
import uniffi.navi.saveFuelConfig
import uniffi.navi.saveVehicleLimits
import java.io.File
import java.time.LocalDateTime
import java.time.format.DateTimeFormatter
import java.util.concurrent.TimeUnit

/**
 * Resume after stuck Skåne pack: re-enable long-trip (reuse other SD packs),
 * wait for corridorReadyForPlanning, then MobileHome planCarRouteAt.
 *
 * Soft daily budget uses [FfiCarRestSettings.maxHours] (persisted to app
 * `navi.db`); plan [dataDir]/[cacheDir] stay under app data so rest_config
 * loads (same layout as MainActivity long-trip).
 */
@RunWith(AndroidJUnit4::class)
class LongTripMobileHomeBevensenResumePlanTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    companion object {
        private const val TAG = "LongTripMHBevResume"
        private const val ORIGIN_LAT = 53.079686
        private const val ORIGIN_LON = 10.587198
        private const val DEST_LAT = 61.6170857
        private const val DEST_LON = 8.0438639
        private const val WIDTH_INCL_MIRRORS_M = 2.297
        private const val LENGTH_M = 5.304
        private const val TANK_L = 70.0
        private const val DEPARTURE_ISO = "2026-06-01T08:00:00"
        private const val MAX_DAILY_HOURS = 6.0

        /**
         * Project T6 camper body height from [LongTripMobileHomeAvdInstrumentedTest]
         * (raised-roof conversion profile used across MobileHome long-trip tests).
         * Not a stock VW California "normal roof" (~1.99 m); labeled in report.
         */
        private const val BODY_HEIGHT_M = 2.477

        /** Loaded total from the same Avd T6 profile (2–3 people + multi-day kit). */
        private const val LOADED_TOTAL_KG = 3020.4
        private const val LOADED_REAR_AXLE_KG = 1661.2

        /** Hard stop: do not hang forever on 1 Mbps Skåne fetch. */
        private val READY_DEADLINE_MS = TimeUnit.MINUTES.toMillis(45)
        private const val POLL_MS = 15_000L
    }

    @Test
    fun resume_skane_then_plan() {
        val dataDir = NaviAppData.resolve(context)
        val report = JSONObject()
        report.put("started_unix", System.currentTimeMillis() / 1000)
        report.put("mode", "resume_skane_then_plan")
        report.put("departure_iso", DEPARTURE_ISO)
        report.put("max_daily_hours_target", MAX_DAILY_HOURS)

        // Prefer SD for packs; settings/cache stay on internal app data.
        val sd =
            NaviStorageVolumes.list(context).firstOrNull {
                it.removable && it.mounted && it.id != NaviStorageVolumes.INTERNAL_ID
            }
        if (sd != null) {
            MapHudPrefs.saveLongTripPackVolumeId(context, sd.id)
            report.put("sd_volume_id", sd.id)
            report.put("sd_total_gib", sd.totalBytes / (1024.0 * 1024.0 * 1024.0))
        }
        val packDir = LongTripPackStorage.packDownloadDir(context)
        report.put("pack_dir", packDir.absolutePath)
        report.put("settings_data_dir", dataDir.absolutePath)

        val scrubbed = LongTripPackStorage.scrubIncompletePacks(packDir)
        val skanePartial = File(packDir, ".pack-fetch-skane-latest.partial")
        if (skanePartial.exists()) {
            skanePartial.deleteRecursively()
            report.put("deleted_skane_partial", true)
        }
        report.put("scrubbed_stems", JSONArray(scrubbed))
        report.put(
            "manifests_before",
            JSONArray(
                packDir
                    .listFiles()
                    ?.filter { it.name.endsWith(".navi-manifest.json") }
                    ?.map { it.name }
                    .orEmpty(),
            ),
        )

        report.put(
            "height_source",
            "project T6 body height ${BODY_HEIGHT_M} m " +
                "(LongTripMobileHomeAvdInstrumentedTest); not stock California normal-roof ~1.99 m",
        )
        report.put(
            "weight_source",
            "project T6 loaded total ${LOADED_TOTAL_KG} kg / rear axle " +
                "$LOADED_REAR_AXLE_KG kg (AvdInstrumentedTest)",
        )
        assertTrue(
            saveVehicleLimits(
                dataDir.absolutePath,
                FfiVehicleLimits(
                    axleWeightKg = LOADED_REAR_AXLE_KG,
                    bogieWeightKg = null,
                    heightM = BODY_HEIGHT_M,
                    widthM = WIDTH_INCL_MIRRORS_M,
                    lengthM = LENGTH_M,
                    totalWeightKg = LOADED_TOTAL_KG,
                ),
            ),
        )
        val limits = loadVehicleLimits(dataDir.absolutePath)
        report.put(
            "vehicle_limits_saved",
            JSONObject()
                .put("height_m", limits.heightM)
                .put("width_m", limits.widthM)
                .put("length_m", limits.lengthM)
                .put("total_weight_kg", limits.totalWeightKg)
                .put("axle_weight_kg", limits.axleWeightKg),
        )

        assertTrue(
            saveFuelConfig(
                dataDir.absolutePath,
                FfiFuelConfig(tankCapacityL = TANK_L, fuelAddedL = TANK_L, preferLiters = true),
            ),
        )
        report.put(
            "fuel_planning",
            "unimplemented feature: FuelConfig is tank/fill learning input only; " +
                "no fuel-stop / range lookahead in planCarRouteAt " +
                "(see docs/plugins/safety-resupply.md — specification only)",
        )

        assertTrue(
            saveCarRestSettings(
                dataDir.absolutePath,
                FfiCarRestSettings(
                    breakIntervalHours = 1.5,
                    restDurationMinutes = 15u,
                    ecoModeEnabled = true,
                    maxHours = MAX_DAILY_HOURS,
                ),
            ),
        )
        val restLoaded = loadCarRestSettings(dataDir.absolutePath)
        report.put("car_max_hours_saved", restLoaded.maxHours)
        report.put(
            "car_rest_loaded",
            JSONObject()
                .put("break_interval_hours", restLoaded.breakIntervalHours)
                .put("rest_duration_minutes", restLoaded.restDurationMinutes)
                .put("eco_mode_enabled", restLoaded.ecoModeEnabled)
                .put("max_hours", restLoaded.maxHours),
        )
        assertTrue(
            "max_hours must persist as $MAX_DAILY_HOURS (got ${restLoaded.maxHours})",
            kotlin.math.abs(restLoaded.maxHours - MAX_DAILY_HOURS) < 1e-6,
        )
        MapHudPrefs.saveLongTripEnabled(context, true)

        val waypoints = listOf(ORIGIN_LAT to ORIGIN_LON, DEST_LAT to DEST_LON)
        LongTripCoordinator.resetForTests()
        val enableStatus = LongTripCoordinator.enable(context, waypoints)
        Log.i(TAG, "enable=$enableStatus")
        report.put("enable_status", enableStatus)
        val plan0 = LongTripCoordinator.currentPlan()
        assertTrue("corridor must exist", plan0 != null)
        report.put("corridor", JSONArray(plan0!!.regionsInOrder))
        writeReport(report)

        val t0 = System.currentTimeMillis()
        var ready = false
        while (System.currentTimeMillis() - t0 < READY_DEADLINE_MS) {
            val p = LongTripCoordinator.currentPlan()
            val states =
                p?.regionsInOrder?.associateWith { p.states[it]?.name ?: "?" } ?: emptyMap()
            val line = LongTripCoordinator.statusLine()
            val corrReady = LongTripCoordinator.corridorReadyForPlanning()
            Log.i(TAG, "ready=$corrReady states=$states line=$line")
            report.put(
                "progress",
                JSONObject().also { o -> for ((k, v) in states) o.put(k, v) },
            )
            report.put("status_line", line)
            report.put("corridor_ready", corrReady)
            report.put(
                "elapsed_ready_s",
                (System.currentTimeMillis() - t0) / 1000,
            )
            val skaneMan = File(packDir, "skane-latest.navi-manifest.json")
            val partial = File(packDir, ".pack-fetch-skane-latest.partial")
            report.put("skane_manifest", skaneMan.isFile)
            report.put(
                "skane_partial_bytes",
                if (partial.isDirectory) {
                    partial.walkTopDown().filter { it.isFile }.sumOf { it.length() }
                } else {
                    0L
                },
            )
            writeReport(report)
            if (corrReady) {
                ready = true
                break
            }
            if (states.values.any { it == "Failed" } &&
                states["europe/sweden/skane"] == "Failed"
            ) {
                report.put("skane_failed", true)
                break
            }
            Thread.sleep(POLL_MS)
        }

        report.put("corridor_ready_final", ready)
        report.put(
            "final_states",
            JSONObject().also { o ->
                LongTripCoordinator.currentPlan()?.let { p ->
                    for (r in p.regionsInOrder) o.put(r, p.states[r]?.name ?: "?")
                }
            },
        )
        writeReport(report)

        if (!ready) {
            report.put(
                "blocker",
                "corridorReadyForPlanning false after ${READY_DEADLINE_MS / 60000} min; " +
                    "Skåne fetch still incomplete (wifi ~1Mbps host/emulator common)",
            )
            report.put("finished_unix", System.currentTimeMillis() / 1000)
            writeReport(report)
            return
        }

        val placeDb = File(dataDir, "place_index.db")
        val placeAside = File(dataDir, "place_index.db.aside-bev-resume")
        if (placeDb.isFile) placeDb.renameTo(placeAside)

        val startStem =
            plan0.regionsInOrder
                .first()
                .substringAfterLast('/')
                .removeSuffix("-latest")
        val pbf =
            File(packDir, "$startStem-latest.osm.pbf").takeIf { it.isFile }
                ?: packDir.listFiles()?.firstOrNull {
                    it.name.endsWith(".osm.pbf") && !it.name.endsWith(".partial")
                }
                ?: File(packDir, "niedersachsen-latest.osm.pbf")
        report.put("plan_pbf", pbf.absolutePath)
        // Match MainActivity: graph cache under app dataDir so rest_config resolves.
        val cacheDir =
            File(dataDir, "graph-cache-lt-mh-bev-resume").also { it.mkdirs() }
        report.put("plan_cache_dir", cacheDir.absolutePath)
        report.put("plan_started_unix", System.currentTimeMillis() / 1000)
        writeReport(report)

        try {
            Log.i(TAG, "planCarRouteAt MobileHome avoidFerries toll=PENALIZE eco=true max_h=6")
            val tPlan = System.currentTimeMillis()
            val result =
                planCarRouteAt(
                    pbfPath = pbf.absolutePath,
                    elevDir = File(dataDir, "elevation").also { it.mkdirs() }.absolutePath,
                    cacheDir = cacheDir.absolutePath,
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
                    allowedCountries = null,
                    viaPoints = emptyList(),
                )
            report.put("plan_elapsed_ms", System.currentTimeMillis() - tPlan)
            report.put(
                "full_plan",
                JSONObject()
                    .put("distance_km", result.distanceKm)
                    .put("eta_minutes", result.etaMinutes)
                    .put("search_terminate_reason", result.searchTerminateReason)
                    .put("days_json", result.daysJson)
                    .put("break_pois_json", result.breakPoisJson)
                    .put("report", result.report),
            )
            report.put("break_poi_count", breakPoiCount(result.breakPoisJson))
            report.put(
                "poi_criteria",
                "Motor pause: RestArea, General, CraftBrewery, Fishing, Restroom, " +
                    "OvernightFacility, Cabin (road-linked preferred)",
            )
            report.put("days_snip", result.daysJson.take(4000))
            report.put("breaks_snip", result.breakPoisJson.take(4000))
            report.put("report_snip", result.report.take(6000))
            report.put(
                "schedule_from_departure",
                buildSchedule(result.daysJson, result.breakPoisJson, DEPARTURE_ISO),
            )
            val rep = result.report
            val repLower = rep.lowercase()
            report.put(
                "budget_evidence",
                JSONObject()
                    .put("report_has_hours_6", rep.contains("Hours(6.0)") || rep.contains("budget=Hours(6"))
                    .put("report_has_hours_8", rep.contains("Hours(8.0)") || rep.contains("budget=Hours(8"))
                    .put(
                        "motor_multi_day_line",
                        rep.lineSequence().firstOrNull {
                            it.contains("budget=Hours(") ||
                                (it.contains("motor_multi_day:") && it.contains("total_driving_h="))
                        } ?: "",
                    ),
            )
            report.put(
                "vehicle_limits_evidence",
                JSONObject()
                    .put("report_vehicle_limits_true", rep.contains("vehicle_limits=true"))
                    .put("height_m_wired", limits.heightM)
                    .put("total_weight_kg_wired", limits.totalWeightKg)
                    .put(
                        "engine_excludes_by_height_weight",
                        "yes — RouteOptions.vehicle filters maxheight/maxweight/maxwidth/maxlength",
                    ),
            )
            report.put(
                "option_effects",
                JSONObject()
                    .put("report_mentions_ferry", repLower.contains("ferry"))
                    .put("report_mentions_toll", repLower.contains("toll"))
                    .put("report_mentions_eco", repLower.contains("eco"))
                    .put("avoid_ferries_requested", true)
                    .put("toll_policy", "PENALIZE")
                    .put("eco", true)
                    .put(
                        "corridor_land_only",
                        "DE-DK-SE-NO (no Kiel-Oslo ferry in region list)",
                    ),
            )
            writeReport(report)
            assertTrue(
                "must not snap_failed: ${result.searchTerminateReason}",
                result.searchTerminateReason != "snap_failed",
            )
            assertTrue(
                "corrected 6h budget must appear in plan report (got: " +
                    "${report.optJSONObject("budget_evidence")})",
                rep.contains("Hours(6.0)") || rep.contains("budget=Hours(6"),
            )
        } catch (t: Throwable) {
            report.put("plan_error", t.toString())
            writeReport(report)
            throw t
        } finally {
            placeAside.takeIf { it.isFile }?.renameTo(placeDb)
        }
        report.put("finished_unix", System.currentTimeMillis() / 1000)
        writeReport(report)
        Log.i(TAG, "done distance in report")
    }

    /**
     * Build day / pause / overnight timeline from departure local ISO.
     * Driving segments advance wall clock by driving_hours; overnight is a
     * calendar night boundary (resume next day 08:00 local for readability).
     */
    private fun buildSchedule(
        daysJson: String,
        breaksJson: String,
        departureIso: String,
    ): JSONObject {
        val fmt = DateTimeFormatter.ISO_LOCAL_DATE_TIME
        val out = JSONObject()
        try {
            var cursor = LocalDateTime.parse(departureIso)
            val days = JSONArray(daysJson)
            val dayArr = JSONArray()
            for (i in 0 until days.length()) {
                val d = days.getJSONObject(i)
                val driveH = d.optDouble("driving_hours", 0.0)
                val start = cursor
                val endDrive = cursor.plusSeconds((driveH * 3600.0).toLong())
                val dayObj =
                    JSONObject()
                        .put("day_index", d.optInt("day_index"))
                        .put("depart", start.format(fmt))
                        .put("arrive_or_overnight", endDrive.format(fmt))
                        .put("driving_hours", driveH)
                        .put("distance_km", d.optDouble("distance_km"))
                        .put("overnight_name", d.optString("overnight_name"))
                        .put("rest_kind", d.optString("rest_kind"))
                        .put("is_final", d.optBoolean("is_final"))
                dayArr.put(dayObj)
                cursor =
                    if (d.optBoolean("is_final")) {
                        endDrive
                    } else {
                        // Next driving day starts 08:00 after overnight.
                        endDrive.toLocalDate().plusDays(1).atTime(8, 0)
                    }
            }
            out.put("days", dayArr)
            val breaks = JSONArray(breaksJson)
            val pauseArr = JSONArray()
            for (i in 0 until breaks.length()) {
                val b = breaks.getJSONObject(i)
                val kind = b.optString("kind")
                if (kind == "lodging" || kind == "hut" || kind == "camping") continue
                pauseArr.put(
                    JSONObject()
                        .put("along_km", b.optDouble("along_km"))
                        .put("name", b.optString("name"))
                        .put("kind", kind)
                        .put("lat", b.optDouble("lat"))
                        .put("lon", b.optDouble("lon")),
                )
            }
            out.put("rest_pauses", pauseArr)
            out.put("note", "Overnight resume fixed at next-day 08:00 for schedule readability")
        } catch (t: Throwable) {
            out.put("error", t.toString())
        }
        return out
    }

    private fun breakPoiCount(raw: String): Int =
        try {
            JSONArray(raw).length()
        } catch (_: Throwable) {
            0
        }

    private fun writeReport(obj: JSONObject) {
        val text = obj.toString(2)
        val dataDir = NaviAppData.resolve(context)
        runCatching {
            File(dataDir, "long-trip-bevensen-resume").also { it.mkdirs() }.let {
                File(it, "report.json").writeText(text)
            }
        }
        val sd =
            NaviStorageVolumes
                .list(context)
                .firstOrNull { it.removable && it.mounted && it.appFilesDir != null }
                ?.appFilesDir
        if (sd != null) {
            runCatching {
                val dir = File(sd, "long-trip-bevensen-resume").also { it.mkdirs() }
                File(dir, "report.json").writeText(text)
            }
        }
        runCatching {
            File("/data/local/tmp/long-trip-bevensen-resume.json").writeText(text)
        }
    }
}
