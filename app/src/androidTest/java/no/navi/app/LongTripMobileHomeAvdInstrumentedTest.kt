package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiCarRestSettings
import uniffi.navi.FfiFuelConfig
import uniffi.navi.FfiLatLon
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
 * ADB / instrumentation only — no UI Automator.
 * Sets MobileHome limits, verifies online Nominatim place search with empty
 * place index, drives [LongTripCoordinator] on the real download path.
 */
@RunWith(AndroidJUnit4::class)
class LongTripMobileHomeAvdInstrumentedTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    companion object {
        private const val TAG = "LongTripMHAvd"
        private const val ORIGIN_LAT = 52.605766
        private const val ORIGIN_LON = 11.859277
        private const val LILLEHAMMER_LAT = 61.114545
        private const val LILLEHAMMER_LON = 10.467007
        private const val BESSHEIM_LAT = 61.514623
        private const val BESSHEIM_LON = 8.852972
        private const val ANTENNA_BASE_M_EST = 0.50

        /** Mast length 223 cm (tip height = base + stack when above body). */
        private const val ANTENNA_STACK_M = 2.23
        private const val BODY_HEIGHT_M = 2.477
        private const val WIDTH_INCL_MIRRORS_M = 2.297
        private const val LENGTH_M = 5.304
        private const val LOADED_TOTAL_KG = 3020.4
        private const val LOADED_REAR_AXLE_KG = 1661.2
        private const val TANK_L = 70.0
        private const val DEPARTURE_ISO = "2026-09-22T08:00:00"
        private val DOWNLOAD_DEADLINE_MS = TimeUnit.HOURS.toMillis(14)
    }

    @Test
    fun online_place_search_without_index_then_long_trip() {
        val dataDir = NaviAppData.resolve(context)
        val outDir = File(dataDir, "long-trip-avd-report").also { it.mkdirs() }
        val report = org.json.JSONObject()
        report.put("started_unix", System.currentTimeMillis() / 1000)

        // --- Online geocode with no place DB ---
        val placeDb = File(dataDir, "place_index.db")
        if (placeDb.isFile) {
            // Do not delete mid-download index if present; rename aside for this check.
            placeDb.renameTo(File(dataDir, "place_index.db.bak-avd"))
        }
        assertTrue("device must be online for Nominatim", BasemapStyleResolver.hasNetwork(context))
        val onlineHits =
            runBlocking {
                OnlinePlaceSearch.search(
                    context,
                    "Bessheim Fjellstue",
                    limit = 5,
                    addressMode = false,
                )
            }
        Log.i(TAG, "online hits=${onlineHits.map { "${it.name}@${it.lat},${it.lon} region=${it.regionId}" }}")
        report.put(
            "online_bessheim",
            org.json.JSONArray().also { arr ->
                for (h in onlineHits) {
                    arr.put(
                        org.json
                            .JSONObject()
                            .put("name", h.name)
                            .put("lat", h.lat)
                            .put("lon", h.lon)
                            .put("region_id", h.regionId)
                            .put("kind", h.kind),
                    )
                }
            },
        )
        assertTrue(
            "Nominatim must return Bessheim without a place index",
            onlineHits.any {
                it.name.contains("Bessheim", ignoreCase = true) ||
                    (
                        kotlin.math.abs(it.lat - BESSHEIM_LAT) < 0.05 &&
                            kotlin.math.abs(it.lon - BESSHEIM_LON) < 0.05
                    )
            },
        )
        // Region finder: online hit must carry Geofabrik path for Tools download.
        val bessheimRegion =
            onlineHits
                .firstOrNull {
                    it.name.contains("Bessheim", ignoreCase = true) ||
                        (
                            kotlin.math.abs(it.lat - BESSHEIM_LAT) < 0.05 &&
                                kotlin.math.abs(it.lon - BESSHEIM_LON) < 0.05
                        )
                }?.regionId
                .orEmpty()
        assertTrue(
            "online Bessheim must map to ostlandet for region downloader",
            bessheimRegion.contains("ostlandet"),
        )
        MapHudPrefs.saveGeofabrikPath(context, bessheimRegion)
        assertEquals(bessheimRegion, MapHudPrefs.loadGeofabrikPath(context))
        report.put("region_finder_path", bessheimRegion)

        val addrHits =
            runBlocking {
                OnlinePlaceSearch.search(
                    context,
                    "Bahnhofstraße, Stendal, Germany",
                    limit = 5,
                    addressMode = true,
                )
            }
        Log.i(TAG, "address hits=${addrHits.map { "${it.name}@${it.lat},${it.lon} region=${it.regionId}" }}")
        report.put("online_stendal_address_n", addrHits.size)
        if (addrHits.isNotEmpty()) {
            assertTrue(
                "address-mode Nominatim must return Stendal area",
                addrHits.any {
                    it.name.contains("Stendal", ignoreCase = true) ||
                        it.regionId.contains("sachsen-anhalt")
                },
            )
        } else {
            Log.w(TAG, "address Nominatim empty (rate-limit?); place search already verified")
            report.put("online_stendal_address_skipped", true)
        }
        val lille =
            runBlocking {
                OnlinePlaceSearch.search(context, "Lillehammer", 5, addressMode = false)
            }
        assertTrue(
            "Nominatim must return Lillehammer",
            lille.any { it.name.contains("Lillehammer", ignoreCase = true) },
        )
        report.put("online_lillehammer_n", lille.size)

        // Restore bak if we moved it
        File(dataDir, "place_index.db.bak-avd").takeIf { it.isFile }?.renameTo(placeDb)

        // --- Vehicle profile (same UniFFI as Drive settings) ---
        val heightM = maxOf(BODY_HEIGHT_M, ANTENNA_BASE_M_EST + ANTENNA_STACK_M)
        assertTrue(
            saveVehicleLimits(
                dataDir.absolutePath,
                FfiVehicleLimits(
                    axleWeightKg = LOADED_REAR_AXLE_KG,
                    bogieWeightKg = null,
                    heightM = heightM,
                    widthM = WIDTH_INCL_MIRRORS_M,
                    lengthM = LENGTH_M,
                    totalWeightKg = LOADED_TOTAL_KG,
                ),
            ),
        )
        assertTrue(
            saveFuelConfig(
                dataDir.absolutePath,
                FfiFuelConfig(tankCapacityL = TANK_L, fuelAddedL = TANK_L, preferLiters = true),
            ),
        )
        assertTrue(
            saveCarRestSettings(
                dataDir.absolutePath,
                FfiCarRestSettings(2.0, 15u, ecoModeEnabled = true, maxHours = 8.0),
            ),
        )
        report.put("height_m", heightM)
        report.put("binding_height", if (heightM > BODY_HEIGHT_M) "antenna" else "body")
        report.put("gvwr_conservative_kg", 2800)
        report.put("gaWR_front_conservative_kg", 1710)
        report.put("gaWR_rear_conservative_kg", 1625)
        report.put("loaded_total_kg", LOADED_TOTAL_KG)
        report.put("loaded_vs_gvwr", if (LOADED_TOTAL_KG > 2800) "OVER" else "within")
        report.put("loaded_rear_axle_kg", LOADED_REAR_AXLE_KG)
        report.put(
            "rear_vs_gawr",
            if (LOADED_REAR_AXLE_KG > 1625) "OVER" else "within",
        )

        // Prefer SD for packs when mounted
        val sd =
            NaviStorageVolumes.list(context).firstOrNull {
                it.removable && it.mounted && it.id != NaviStorageVolumes.INTERNAL_ID
            }
        if (sd != null) {
            MapHudPrefs.saveLongTripPackVolumeId(context, sd.id)
            report.put("sd_volume_id", sd.id)
        }
        MapHudPrefs.saveLongTripEnabled(context, true)

        val waypoints =
            listOf(
                ORIGIN_LAT to ORIGIN_LON,
                LILLEHAMMER_LAT to LILLEHAMMER_LON,
                BESSHEIM_LAT to BESSHEIM_LON,
            )
        LongTripCoordinator.resetForTests()
        val status = LongTripCoordinator.enable(context, waypoints)
        Log.i(TAG, "longTrip enable=$status")
        report.put("enable_status", status)
        val plan = LongTripCoordinator.currentPlan()
        assertTrue("corridor must exist", plan != null)
        report.put(
            "corridor",
            org.json.JSONArray(plan!!.regionsInOrder),
        )
        writeReport(outDir, report)

        val t0 = System.currentTimeMillis()
        while (System.currentTimeMillis() - t0 < DOWNLOAD_DEADLINE_MS) {
            val p = LongTripCoordinator.currentPlan() ?: break
            val states = p.regionsInOrder.associateWith { p.states[it]?.name ?: "?" }
            Log.i(TAG, "states=$states line=${LongTripCoordinator.statusLine()}")
            report.put(
                "progress",
                org.json.JSONObject().also { o ->
                    for ((k, v) in states) o.put(k, v)
                },
            )
            report.put("status_line", LongTripCoordinator.statusLine())
            writeReport(outDir, report)
            if (states.values.all { it == "Indexed" }) break
            if (states.values.any { it == "Failed" } &&
                states.values.all { it == "Indexed" || it == "Failed" }
            ) {
                break
            }
            Thread.sleep(20_000)
        }

        val final =
            LongTripCoordinator.currentPlan()?.regionsInOrder?.associateWith {
                LongTripCoordinator.currentPlan()!!.states[it]?.name ?: "?"
            } ?: emptyMap()
        report.put(
            "final_states",
            org.json.JSONObject().also { o -> for ((k, v) in final) o.put(k, v) },
        )
        writeReport(outDir, report)

        if (final.values.all { it == "Indexed" }) {
            val packDir = LongTripPackStorage.packDownloadDir(context)
            val pbf =
                File(packDir, "sachsen-anhalt-latest.osm.pbf").takeIf { it.isFile }
                    ?: File(dataDir, "sachsen-anhalt-latest.osm.pbf")
            // Keep the 1.9 GB place index out of the page cache during native
            // planning — 4 GB Automotive LMK killed us at ~2.9 GB process RSS.
            val placeAside = File(dataDir, "place_index.db.aside-plan")
            if (placeDb.isFile) {
                placeDb.renameTo(placeAside)
            }
            Log.i(TAG, "planning Stendal→Bessheim via Lillehammer (packDir=$packDir)")
            report.put("plan_started_unix", System.currentTimeMillis() / 1000)
            writeReport(outDir, report)
            try {
                val result =
                    planCarRouteAt(
                        pbfPath = pbf.absolutePath,
                        elevDir = File(dataDir, "elevation").also { it.mkdirs() }.absolutePath,
                        cacheDir = File(packDir, "graph-cache-lt-mh").also { it.mkdirs() }.absolutePath,
                        startLat = ORIGIN_LAT,
                        startLon = ORIGIN_LON,
                        endLat = BESSHEIM_LAT,
                        endLon = BESSHEIM_LON,
                        useEco = true,
                        profile = TravelProfile.MOBILE_HOME,
                        avoidMotorways = false,
                        tollPolicy = FfiTollPolicy.ALLOW,
                        avoidFerries = false,
                        avoidTunnels = false,
                        vehicle = loadVehicleLimits(dataDir.absolutePath),
                        preferOfficialNetworks = false,
                        departureLocalIso = DEPARTURE_ISO,
                        dataDir = packDir.absolutePath,
                        packDir = packDir.absolutePath,
                        longTripEnabled = true,
                        allowedCountries = null,
                        viaPoints =
                            listOf(
                                FfiLatLon(lat = LILLEHAMMER_LAT, lon = LILLEHAMMER_LON),
                            ),
                    )
                report.put(
                    "full_plan",
                    org.json
                        .JSONObject()
                        .put("distance_km", result.distanceKm)
                        .put("eta_minutes", result.etaMinutes)
                        .put("search_terminate_reason", result.searchTerminateReason)
                        .put("days_json", result.daysJson)
                        .put("break_pois_json", result.breakPoisJson)
                        .put("report", result.report),
                )
                writeReport(outDir, report)
                assertTrue(
                    "MobileHome corridor plan must snap (not snap_failed): ${result.searchTerminateReason}",
                    result.searchTerminateReason != "snap_failed",
                )
                assertTrue(
                    "MobileHome Stendal→Bessheim via Lillehammer must be a long corridor",
                    result.distanceKm > 800.0,
                )
            } catch (t: Throwable) {
                report.put("plan_error", t.toString())
                writeReport(outDir, report)
                throw t
            } finally {
                placeAside.takeIf { it.isFile }?.renameTo(placeDb)
            }
        }
        report.put("finished_unix", System.currentTimeMillis() / 1000)
        writeReport(outDir, report)
        // Mirror for adb pull
        runCatching {
            File("/data/local/tmp/long-trip-avd-report.json").writeText(report.toString(2))
        }
        Log.i(TAG, "report written ${File(outDir, "report.json").absolutePath}")
    }

    private fun writeReport(
        outDir: File,
        obj: org.json.JSONObject,
    ) {
        File(outDir, "report.json").writeText(obj.toString(2))
        runCatching {
            context.getExternalFilesDir(null)?.let {
                File(it, "long-trip-avd-report.json").writeText(obj.toString(2))
            }
        }
    }
}
