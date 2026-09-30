package no.navi.app

import android.os.Debug
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.CampingCallKind
import uniffi.navi.FfiCarRestSettings
import uniffi.navi.FfiFuelConfig
import uniffi.navi.FfiLatLon
import uniffi.navi.FfiTollPolicy
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.campingPluginConfigure
import uniffi.navi.campingPluginInstallGuest
import uniffi.navi.campingPluginSetEnabled
import uniffi.navi.campingPluginSetNavContext
import uniffi.navi.campingPluginSetTimezone
import uniffi.navi.campingPluginSuggestAlongRoute
import uniffi.navi.datexRefreshJson
import uniffi.navi.ensurePoiLookaheadLoaded
import uniffi.navi.loadCarRestSettings
import uniffi.navi.loadVehicleLimits
import uniffi.navi.planCarRouteAt
import uniffi.navi.poiLookaheadQueryJson
import uniffi.navi.saveCarRestSettings
import uniffi.navi.saveFuelConfig
import uniffi.navi.saveVehicleLimits
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.util.TimeZone
import java.util.concurrent.TimeUnit

/**
 * One-shot Bevensen → Dalsøren MobileHome campaign (not wired into CI).
 *
 * Real packs on removable SD via [LongTripCoordinator], real plan path, synthetic
 * DATEX closures only. Evidence: app data + /data/local/tmp + SD report mirror.
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
class LongTripMobileHomeBevensenDalsorenCampaignTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    companion object {
        private const val TAG = "BevensenDalsorenCamp"

        private const val ORIGIN_LAT = 53.079686
        private const val ORIGIN_LON = 10.587198
        private const val VIA_LAT = 61.6170857
        private const val VIA_LON = 8.0438639

        // Nominatim: Dalsøren Camping, Luster, Vestland
        private const val DEST_LAT = 61.4433766
        private const val DEST_LON = 7.4614016

        private const val WIDTH_INCL_MIRRORS_M = 2.297
        private const val LENGTH_M = 5.304
        private const val BODY_HEIGHT_M = 2.477
        private const val LOADED_TOTAL_KG = 3020.4
        private const val LOADED_REAR_AXLE_KG = 1661.2
        private const val TANK_L = 70.0
        private const val DEPARTURE_ISO = "2026-06-01T08:00:00"
        private const val MAX_DAILY_HOURS = 6.0

        private val DOWNLOAD_DEADLINE_MS = TimeUnit.HOURS.toMillis(20)
        private val PLAN_DEADLINE_MS = TimeUnit.HOURS.toMillis(6)
        private const val POLL_MS = 20_000L
    }

    @Test
    fun bevensen_via_sognefjell_to_dalsoren_campaign() {
        val dataDir = NaviAppData.resolve(context)
        val report = JSONObject()
        report.put("started_unix", System.currentTimeMillis() / 1000)
        report.put("campaign", "bevensen_dalsoren_mobilehome")
        report.put("branch_note", "right-to-roam")
        report.put("origin", JSONObject().put("lat", ORIGIN_LAT).put("lon", ORIGIN_LON).put("name", "Bad Bevensen Kurpark Stellplatz"))
        report.put("via", JSONObject().put("lat", VIA_LAT).put("lon", VIA_LON).put("name", "Sognefjellsvegen (via)"))
        report.put("dest", JSONObject().put("lat", DEST_LAT).put("lon", DEST_LON).put("name", "Dalsøren Camping"))
        report.put("departure_iso", DEPARTURE_ISO)
        report.put("ram_before_mib", processPssMib())

        // Prefer removable SD for long-trip packs.
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
                it.removable && it.mounted && it.id != NaviStorageVolumes.INTERNAL_ID && it.appFilesDir != null
            }
        if (sd != null) {
            MapHudPrefs.saveLongTripPackVolumeId(context, sd.id)
            report.put("sd_volume_id", sd.id)
            report.put("sd_label", sd.label)
            report.put("sd_total_gib", sd.totalBytes / (1024.0 * 1024.0 * 1024.0))
            report.put("sd_app_files", sd.appFilesDir!!.absolutePath)
        } else {
            report.put("sd_warning", "No removable volume; packs may land internal")
        }
        val packDir = LongTripPackStorage.packDownloadDir(context)
        report.put("pack_dir", packDir.absolutePath)
        report.put("settings_data_dir", dataDir.absolutePath)
        report.put(
            "pack_on_removable",
            sd != null && packDir.absolutePath.contains(sd.appFilesDir!!.absolutePath),
        )

        // Vehicle + fuel + soft rest (eco, 6 h/day, 1.5 h / 15 min).
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
            "vehicle",
            JSONObject()
                .put("label", "VW Transporter T6 2.0 BiTDi 4Motion camper")
                .put("height_m", limits.heightM)
                .put("width_m", limits.widthM)
                .put("length_m", limits.lengthM)
                .put("total_weight_kg", limits.totalWeightKg)
                .put("axle_weight_kg", limits.axleWeightKg)
                .put("tank_l", TANK_L),
        )
        assertTrue(
            saveFuelConfig(
                dataDir.absolutePath,
                FfiFuelConfig(tankCapacityL = TANK_L, fuelAddedL = TANK_L, preferLiters = true),
            ),
        )
        report.put(
            "fuel_planning",
            "unimplemented in planCarRouteAt; FuelConfig HUD only; " +
                "report estimate uses 500–600 mile range with 100 km margin",
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
        val rest = loadCarRestSettings(dataDir.absolutePath)
        report.put(
            "car_rest",
            JSONObject()
                .put("break_interval_hours", rest.breakIntervalHours)
                .put("rest_duration_minutes", rest.restDurationMinutes)
                .put("eco_mode_enabled", rest.ecoModeEnabled)
                .put("max_hours", rest.maxHours),
        )

        // Plugins / prefs: DATEX, attractions, wild camping, long trip.
        MapHudPrefs.saveLongTripEnabled(context, true)
        MapHudPrefs.saveDatexPluginEnabled(context, true)
        MapHudPrefs.saveDatexWifiOnly(context, false)
        MapHudPrefs.savePoiLookaheadEnabled(context, true)
        MapHudPrefs.saveCampingPluginEnabled(context, true)
        campingPluginConfigure(context.filesDir.absolutePath, dataDir.absolutePath, TimeZone.getDefault().id)
        installCampingGuest()
        campingPluginSetEnabled(true)
        campingPluginSetTimezone("Europe/Oslo")
        report.put(
            "settings",
            JSONObject()
                .put("eco", true)
                .put("avoid_toll_roads", true)
                .put("toll_policy", "PENALIZE")
                .put("ferries", "use")
                .put("avoid_ferries", false)
                .put("soft_daily_budget_h", MAX_DAILY_HOURS)
                .put("soft_break_interval_h", 1.5)
                .put("soft_rest_min", 15)
                .put("wild_camping", true)
                .put("long_trip", true)
                .put("datex", true)
                .put("nearby_attractions", true),
        )

        // graph_format_version from live current.json (rebake may be in progress).
        report.put("current_json_regions", fetchCurrentJsonRegions())
        writeReport(report)

        // Synthetic DATEX road closures (DE/DK/SE/NO) — only synthetic inputs allowed.
        val datexCache = File(dataDir, "datex_cache").also { it.mkdirs() }
        val datexSeed = seedSyntheticDatex(datexCache)
        report.put("datex_synthetic_reroutes", datexSeed)
        datexRefreshJson(
            enabled = true,
            host = uniffi.navi.datexSettingsDefaultHost(),
            port = uniffi.navi.datexSettingsDefaultPort(),
            routeLatLonJson =
                JSONArray()
                    .put(JSONArray().put(ORIGIN_LAT).put(ORIGIN_LON))
                    .put(JSONArray().put(VIA_LAT).put(VIA_LON))
                    .put(JSONArray().put(DEST_LAT).put(DEST_LON))
                    .toString(),
            wifiOnly = false,
            onWifi = true,
            useDiscoveryChain = false,
            cacheDir = datexCache.absolutePath,
        )
        // Re-stamp + refresh fetched_unix after refresh (refresh may overwrite XML).
        seedSyntheticDatex(datexCache)
        File(datexCache, "apply_to_routing").writeText("1")
        report.put("datex_cache_dir", datexCache.absolutePath)
        writeReport(report)

        val waypoints =
            listOf(
                ORIGIN_LAT to ORIGIN_LON,
                VIA_LAT to VIA_LON,
                DEST_LAT to DEST_LON,
            )
        LongTripCoordinator.resetForTests()
        val scrubbed = LongTripPackStorage.scrubIncompletePacks(packDir)
        report.put("scrubbed_stems", JSONArray(scrubbed))
        val enableStatus = LongTripCoordinator.enable(context, waypoints)
        Log.i(TAG, "enable=$enableStatus")
        report.put("enable_status", enableStatus)
        val plan0 = LongTripCoordinator.currentPlan()
        assertTrue("corridor must exist: $enableStatus", plan0 != null)
        report.put("corridor", JSONArray(plan0!!.regionsInOrder))
        writeReport(report)

        val regionTimings = JSONObject()
        val seenState = mutableMapOf<String, String>()
        val stateEnteredUnix = mutableMapOf<String, Long>()
        val indexedUnix = mutableMapOf<String, Long>()
        val t0 = System.currentTimeMillis()
        var ready = false
        while (System.currentTimeMillis() - t0 < DOWNLOAD_DEADLINE_MS) {
            val p = LongTripCoordinator.currentPlan()
            val states =
                p?.regionsInOrder?.associateWith { p.states[it]?.name ?: "?" } ?: emptyMap()
            val nowUnix = System.currentTimeMillis() / 1000
            for ((reg, st) in states) {
                if (seenState[reg] != st) {
                    Log.i(TAG, "region $reg -> $st")
                    if (!stateEnteredUnix.containsKey("$reg:$st")) {
                        stateEnteredUnix["$reg:$st"] = nowUnix
                    }
                    if (st == "Indexed" || st == "Installed") {
                        indexedUnix.putIfAbsent(reg, nowUnix)
                    }
                    seenState[reg] = st
                }
            }
            val line = LongTripCoordinator.statusLine()
            val corrReady = LongTripCoordinator.corridorReadyForPlanning()
            report.put(
                "progress",
                JSONObject().also { o -> for ((k, v) in states) o.put(k, v) },
            )
            report.put("status_line", line)
            report.put("corridor_ready", corrReady)
            report.put("ram_download_mib", processPssMib())
            report.put(
                "region_order_states",
                JSONArray().also { arr ->
                    for (r in p?.regionsInOrder.orEmpty()) {
                        arr.put(
                            JSONObject()
                                .put("region", r)
                                .put("state", states[r] ?: "?")
                                .put("indexed_unix", indexedUnix[r] ?: JSONObject.NULL),
                        )
                    }
                },
            )
            writeReport(report)
            if (corrReady) {
                ready = true
                break
            }
            if (states.values.any { it == "Failed" } &&
                states.values.all { it == "Indexed" || it == "Installed" || it == "Failed" }
            ) {
                break
            }
            Thread.sleep(POLL_MS)
        }
        report.put("download_elapsed_ms", System.currentTimeMillis() - t0)
        report.put("corridor_ready_final", ready)
        for (r in plan0.regionsInOrder) {
            val start =
                stateEnteredUnix["$r:Downloading"] ?: stateEnteredUnix["$r:Queued"]
                    ?: (t0 / 1000)
            val done = indexedUnix[r]
            regionTimings.put(
                r,
                JSONObject()
                    .put("indexed_unix", done ?: JSONObject.NULL)
                    .put(
                        "process_index_secs",
                        if (done != null) done - start else JSONObject.NULL,
                    ).put("final_state", seenState[r] ?: "?"),
            )
        }
        report.put("region_timings", regionTimings)
        report.put(
            "pack_files",
            JSONArray(
                packDir.listFiles()?.map { "${it.name}:${it.length()}" }.orEmpty(),
            ),
        )
        writeReport(report)
        assertTrue(
            "corridor must be ready for planning within deadline; states=$seenState line=${LongTripCoordinator.statusLine()}",
            ready,
        )

        // Fresh DATEX stamp immediately before plan (15 min max-age).
        seedSyntheticDatex(datexCache)
        File(datexCache, "apply_to_routing").writeText("1")

        val placeDb = File(dataDir, "place_index.db")
        val placeAside = File(dataDir, "place_index.db.aside-bev-dalsoren")
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
                ?: File(dataDir, "$startStem-latest.osm.pbf")
        report.put("plan_pbf", pbf.absolutePath)
        val cacheDir = File(dataDir, "graph-cache-lt-mh-bev-dalsoren").also { it.mkdirs() }
        report.put("plan_cache_dir", cacheDir.absolutePath)
        report.put("plan_started_unix", System.currentTimeMillis() / 1000)
        report.put("ram_pre_plan_mib", processPssMib())
        writeReport(report)

        try {
            // Densify reorders vias by progress_t on OD (NW), not list order.
            // Helsingborg as endpoint disconnected northbound (ferry/dead-end snap).
            // Use Landskrona then Ängelholm so E6 transit stays inside Skåne, then
            // Gothenburg. MAX_ROUTE_VIA_POINTS=4.
            val viasUsed =
                listOf(
                    FfiLatLon(lat = 55.8704, lon = 12.8302), // Landskrona E6
                    FfiLatLon(lat = 56.2430, lon = 12.8630), // Ängelholm E6
                    FfiLatLon(lat = 57.7089, lon = 11.9746), // Gothenburg
                    FfiLatLon(lat = VIA_LAT, lon = VIA_LON), // Sognefjell
                )
            report.put(
                "plan_via_note",
                "Landskrona+Ängelholm+Gothenburg+Sognefjell. Avoid Helsingborg endpoint snap; " +
                    "E6 transit via Landskrona→Ängelholm.",
            )
            var planMsTotal = 0L
            val t0Plan = System.currentTimeMillis()
            val result =
                runPlan(
                    pbf = pbf,
                    dataDir = dataDir,
                    packDir = packDir,
                    cacheDir = cacheDir,
                    vias = viasUsed,
                )
            planMsTotal += System.currentTimeMillis() - t0Plan
            report.put(
                "vias_used",
                JSONArray().also { arr ->
                    for (v in viasUsed) {
                        arr.put(JSONObject().put("lat", v.lat).put("lon", v.lon))
                    }
                },
            )
            report.put("plan_elapsed_ms", planMsTotal)
            if (planMsTotal > PLAN_DEADLINE_MS) {
                report.put("plan_deadline_note", "exceeded ${PLAN_DEADLINE_MS}ms soft deadline")
            }
            report.put("ram_post_plan_mib", processPssMib())
            report.put(
                "full_plan",
                JSONObject()
                    .put("distance_km", result.distanceKm)
                    .put("eta_minutes", result.etaMinutes)
                    .put("search_terminate_reason", result.searchTerminateReason)
                    .put("days_json", result.daysJson)
                    .put("break_pois_json", result.breakPoisJson)
                    .put("maneuvers_json", result.maneuversJson.take(200_000))
                    .put("report", result.report.take(80_000)),
            )
            report.put("break_poi_count", jsonArrayLen(result.breakPoisJson))
            report.put("maneuver_count", jsonArrayLen(result.maneuversJson))
            report.put("days_parsed", parseDays(result.daysJson))
            report.put("rest_places", parseRestPlaces(result.breakPoisJson, result.daysJson))
            report.put(
                "ferries",
                JSONObject()
                    .put("route_uses_ferry", result.report.contains("route_uses_ferry=true"))
                    .put(
                        "ferry_legs",
                        countFerryUses(result.report),
                    ).put(
                        "report_ferry_lines",
                        JSONArray(
                            result.report
                                .lineSequence()
                                .filter { it.contains("ferry", ignoreCase = true) }
                                .take(40)
                                .toList(),
                        ),
                    ),
            )
            report.put("datex_in_plan_report", extractDatexFromPlanReport(result.report))
            report.put("fuel_stops_estimate", estimateFuelStops(result.distanceKm))
            report.put(
                "expected_check",
                JSONObject()
                    .put("distance_km", result.distanceKm)
                    .put("distance_ok", result.distanceKm in 1800.0..2300.0)
                    .put("duration_h", result.etaMinutes / 60.0)
                    .put("duration_ok", (result.etaMinutes / 60.0) in 22.0..35.0)
                    .put("maneuvers", jsonArrayLen(result.maneuversJson))
                    .put(
                        "maneuvers_ok",
                        jsonArrayLen(result.maneuversJson) in 200..500,
                    ),
            )
            writeReport(report)

            assertTrue(
                "must not snap_failed: ${result.searchTerminateReason}",
                result.searchTerminateReason != "snap_failed",
            )
            assertTrue(
                "distance must be positive (got ${result.distanceKm}; reason=${result.searchTerminateReason})",
                result.distanceKm > 100.0,
            )

            // Nearby attractions (POI look-ahead) along a few route samples.
            report.put("attractions", sampleAttractions(dataDir, pbf, result.routePolyline))
            writeReport(report)

            // Wild camping suggestions along the planned corridor.
            report.put("wild_camping", sampleWildCamping(result.routePolyline))
            writeReport(report)
        } catch (t: Throwable) {
            report.put("plan_error", t.toString())
            report.put("ram_on_error_mib", processPssMib())
            writeReport(report)
            throw t
        } finally {
            placeAside.takeIf { it.isFile }?.renameTo(placeDb)
        }
        report.put("ram_final_mib", processPssMib())
        report.put("finished_unix", System.currentTimeMillis() / 1000)
        writeReport(report)
        Log.i(TAG, "campaign done")
    }

    private fun installCampingGuest() {
        val am = context.assets
        val name = "right_to_roam_camping"
        val manifest = am.open("plugins/$name/plugin.json").use { it.readBytes().decodeToString() }
        val wasm = am.open("plugins/$name/plugin.wasm").use { it.readBytes() }
        campingPluginInstallGuest(name, manifest, wasm)
    }

    private fun runPlan(
        pbf: File,
        dataDir: File,
        packDir: File,
        cacheDir: File,
        vias: List<FfiLatLon>,
    ): uniffi.navi.CorridorRouteResult {
        Log.i(TAG, "planCarRouteAt MobileHome ferries=use toll=PENALIZE eco vias=${vias.size}")
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
                avoidFerries = false,
                avoidTunnels = false,
                vehicle = loadVehicleLimits(dataDir.absolutePath),
                preferOfficialNetworks = false,
                departureLocalIso = DEPARTURE_ISO,
                dataDir = dataDir.absolutePath,
                packDir = packDir.absolutePath,
                longTripEnabled = true,
                allowedCountries = null,
                viaPoints = vias,
            )
        val planMs = System.currentTimeMillis() - tPlan
        // Accumulate into latest report file via caller; also log.
        Log.i(
            TAG,
            "plan done ms=$planMs km=${result.distanceKm} reason=${result.searchTerminateReason}",
        )
        return result
    }

    private fun countFerryUses(report: String): Int {
        var n = 0
        for (line in report.lineSequence()) {
            if (line.contains("route_uses_ferry=true")) n++
        }
        return n
    }

    private fun seedSyntheticDatex(cacheDir: File): JSONArray {
        // Block closures near corridor chords across DE / DK / SE / NO.
        data class Sit(
            val id: String,
            val country: String,
            val lat: Double,
            val lon: Double,
            val label: String,
        )
        val sits =
            listOf(
                Sit("syn-de-a7", "DE", 53.2500, 10.0500, "Synthetic DE A7/E45 closure (Lower Saxony)"),
                Sit("syn-dk-e45", "DK", 55.4000, 9.4800, "Synthetic DK E45 closure (Jutland)"),
                // Inland of E6 so Block still has a coastal reroute (not corridor kill).
                Sit("syn-se-e6", "SE", 56.2500, 13.1500, "Synthetic SE 13/inland closure (Skåne)"),
                Sit("syn-se-gbg", "SE", 57.7200, 12.1500, "Synthetic SE inland closure (Gothenburg east)"),
                Sit("syn-no-e6", "NO", 60.8000, 10.9000, "Synthetic NO parallel closure (Innlandet east)"),
                Sit("syn-no-fv55", "NO", 61.5500, 7.9500, "Synthetic NO Fv55 spur closure (toward Dalsøren)"),
            )
        val now = System.currentTimeMillis() / 1000
        val sb = StringBuilder()
        sb.append(
            """<?xml version='1.0' encoding='UTF-8' standalone='yes'?>
<ns2:messageContainer xmlns="http://datex2.eu/schema/3/common" xmlns:ns2="http://datex2.eu/schema/3/messageContainer" xmlns:ns9="http://datex2.eu/schema/3/locationReferencing" xmlns:ns12="http://datex2.eu/schema/3/situation" modelBaseVersion="3">
<ns2:payload xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="ns12:SituationPublication" lang="no" modelBaseVersion="3">
<feedType>FULL</feedType>
<publicationTime>2026-09-30T08:00:00.000+02:00</publicationTime>
<publicationCreator><country>no</country><nationalIdentifier>NAVI-SYNTH</nationalIdentifier></publicationCreator>
""",
        )
        val out = JSONArray()
        for (s in sits) {
            sb.append(
                """
<ns12:situation id="${s.id}"><ns12:overallSeverity>high</ns12:overallSeverity><ns12:headerInformation><confidentiality>noRestriction</confidentiality><informationStatus>test</informationStatus></ns12:headerInformation><ns12:situationRecord xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="ns12:MaintenanceWorks" id="${s.id}_1" version="1"><ns12:situationRecordCreationTime>2026-09-30T07:00:00+02:00</ns12:situationRecordCreationTime><ns12:situationRecordVersionTime>2026-09-30T07:00:00+02:00</ns12:situationRecordVersionTime><ns12:probabilityOfOccurrence>certain</ns12:probabilityOfOccurrence><ns12:severity>high</ns12:severity><ns12:source><sourceCountry>${s.country}</sourceCountry><sourceIdentification>navi-synth</sourceIdentification><sourceName><values><value lang="en">Navi synthetic DATEX</value></values></sourceName><sourceType>roadAuthorities</sourceType></ns12:source><ns12:validity><validityStatus>definedByValidityTimeSpec</validityStatus><validityTimeSpecification><overallStartTime>2026-05-01T00:00:00+02:00</overallStartTime><overallEndTime>2026-12-31T23:59:00+01:00</overallEndTime></validityTimeSpecification></ns12:validity><ns12:impact><ns12:numberOfLanesRestricted>2</ns12:numberOfLanesRestricted></ns12:impact><ns12:generalPublicComment><ns12:comment><values><value lang="no">Vegen er stengt.|Omkjøring er skiltet. ${s.label}</value></values></ns12:comment><ns12:commentType>dataProcessingNote</ns12:commentType></ns12:generalPublicComment><ns12:locationReference xsi:type="ns9:LocationGroupByList"><ns9:locationContainedInGroup xsi:type="ns9:LinearLocation"><ns9:coordinatesForDisplay><ns9:latitude>${s.lat}</ns9:latitude><ns9:longitude>${s.lon}</ns9:longitude></ns9:coordinatesForDisplay><ns9:supplementaryPositionalDescription><ns9:locationDescription><values><value lang="en">${s.label}</value></values></ns9:locationDescription><ns9:carriageway><ns9:carriageway>mainCarriageway</ns9:carriageway><ns9:originalNumberOfLanes>2</ns9:originalNumberOfLanes></ns9:carriageway><ns9:namedArea xsi:type="ns9:IsoNamedArea"><ns9:areaName><values><value lang="en">${s.country}</value></values></ns9:areaName><ns9:country>${s.country}</ns9:country></ns9:namedArea></ns9:supplementaryPositionalDescription></ns9:locationContainedInGroup></ns12:locationReference><ns12:roadworksIdentifier>${s.id}</ns12:roadworksIdentifier><ns12:roadMaintenanceType>roadworks</ns12:roadMaintenanceType></ns12:situationRecord></ns12:situation>
""",
            )
            out.put(
                JSONObject()
                    .put("id", s.id)
                    .put("country", s.country)
                    .put("lat", s.lat)
                    .put("lon", s.lon)
                    .put("label", s.label)
                    .put("impact", "Block")
                    .put("synthetic", true),
            )
        }
        sb.append("</ns2:payload></ns2:messageContainer>\n")
        File(cacheDir, "datex-GetSituation.xml").writeText(sb.toString())
        File(cacheDir, "datex-cache.json").writeText(
            """
            {
              "fetched_unix": $now,
              "source_fingerprint": "navi-synth-bevensen-dalsoren",
              "data_source": "server-duckdns",
              "base_url": "https://navigate-me.duckdns.org",
              "attribution": "synthetic campaign DATEX",
              "source": null
            }
            """.trimIndent(),
        )
        File(cacheDir, "apply_to_routing").writeText("1")
        return out
    }

    private fun fetchCurrentJsonRegions(): JSONObject {
        val out = JSONObject()
        return try {
            val conn = URL("https://navigate-me.duckdns.org/current.json").openConnection() as HttpURLConnection
            conn.connectTimeout = 15_000
            conn.readTimeout = 60_000
            conn.requestMethod = "GET"
            val body = conn.inputStream.bufferedReader().use { it.readText() }
            conn.disconnect()
            val root = JSONObject(body)
            out.put("generation", root.optString("generation"))
            out.put("created_unix", root.optLong("created_unix"))
            val arr = root.optJSONArray("regions") ?: JSONArray()
            val want =
                JSONArray().also { kept ->
                    for (i in 0 until arr.length()) {
                        val r = arr.optJSONObject(i) ?: continue
                        val id = r.optString("region_id")
                        if (id.contains("germany") ||
                            id.contains("denmark") ||
                            id.contains("sweden") ||
                            id.contains("norway") ||
                            id == "europe/denmark"
                        ) {
                            kept.put(
                                JSONObject()
                                    .put("region_id", id)
                                    .put("graph_format_version", r.optInt("graph_format_version", -1))
                                    .put("bytes", r.optLong("bytes"))
                                    .put("generation", r.optString("generation")),
                            )
                        }
                    }
                }
            out.put("regions", want)
            out
        } catch (t: Throwable) {
            out.put("error", t.toString())
            out
        }
    }

    private fun estimateFuelStops(distanceKm: Double): JSONObject {
        // 500–600 miles full-tank range; keep 100 km margin; start full.
        fun stops(rangeMiles: Double): Int {
            val rangeKm = rangeMiles * 1.609344
            val usable = (rangeKm - 100.0).coerceAtLeast(1.0)
            return maxOf(0, kotlin.math.ceil(distanceKm / usable).toInt() - 1)
        }
        return JSONObject()
            .put("distance_km", distanceKm)
            .put("range_miles_low", 500)
            .put("range_miles_high", 600)
            .put("margin_km", 100)
            .put("stops_at_500mi", stops(500.0))
            .put("stops_at_600mi", stops(600.0))
            .put(
                "note",
                "Report-only estimate; fuel-stop planning unimplemented in engine",
            )
    }

    private fun sampleAttractions(
        dataDir: File,
        pbf: File,
        polyline: String,
    ): JSONObject {
        val o = JSONObject()
        return try {
            val stats = ensurePoiLookaheadLoaded(dataDir.absolutePath, pbf.absolutePath)
            o.put("load_ok", true)
            o.put(
                "load_stats",
                JSONObject()
                    .put("records", stats.records.toLong())
                    .put("cone_m", stats.coneM)
                    .put("half_width_deg", stats.halfWidthDeg),
            )
            val samples = samplePolyline(polyline, 8)
            val byType = JSONObject()
            var total = 0
            val hits = JSONArray()
            for ((lat, lon) in samples) {
                val raw = poiLookaheadQueryJson(lat, lon, 0.0, true, false)
                val arr =
                    try {
                        JSONArray(raw)
                    } catch (_: Throwable) {
                        JSONObject(raw).optJSONArray("hits") ?: JSONArray()
                    }
                for (i in 0 until arr.length()) {
                    val h = arr.optJSONObject(i) ?: continue
                    total++
                    val kind = h.optString("kind", h.optString("category", "unknown"))
                    byType.put(kind, byType.optInt(kind) + 1)
                    if (hits.length() < 40) {
                        hits.put(
                            JSONObject()
                                .put("name", h.optString("name"))
                                .put("kind", kind)
                                .put("lat", h.optDouble("lat"))
                                .put("lon", h.optDouble("lon")),
                        )
                    }
                }
            }
            o.put("sample_points", samples.size)
            o.put("hit_count", total)
            o.put("by_type", byType)
            o.put("hits_snip", hits)
            o
        } catch (t: Throwable) {
            o.put("error", t.toString())
            o
        }
    }

    private fun sampleWildCamping(polyline: String): JSONObject {
        val o = JSONObject()
        return try {
            val samples = samplePolyline(polyline, 12)
            if (samples.size < 2) {
                o.put("error", "polyline too short")
                return o
            }
            val wpJson =
                campingWaypointsJson(
                    samples.map { (lat, lon) -> doubleArrayOf(lat, lon) },
                )
            val dest = samples.last()
            campingPluginSetNavContext(
                waypointsJson = wpJson,
                destLat = dest.first,
                destLon = dest.second,
                profile = TravelProfile.MOBILE_HOME,
                professionalDriver = false,
            )
            val call = campingPluginSuggestAlongRoute(16u)
            o.put("kind", call.kind.name)
            o.put("message", call.message)
            o.put("result_json", call.resultJson?.take(50_000))
            if (call.kind == CampingCallKind.OK && !call.resultJson.isNullOrBlank()) {
                val parsed = JSONObject(call.resultJson!!)
                val cards = JSONArray()
                val list = parsed.optJSONObject("list")?.optJSONArray("cards") ?: JSONArray()
                val foot = parsed.optJSONObject("onFootFromHere")?.optJSONArray("cards") ?: JSONArray()
                for (src in listOf(list, foot)) {
                    for (i in 0 until src.length()) {
                        val c = src.optJSONObject(i) ?: continue
                        cards.put(
                            JSONObject()
                                .put("title", c.optString("title", c.optString("name")))
                                .put("lat", c.optDouble("lat"))
                                .put("lon", c.optDouble("lon"))
                                .put("country", c.optString("countryIso"))
                                .put("legal", c.optString("legal")),
                        )
                    }
                }
                o.put("wild_camping_site_count", cards.length())
                o.put("sites", cards)
            } else {
                o.put("wild_camping_site_count", 0)
            }
            o
        } catch (t: Throwable) {
            o.put("error", t.toString())
            o
        }
    }

    private fun samplePolyline(
        polyline: String,
        n: Int,
    ): List<Pair<Double, Double>> {
        // Encoded polyline5 or raw JSON [[lat,lon],...] — try JSON first.
        try {
            val arr = JSONArray(polyline)
            if (arr.length() >= 2) {
                val step = maxOf(1, arr.length() / n)
                val out = ArrayList<Pair<Double, Double>>()
                var i = 0
                while (i < arr.length() && out.size < n) {
                    val pt = arr.optJSONArray(i)
                    if (pt != null && pt.length() >= 2) {
                        out.add(pt.getDouble(0) to pt.getDouble(1))
                    }
                    i += step
                }
                return out
            }
        } catch (_: Throwable) {
        }
        val decoded = decodePolyline(polyline)
        if (decoded.size >= 2) {
            val step = maxOf(1, decoded.size / n)
            return decoded.filterIndexed { i, _ -> i % step == 0 }.take(n)
        }
        // Fallback: origin / via / dest only.
        return listOf(
            ORIGIN_LAT to ORIGIN_LON,
            VIA_LAT to VIA_LON,
            DEST_LAT to DEST_LON,
        )
    }

    /** Navi corridor string `"lon,lat;lon,lat;…"`. */
    private fun decodePolyline(poly: String): List<Pair<Double, Double>> {
        return poly.split(';').mapNotNull { part ->
            val bits = part.split(',')
            if (bits.size < 2) return@mapNotNull null
            val lo = bits[0].toDoubleOrNull() ?: return@mapNotNull null
            val la = bits[1].toDoubleOrNull() ?: return@mapNotNull null
            la to lo
        }
    }

    private fun parseDays(raw: String): JSONArray {
        val out = JSONArray()
        try {
            val arr = JSONArray(raw)
            for (i in 0 until arr.length()) {
                val d = arr.optJSONObject(i) ?: continue
                out.put(
                    JSONObject()
                        .put("day_index", d.optInt("day_index", i))
                        .put("distance_km", d.optDouble("distance_km"))
                        .put("driving_hours", d.optDouble("driving_hours"))
                        .put("overnight_name", d.optString("overnight_name"))
                        .put("rest_kind", d.optString("rest_kind"))
                        .put("is_final", d.optBoolean("is_final")),
                )
            }
        } catch (_: Throwable) {
            out.put(JSONObject().put("parse_error", true))
        }
        return out
    }

    private fun parseRestPlaces(
        breakPois: String,
        days: String,
    ): JSONArray {
        val out = JSONArray()
        try {
            val arr = JSONArray(breakPois)
            for (i in 0 until arr.length()) {
                val b = arr.optJSONObject(i) ?: continue
                out.put(
                    JSONObject()
                        .put("name", b.optString("name"))
                        .put("lat", b.optDouble("lat"))
                        .put("lon", b.optDouble("lon"))
                        .put("kind", b.optString("kind"))
                        .put("along_km", b.optDouble("along_km")),
                )
            }
        } catch (_: Throwable) {
        }
        try {
            val arr = JSONArray(days)
            for (i in 0 until arr.length()) {
                val d = arr.optJSONObject(i) ?: continue
                val name = d.optString("overnight_name")
                if (name.isNotBlank()) {
                    out.put(
                        JSONObject()
                            .put("name", name)
                            .put("lat", d.optDouble("overnight_lat", Double.NaN))
                            .put("lon", d.optDouble("overnight_lon", Double.NaN))
                            .put("kind", "overnight")
                            .put("rest_kind", d.optString("rest_kind")),
                    )
                }
            }
        } catch (_: Throwable) {
        }
        return out
    }

    private fun extractDatexFromPlanReport(rep: String): JSONObject {
        val o = JSONObject()
        o.put(
            "lines",
            JSONArray(
                rep
                    .lineSequence()
                    .filter { it.contains("datex", ignoreCase = true) }
                    .take(40)
                    .toList(),
            ),
        )
        return o
    }

    private fun jsonArrayLen(raw: String): Int =
        try {
            JSONArray(raw).length()
        } catch (_: Throwable) {
            0
        }

    private fun processPssMib(): Double {
        val mi = Debug.MemoryInfo()
        Debug.getMemoryInfo(mi)
        return mi.totalPss / 1024.0
    }

    private fun writeReport(obj: JSONObject) {
        val text = obj.toString(2)
        val dataDir = NaviAppData.resolve(context)
        runCatching {
            File(dataDir, "long-trip-bevensen-dalsoren").also { it.mkdirs() }.let {
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
                val dir = File(sd, "long-trip-bevensen-dalsoren").also { it.mkdirs() }
                File(dir, "report.json").writeText(text)
            }
        }
        runCatching {
            File("/data/local/tmp/long-trip-bevensen-dalsoren.json").writeText(text)
        }
        runCatching {
            context.getExternalFilesDir(null)?.let {
                File(it, "long-trip-bevensen-dalsoren.json").writeText(text)
            }
        }
    }
}
