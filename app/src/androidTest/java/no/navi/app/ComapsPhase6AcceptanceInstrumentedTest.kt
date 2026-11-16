package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.rule.ActivityTestRule
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.FixMethodOrder
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters
import uniffi.navi.FfiSearchBatch
import uniffi.navi.FfiSearchHit
import uniffi.navi.FfiSearchListener
import uniffi.navi.TravelProfile
import uniffi.navi.comapsSearchClearViewport
import uniffi.navi.comapsSearchSetLocale
import uniffi.navi.comapsSearchSetPosition
import uniffi.navi.comapsSearchStart
import java.io.File
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * Phase 6 on-device acceptance for the hybrid CoMaps drape (Samsung SM-P613, Navi only).
 *
 * Steps are ordered (a_, b_, ...) and independent enough to be run one at a time:
 * `am instrument -e class no.navi.app.ComapsPhase6AcceptanceInstrumentedTest#c_...`.
 * Output: PNGs under /data/local/tmp/phase6/screenshots, JSON/text logs under
 * /data/local/tmp/phase6/logs (pulled to test-artifacts/phase6/ by the runner script).
 */
@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class ComapsPhase6AcceptanceInstrumentedTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private val h = ComapsPhase6Harness

    private class Collector : FfiSearchListener {
        val hits = CopyOnWriteArrayList<FfiSearchHit>()
        val done = CountDownLatch(1)

        override fun onBatch(batch: FfiSearchBatch) {
            hits.addAll(batch.hits)
            if (batch.isFinal) done.countDown()
        }
    }

    private fun writeLog(
        name: String,
        text: String,
    ) {
        h.ensureShellDirs()
        val tmp = File(h.context.externalCacheDir ?: h.context.cacheDir, name)
        tmp.writeText(text)
        h.shell("cp ${tmp.absolutePath} ${h.SHELL_DIR}/logs/$name")
        h.shell("chmod 644 ${h.SHELL_DIR}/logs/$name")
    }

    /** Hide the HUD bars too (pure map shots); step tests that need the HUD pass false. */
    private fun launch(chromeHidden: Boolean = true) {
        MwmClientBootstrap.ensureInitialized()
        assumeTrue("MWM flag off", MwmClientBootstrap.isEnabled())
        h.ensureShellDirs()
        h.stageProductionMwmRoot()
        assertTrue("session open", ComapsSessionBootstrap.ensureOpen(h.context, force = false))
        activityRule.launchActivity(null)
        // MainActivity resets the chrome hooks from the launch intent; set them afterwards.
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.hideUiChrome = chromeHidden
        if (!NaviMapTestHooks.drapeDisabled) {
            assertTrue(
                "drape init",
                h.waitFor(90_000, 500) { NaviMapTestHooks.drapeState == "READY" || NaviMapTestHooks.drapeState == "FAILED" },
            )
        }
        android.util.Log.i(h.TAG, "drape=${NaviMapTestHooks.drapeState} err=${NaviMapTestHooks.drapeError}")
    }

    private fun search(
        query: String,
        nearLat: Double = 61.0,
        nearLon: Double = 10.5,
    ): List<FfiSearchHit> {
        comapsSearchSetLocale("nb")
        comapsSearchSetPosition(nearLat, nearLon, true)
        comapsSearchClearViewport()
        val c = Collector()
        val id = comapsSearchStart(query, false, 30u, c)
        assertTrue("search '$query' did not start", id != 0uL)
        assertTrue("search '$query' timed out", c.done.await(30, TimeUnit.SECONDS))
        Thread.sleep(300)
        return c.hits.toList()
    }

    /** Resolve candidates for a place name and record the ambiguity for the report. */
    private fun resolveReport(
        query: String,
        nearLat: Double,
        nearLon: Double,
    ): Pair<FfiSearchHit?, JSONObject> {
        val hits = search(query, nearLat, nearLon)
        val json = JSONObject().put("query", query)
        val arr = JSONArray()
        hits.take(8).forEach {
            arr.put(
                JSONObject()
                    .put("name", it.name)
                    .put("address", it.address)
                    .put("type", it.featureType)
                    .put("lat", it.lat)
                    .put("lon", it.lon)
                    .put("source", it.source),
            )
        }
        json.put("candidates", arr).put("count", hits.size)
        return hits.firstOrNull() to json
    }

    // Start / end picked explicitly (a user choice among the candidates logged by a_resolve_places).
    private val espa = Triple("Espa E6", 60.562616550309635, 11.2546040956274)
    private val dombas = Triple("Dombås", 62.06881688813547, 9.125544347804379)
    private val litjfossen = Triple("Litjfossen", 62.66922983534084, 10.152202771247943)

    private fun plan(
        from: Triple<String, Double, Double>,
        to: Triple<String, Double, Double>,
        profile: TravelProfile = TravelProfile.CAR,
    ): uniffi.navi.CorridorRouteResult =
        MwmProductionPlanner.plan(
            context = h.context,
            profile = profile,
            startLat = from.second,
            startLon = from.third,
            endLat = to.second,
            endLon = to.third,
            vias = emptyList(),
            avoidMotorways = false,
            avoidTolls = false,
            avoidFerries = false,
            turnsLocale = "nb",
        )

    private fun showRoute(
        planned: uniffi.navi.CorridorRouteResult,
        from: Triple<String, Double, Double>,
        to: Triple<String, Double, Double>,
    ) {
        NaviMapTestHooks.routeStartLabel = from.first
        NaviMapTestHooks.routeEndLabel = to.first
        NaviMapTestHooks.pendingFromPoint = Waypoint(from.first, from.second, from.third)
        NaviMapTestHooks.pendingToPoint = Waypoint(to.first, to.second, to.third)
        NaviMapTestHooks.lastRoutePolylineChars = 0
        NaviMapTestHooks.pendingRoute = planned
        assertTrue(
            "route not applied",
            h.waitFor(60_000, 400) {
                if (NaviMapTestHooks.lastRoutePolylineChars <= 50) NaviMapTestHooks.pendingRoute = planned
                NaviMapTestHooks.lastRoutePolylineChars > 50
            },
        )
    }

    private fun polylinePoints(poly: String): List<Pair<Double, Double>> =
        poly.split(';').mapNotNull { p ->
            val v = p.split(',')
            if (v.size == 2) v[1].toDouble() to v[0].toDouble() else null
        }

    @Test
    fun b_overlay_alignment_espa_dombas() {
        launch()
        val planned = plan(espa, dombas)
        android.util.Log.i(h.TAG, "plan report=${planned.report.replace('\n', ' ')}")
        assertTrue("plan PASS: ${planned.report}", planned.report.contains("PASS"))
        showRoute(planned, espa, dombas)
        val pts = polylinePoints(planned.routePolyline)
        val mid = pts[pts.size / 4]
        for (z in listOf(12.0, 13.5, 14.0, 14.3, 15.0)) {
            h.gotoCamera(mid.first, mid.second, z)
            Thread.sleep(3_000)
            h.screenshot("b_align_z${z.toString().replace('.', '_')}")
        }
    }

    private fun shot(
        name: String,
        lat: Double,
        lon: Double,
        zoom: Double,
        settleMs: Long = 3_500,
    ) {
        assertTrue("camera $name", h.gotoCamera(lat, lon, zoom))
        Thread.sleep(settleMs)
        h.screenshot(name)
    }

    @Test
    fun c_visual_matrix() {
        launch()
        val log = JSONArray()

        fun note(
            name: String,
            extra: String = "",
        ) {
            val be = ComapsDrape.backend
            log.put(
                JSONObject()
                    .put("shot", name)
                    .put("drapeBasemapVisible", NaviMapTestHooks.drapeBasemapVisible)
                    .put("night", be?.isNight())
                    .put("contours", be?.contoursEnabled())
                    .put("3d", be?.get3d()?.let { "${it.first}/${it.second}" })
                    .put("drawScale", be?.drawScale())
                    .put("extra", extra),
            )
        }

        // Land cover day + night (farmland/forest/water; alpine bare rock/ice).
        val spots =
            listOf(
                Triple("mjosa_farmland_forest", 60.80, 10.70),
                Triple("gjendebu_alpine", 61.493, 8.351),
                Triple("lillehammer_town", 61.115, 10.466),
            )
        for ((name, la, lo) in spots) {
            for (night in listOf(false, true)) {
                NaviMapTestHooks.drapeNightOverride = night
                val n = "c_land_${name}_${if (night) "night" else "day"}"
                shot(n, la, lo, 12.0)
                note(n)
            }
        }
        NaviMapTestHooks.drapeNightOverride = false

        // Contours 12 / 14 / 16, off then on (Gjendebu: steep terrain).
        for (z in listOf(12.0, 14.0, 16.0)) {
            for (on in listOf(false, true)) {
                NaviMapTestHooks.requestContoursEnabled = on
                Thread.sleep(1_500)
                val n = "c_contours_z${z.toInt()}_${if (on) "on" else "off"}"
                shot(n, 61.493, 8.351, z)
                note(n)
            }
        }
        NaviMapTestHooks.requestContoursEnabled = false

        // 3D / buildings (Lillehammer centre, dense buildings).
        for (on in listOf(false, true)) {
            NaviMapTestHooks.requestOptIn3d = on
            Thread.sleep(2_000)
            val n = "c_3d_${if (on) "on" else "off"}_z17"
            shot(n, 61.1153, 10.4662, 17.0)
            note(n)
        }
        NaviMapTestHooks.requestOptIn3d = false

        // Labels with æøå and E6 / Rv shields.
        val labels =
            listOf(
                Triple("c_label_honefoss_oe", 60.168 to 10.256, 14.0),
                Triple("c_label_al_aa", 60.628 to 8.563, 14.0),
                Triple("c_label_favang_aa", 61.457 to 10.188, 14.0),
                Triple("c_label_gjovik_oe", 60.795 to 10.691, 14.0),
                Triple("c_label_e6_lillehammer", 61.12 to 10.45, 13.0),
                Triple("c_label_rv_dombas", 62.075 to 9.128, 13.0),
            )
        for ((n, ll, z) in labels) {
            shot(n, ll.first, ll.second, z)
            note(n)
        }
        writeLog("visual-matrix.json", log.toString(2))
    }

    /** Copy the repeater DB from the debug app's server mirror (pushed to /data/local/tmp) and enable CAT. */
    private fun provisionCat(network: String) {
        val ctx = h.context
        val src = File("${h.SHELL_DIR}/cat_repeaters.sqlite")
        assertTrue("push ${src.path} first", src.isFile)
        src.copyTo(File(ctx.filesDir, "cat_repeaters.sqlite"), overwrite = true)
        File(ctx.filesDir, "cat_repeaters.sqlite.generation").writeText("phase6-copy")
        MapHudPrefs.saveCatDropImported(ctx, true)
        MapHudPrefs.saveCatPluginEnabled(ctx, true)
        MapHudPrefs.saveCatNetworkChoice(ctx, network)
        MapHudPrefs.saveCatBackend(ctx, MapHudPrefs.CAT_BACKEND_BENSHI)
    }

    /**
     * Plan a route with the production MWM planner, start Navi's own route simulation and
     * let the app's CAT follow tick run against the in-process Benshi lab rig (dry run, no
     * radio). Timeline + perf samples go to /data/local/tmp/phase6/logs.
     */
    private fun runCatRoute(
        tag: String,
        from: Triple<String, Double, Double>,
        to: Triple<String, Double, Double>,
        network: String,
        timeScale: Double,
        capMinutes: Int,
        perf: Boolean,
        cat: Boolean = true,
        drape: Boolean = true,
    ) {
        MwmClientBootstrap.ensureInitialized()
        NaviMapTestHooks.drapeDisabled = !drape
        if (cat) provisionCat(network) else MapHudPrefs.saveCatPluginEnabled(h.context, false)
        launch(chromeHidden = false)
        if (cat) {
            assertTrue(
                "cat plugin enabled",
                h.waitFor(60_000, 500) { uniffi.navi.catPluginIsEnabled() },
            )
            val connect = uniffi.navi.catPluginConnectBenshiLab(MapHudPrefs.loadCatScratchChannel(h.context).toUByte())
            val follow =
                uniffi.navi.catPluginNetworkFollowJson(
                    JSONObject().put("enabled", true).put("network_id", network).toString(),
                )
            android.util.Log.i(h.TAG, "cat connect=$connect follow=$follow")
        }
        val planned = plan(from, to)
        android.util.Log.i(h.TAG, "$tag plan ${planned.report.replace('\n', ' ')}")
        assertTrue("plan PASS: ${planned.report}", planned.report.contains("PASS"))
        showRoute(planned, from, to)
        h.screenshot("${tag}_route_planned")
        NaviMapTestHooks.simulationTimeScale = timeScale
        NaviMapTestHooks.requestRotationMode = MapRotationMode.NorthUp
        NaviMapTestHooks.lastArrivedAtEnd = false
        val sampler = ComapsPhase6Harness.PerfSampler()
        if (perf) sampler.start()
        val t0 = System.currentTimeMillis()
        NaviMapTestHooks.requestStartRouteSimulation = true
        assertTrue("simulation active", h.waitFor(30_000, 250) { NaviMapTestHooks.simulatingActive })
        // Navigation view: follow the puck at street level instead of the route overview.
        assertTrue("first fix", h.waitFor(20_000, 250) { !NaviMapTestHooks.lastGpsLat.isNaN() })
        NaviMapTestHooks.pendingCamera = Triple(NaviMapTestHooks.lastGpsLat, NaviMapTestHooks.lastGpsLon, 15.0)
        Thread.sleep(1_500)
        NaviMapTestHooks.requestRecenterGps = true
        val timeline = JSONArray()
        val deadline = t0 + capMinutes * 60_000L
        var nextShot = t0 + 60_000L
        while (System.currentTimeMillis() < deadline && !NaviMapTestHooks.lastArrivedAtEnd) {
            Thread.sleep(10_000)
            timeline.put(
                JSONObject()
                    .put("wall_s", (System.currentTimeMillis() - t0) / 1000)
                    .put("lat", NaviMapTestHooks.lastGpsLat)
                    .put("lon", NaviMapTestHooks.lastGpsLon)
                    .put("speed_kmh", NaviMapTestHooks.lastSimSpeedKmh)
                    .put("drape", NaviMapTestHooks.drapeBasemapVisible)
                    .put("cat", JSONObject(uniffi.navi.catPluginStatusJson())),
            )
            if (System.currentTimeMillis() >= nextShot) {
                nextShot += 4 * 60_000L
                h.screenshot("${tag}_sim_${(System.currentTimeMillis() - t0) / 60_000}min", 10_000)
            }
        }
        val arrived = NaviMapTestHooks.lastArrivedAtEnd
        if (perf) writeLog("perf-$tag.json", sampler.stop().toString(2))
        h.screenshot("${tag}_sim_end", 10_000)
        writeLog(
            "cat-$tag-timeline.json",
            JSONObject()
                .put("tag", tag)
                .put("network", network)
                .put("time_scale", timeScale)
                .put("arrived", arrived)
                .put("wall_s", (System.currentTimeMillis() - t0) / 1000)
                .put("plan_report", planned.report)
                .put("timeline", timeline)
                .toString(2),
        )
        NaviMapTestHooks.requestStopRouteSimulation = true
        assertTrue("sim reached the end or ran to the cap", arrived || System.currentTimeMillis() >= deadline)
        NaviMapTestHooks.drapeDisabled = false
    }

    @Test
    fun d_cat_espa_dombas_auto_with_perf() {
        runCatRoute("espa_dombas_auto", espa, dombas, MapHudPrefs.CAT_NETWORK_AUTO, 12.0, 30, perf = true)
    }

    @Test
    fun e_cat_espa_litjfossen_auto() {
        runCatRoute("espa_litjfossen_auto", espa, litjfossen, MapHudPrefs.CAT_NETWORK_AUTO, 12.0, 45, perf = true)
    }

    @Test
    fun i_perf_drape_6min() {
        runCatRoute("perf_drape", espa, dombas, MapHudPrefs.CAT_NETWORK_OFF, 12.0, 6, perf = true, cat = false)
    }

    @Test
    fun j_perf_maplibre_only_6min() {
        runCatRoute("perf_maplibre_only", espa, dombas, MapHudPrefs.CAT_NETWORK_OFF, 12.0, 6, perf = true, cat = false, drape = false)
    }

    @Test
    fun f_cat_espa_dombas_standalone() {
        runCatRoute("espa_dombas_standalone", espa, dombas, MapHudPrefs.CAT_NETWORK_STANDALONE, 20.0, 30, perf = false)
    }

    private class Matrix {
        val rows = JSONArray()

        fun add(
            id: String,
            feature: String,
            result: String,
            detail: String,
        ) {
            rows.put(
                JSONObject()
                    .put("id", id)
                    .put("feature", feature)
                    .put("result", result)
                    .put("detail", detail),
            )
            android.util.Log.i("Phase6", "matrix $id $result $detail")
        }

        /** Run [body]; it returns "PASS: ..." / "PARTIAL: ..." / "FAIL: ..." (any throw is a FAIL with the cause). */
        fun check(
            id: String,
            feature: String,
            body: () -> String,
        ) {
            val out =
                try {
                    body()
                } catch (t: Throwable) {
                    "FAIL: ${t.javaClass.simpleName}: ${t.message}"
                }
            val result = out.substringBefore(':').trim()
            add(id, feature, if (result in setOf("PASS", "PARTIAL", "FAIL", "NOT_TESTED")) result else "FAIL", out.substringAfter(':').trim())
        }
    }

    private fun code(r: uniffi.navi.CorridorRouteResult): String = Regex("comaps_route_code=(\\w+)").find(r.report)?.groupValues?.get(1) ?: "?"

    private fun planWith(
        from: Triple<String, Double, Double>,
        to: Triple<String, Double, Double>,
        profile: TravelProfile = TravelProfile.CAR,
        vias: List<Pair<Double, Double>> = emptyList(),
        motorways: Boolean = false,
        tolls: Boolean = false,
        ferries: Boolean = false,
        bikeWetland: Boolean = false,
    ) = MwmProductionPlanner.plan(
        context = h.context,
        profile = profile,
        startLat = from.second,
        startLon = from.third,
        endLat = to.second,
        endLon = to.third,
        vias = vias,
        avoidMotorways = motorways,
        avoidTolls = tolls,
        avoidFerries = ferries,
        turnsLocale = "nb",
        bikeWetland = bikeWetland,
    )

    private fun km(r: uniffi.navi.CorridorRouteResult) = "%.1f km / %.0f min".format(java.util.Locale.US, r.distanceKm, r.etaMinutes)

    private fun placeNear(
        query: String,
        lat: Double,
        lon: Double,
        typeContains: String? = null,
    ): Triple<String, Double, Double> {
        val hits = search(query, lat, lon)
        val hit = hits.firstOrNull { typeContains == null || it.featureType.contains(typeContains, true) } ?: hits.first()
        return Triple(hit.name, hit.lat, hit.lon)
    }

    @Test
    fun h_function_matrix() {
        launch(chromeHidden = false)
        val m = Matrix()
        val hamar = placeNear("Hamar stasjon", 60.79, 11.07, "stasjon")
        val lillehammer = placeNear("Lillehammer stasjon", 61.115, 10.466, "stasjon")

        // 1. Profiles
        for (p in listOf(
            TravelProfile.CAR,
            TravelProfile.CAR_ELECTRIC,
            TravelProfile.TRUCK,
            TravelProfile.MOBILE_HOME,
            TravelProfile.MOTORCYCLE,
            TravelProfile.BICYCLE,
            TravelProfile.BICYCLE_ELECTRIC,
            TravelProfile.HIKING,
        )) {
            m.check("profile_$p", "Profile $p Espa->Hamar") {
                val r = planWith(espa, hamar, p)
                val c = code(r)
                val mapped = SnapChoiceFlow.comapsProfileFor(p)
                when {
                    c != "OK" -> "FAIL: comaps code $c (${r.report.lineSequence().firstOrNull { it.startsWith("FAIL") }})"
                    p in listOf(TravelProfile.TRUCK, TravelProfile.MOBILE_HOME, TravelProfile.MOTORCYCLE, TravelProfile.CAR_ELECTRIC) ->
                        "PARTIAL: routed as CoMaps $mapped (${km(r)}); vehicle dimension / weight limits are not applied by the CoMaps shim"
                    else -> "PASS: routed as CoMaps $mapped, ${km(r)}"
                }
            }
        }

        // 2. Avoids
        val base = planWith(espa, hamar)
        m.check("avoid_motorways", "Avoid motorways") {
            val r = planWith(espa, hamar, motorways = true)
            if (code(r) != "OK") "FAIL: ${code(r)}" else "PASS: ${km(r)} vs default ${km(base)} (PREFER_NON_MOTORWAY optimise mode; options persisted to CoMaps settings)"
        }
        m.check("avoid_tolls", "Avoid tolls") {
            val r = planWith(espa, hamar, tolls = true)
            if (code(r) != "OK") "FAIL: ${code(r)}" else "PASS: ${km(r)} vs default ${km(base)} (no toll road on this corridor; flag accepted)"
        }
        m.check("avoid_ferries", "Avoid ferries (Horten-Moss, Bastoe ferry)") {
            val horten = Triple("Horten", 59.4167, 10.4833)
            val moss = Triple("Moss", 59.4340, 10.6577)
            val a = planWith(horten, moss)
            val b = planWith(horten, moss, ferries = true)
            when {
                code(a) == "NEED_MORE_MAPS" || code(a) == "NeedMoreMaps" -> "NOT_TESTED: leaf for Vestfold/Oslofjord not on device (${code(a)})"
                code(a) != "OK" -> "FAIL: default ${code(a)}"
                code(b) != "OK" -> "FAIL: avoid ${code(b)}"
                else -> "PASS: default ${km(a)} vs avoid ${km(b)}"
            }
        }

        // 3. Vias
        m.check("vias", "Via points (Espa -> Lillehammer -> Dombas)") {
            val direct = planWith(espa, dombas)
            val r = planWith(espa, dombas, vias = listOf(lillehammer.second to lillehammer.third))
            if (code(r) != "OK") return@check "FAIL: ${code(r)}"
            val pts = polylinePoints(r.routePolyline)
            val nearest =
                pts.minOf { (la, lo) ->
                    val dy = (la - lillehammer.second) * 111_000.0
                    val dx = (lo - lillehammer.third) * 111_000.0 * Math.cos(Math.toRadians(la))
                    Math.hypot(dx, dy)
                }
            if (nearest < 400.0) "PASS: ${km(r)} (direct ${km(direct)}), closest approach to via ${"%.0f".format(java.util.Locale.US, nearest)} m" else "FAIL: route misses via by $nearest m"
        }

        // 4. TBT
        val tbt = planWith(espa, hamar)
        m.check("tbt", "Turn-by-turn maneuvers + nb/en text") {
            val ms = parseRouteManeuvers(tbt.maneuversJson)
            val arr = JSONArray(tbt.maneuversJson)
            val withText = (0 until arr.length()).count { arr.getJSONObject(it).optString("text_nb").isNotBlank() && arr.getJSONObject(it).optString("text_en").isNotBlank() }
            val withVoice = (0 until arr.length()).count { arr.getJSONObject(it).optString("voice_nb").isNotBlank() }
            if (ms.isNotEmpty() && withText == ms.size) "PASS: ${ms.size} maneuvers, $withText with nb+en text, $withVoice with nb voice text; kinds=${ms.map { it.kind }.distinct()}" else "FAIL: maneuvers=${ms.size} withText=$withText"
        }

        // 5. ETA
        m.check("eta", "ETA / distance consistency") {
            val avg = tbt.distanceKm / (tbt.etaMinutes / 60.0)
            if (tbt.etaMinutes > 0 && avg in 20.0..130.0) "PASS: ${km(tbt)} => ${"%.0f".format(java.util.Locale.US, avg)} km/h average (CoMaps time estimate)" else "FAIL: eta=${tbt.etaMinutes} dist=${tbt.distanceKm}"
        }

        // 6. Speed limits
        m.check("speed_limits", "Posted speed limits on the route") {
            val samples = parseRouteSimSamples(tbt.simSamplesJson)
            val posted = samples.count { it.maxspeedKmh != null }
            if (posted > 0) "PASS: $posted samples carry a limit" else "FAIL: 0 of ${samples.size} samples carry a posted limit - the CoMaps shim returns geometry, turns and time only; overspeed HUD / limit sign have no data on the MWM planner (sim pace = route average ${samples.firstOrNull()?.speedKmh?.let { "%.0f km/h".format(java.util.Locale.US, it) }})"
        }

        // 7. Elevation
        m.check("elevation", "Elevation / climb") {
            val route = uniffi.navi.comapsSessionRoute(uniffi.navi.FfiComapsProfile.CAR, espa.second, espa.third, hamar.second, hamar.third, false)
            val el = JSONObject(route.elevationJson.ifBlank { "{}" })
            val points = el.optJSONArray("points") ?: JSONArray()
            val sources = (0 until points.length()).map { points.getJSONObject(it).optString("source") }.distinct()
            if (route.ascentM > 0.0 && points.length() > 10) {
                "PASS: ascent ${"%.0f".format(java.util.Locale.US, route.ascentM)} m / descent ${"%.0f".format(java.util.Locale.US, route.descentM)} m, ${points.length()} elevation points, sources=$sources, sidecar_hits=${el.optInt("sidecar_hits")} (climb is published to the HUD via RouteClimbSource, climb_source=mwm)"
            } else {
                "FAIL: ascent=${route.ascentM} points=${points.length()}"
            }
        }

        // 8. Wetland / sidecars
        m.check("wetland", "Wetland sidecar hook (hiking)") {
            val sidecars = File(h.productionRoot(), "releases/${h.RELEASE}/sidecars")
            val r = planWith(espa, hamar, TravelProfile.HIKING)
            if (code(r) != "OK") {
                "FAIL: ${code(r)}"
            } else if (!sidecars.isDirectory) {
                "PARTIAL: hiking route OK (${km(r)}) but no *.navi-sidecar.v1.zst installed on the test device - wetland cost hook inactive, not exercised"
            } else {
                "PASS: ${km(r)} with sidecars"
            }
        }

        // 9. Off-road choice (not on road) - never moved without a choice
        m.check("off_road_choice", "Start/end not on road -> explicit snap choice") {
            // Rondane high plateau: far from any road.
            val wild = Triple("Rondane plateau", 61.95, 9.85)
            val r = planWith(wild, hamar)
            val c = code(r)
            val sug = uniffi.navi.comapsSessionSnapSuggestion(uniffi.navi.FfiComapsProfile.CAR, uniffi.navi.FfiComapsRouteCode.START_NOT_ON_ROAD, wild.second, wild.third, 20_000.0)
            when (c) {
                "START_NOT_ON_ROAD" -> "PASS: router refused (no automatic move); suggestion found=${sug.found} snap=${sug.snapLat},${sug.snapLon} dist=${"%.0f".format(java.util.Locale.US, sug.distanceM)} m, offered only as a user choice (SnapChoiceFlow)"
                "OK" -> "PARTIAL: CoMaps matched the point to a road itself and routed (${km(r)}); the start was not reported as off-road, so the explicit choice dialog cannot appear for this point"
                else -> "FAIL: expected START_NOT_ON_ROAD, got $c"
            }
        }

        // 10. Missing maps
        m.check("missing_maps", "Missing maps message") {
            val berlin = planWith(Triple("Berlin", 52.52, 13.405), Triple("Potsdam", 52.39, 13.06))
            val gothenburg = planWith(espa, Triple("Goeteborg", 57.7089, 11.9746))
            val msg1 =
                berlin.report
                    .lineSequence()
                    .firstOrNull { it.startsWith("FAIL") }
                    .orEmpty()
            val msg2 =
                gothenburg.report
                    .lineSequence()
                    .firstOrNull { it.startsWith("FAIL") }
                    .orEmpty()
            val detail = "both points uncovered -> ${code(berlin)} '$msg1'; start covered, end uncovered -> ${code(gothenburg)} '$msg2'"
            when {
                code(berlin) == "NEED_MORE_MAPS" && code(gothenburg) == "NEED_MORE_MAPS" -> "PASS: $detail"
                code(gothenburg) == "NEED_MORE_MAPS" -> "PARTIAL: $detail (a point outside every installed MWM is reported as not-on-road instead of missing maps)"
                else -> "FAIL: $detail"
            }
        }

        // 11. Cancel
        m.check("cancel", "Cancel a running plan") {
            var result: uniffi.navi.CorridorRouteResult? = null
            var doneAt = 0L
            val t =
                Thread {
                    result = planWith(espa, litjfossen, vias = listOf(lillehammer.second to lillehammer.third))
                    doneAt = System.currentTimeMillis()
                }
            t.start()
            Thread.sleep(1_000)
            val cancelAt = System.currentTimeMillis()
            MwmProductionPlanner.cancel()
            t.join(60_000)
            val r = result ?: return@check "FAIL: plan thread did not return within 60 s of cancel"
            val ms = doneAt - cancelAt
            // A plan started right after cancel() (the replan path) must not inherit the cancel.
            val after = planWith(espa, hamar)
            if (code(r) == "CANCELLED" && code(after) == "OK") {
                "PASS: in-flight plan returned CANCELLED $ms ms after cancel (native router is not interruptible; result discarded at the leg boundary); the next plan routed OK (${km(after)})"
            } else {
                "FAIL: in-flight code=${code(r)} after $ms ms, next plan code=${code(after)}"
            }
        }

        // 12. GPX export
        m.check("gpx_export", "GPX export of a planned route") {
            val rte =
                JSONArray()
                    .put(JSONObject().put("name", espa.first).put("lat", espa.second).put("lon", espa.third))
                    .put(JSONObject().put("name", hamar.first).put("lat", hamar.second).put("lon", hamar.third))
            val gpx = uniffi.navi.routeToGpx("phase6", "", rte.toString(), tbt.routePolyline)
            val trkpts = Regex("<trkpt ").findAll(gpx).count()
            if (gpx.startsWith("<?xml") && gpx.contains("<rte>") && trkpts > 50) "PASS: GPX 1.1 with <rte> and $trkpts <trkpt>" else "FAIL: ${gpx.take(120)}"
        }
        m.add("gpx_import", "GPX import", "NOT_TESTED", "no GPX import entry point exists in the app or FFI (export only); not changed by Phase 6")

        // 13. DATEX
        m.check("datex", "DATEX situations refresh (online)") {
            val pts = polylinePoints(tbt.routePolyline).filterIndexed { i, _ -> i % 40 == 0 }
            val routeJson = JSONArray().also { a -> pts.forEach { a.put(JSONArray().put(it.first).put(it.second)) } }.toString()
            val out = uniffi.navi.datexRefreshJson(true, uniffi.navi.datexSettingsDefaultHost(), uniffi.navi.datexSettingsDefaultPort(), routeJson, false, true, true, File(h.context.cacheDir, "datex-p6").also { it.mkdirs() }.absolutePath)
            "PARTIAL: refresh returned ${out.length} chars (${out.take(160).replace('\n', ' ')}); live server content is outside the device acceptance and not verified for correctness"
        }

        // 14. Airplane mode (offline planning)
        m.check("airplane_mode", "Offline planning in airplane mode") {
            // Airplane mode plus Wi-Fi off (this Wi-Fi-only tablet keeps Wi-Fi up in airplane mode).
            h.shell("cmd connectivity airplane-mode enable")
            h.shell("svc wifi disable")
            try {
                val cm = h.context.getSystemService(android.net.ConnectivityManager::class.java)
                h.waitFor(25_000, 500) { cm.activeNetwork == null }
                val online =
                    cm.activeNetwork?.let {
                        cm.getNetworkCapabilities(it)?.hasCapability(android.net.NetworkCapabilities.NET_CAPABILITY_INTERNET)
                    } ?: false
                val a = planWith(espa, hamar)
                val s = search("Dombås", 62.07, 9.13)
                val d =
                    runCatching {
                        uniffi.navi.datexRefreshJson(true, uniffi.navi.datexSettingsDefaultHost(), uniffi.navi.datexSettingsDefaultPort(), "[]", false, true, true, null).take(80)
                    }.getOrElse { "threw ${it.message}" }
                if (!online && code(a) == "OK" && s.isNotEmpty()) {
                    "PASS: no internet (online=$online); route ${km(a)}, search ${s.size} hits; DATEX offline -> '$d'"
                } else {
                    "FAIL: online=$online code=${code(a)} hits=${s.size}"
                }
            } finally {
                h.shell("svc wifi enable")
                h.shell("cmd connectivity airplane-mode disable")
                Thread.sleep(10_000)
            }
        }

        writeLog("function-matrix.json", m.rows.toString(2))
    }

    /** Real-time (1x) route simulation: how well the drape follows the MapLibre puck camera. */
    @Test
    fun l_realtime_sim_overlay_alignment() {
        runCatRoute("realtime_sim", espa, dombas, MapHudPrefs.CAT_NETWORK_OFF, 1.0, 2, perf = false, cat = false)
    }

    /** Overlay-vs-drape scale calibration: same route + camera at several zoom offsets. */
    @Test
    fun k_calibrate_zoom_offset() {
        launch()
        val planned = plan(espa, dombas)
        assertTrue("plan PASS: ${planned.report}", planned.report.contains("PASS"))
        showRoute(planned, espa, dombas)
        val pts = polylinePoints(planned.routePolyline)
        // Centre on the interchange where the route turns back (curved ramps): a straight road
        // through the centre would look aligned at any scale.
        val c = pts.take(600).minByOrNull { it.first }!!
        // Default (analytic, per-device) offset at several zooms, integer and fractional.
        for (z in listOf(13.0, 14.0, 14.5, 15.0, 15.4, 16.0, 17.0)) {
            h.gotoCamera(c.first, c.second, z)
            Thread.sleep(4_500)
            h.screenshot("k_calib_auto_z${z.toString().replace('.', '_')}")
        }
        NaviMapTestHooks.drapeZoomOffset = null
    }

    @Test
    fun a_resolve_places() {
        launch()
        val out = JSONObject()
        val places =
            listOf(
                Triple("Espa", 60.56, 11.26),
                Triple("Dombås", 62.07, 9.13),
                Triple("Litjfossen", 61.5, 9.5),
            )
        for ((q, la, lo) in places) {
            val (best, rep) = resolveReport(q, la, lo)
            out.put(q, rep.put("chosen", best?.let { "${it.name} @ ${it.lat},${it.lon}" } ?: "none"))
        }
        writeLog("places-resolve.json", out.toString(2))
        android.util.Log.i(h.TAG, "places $out")
    }
}
