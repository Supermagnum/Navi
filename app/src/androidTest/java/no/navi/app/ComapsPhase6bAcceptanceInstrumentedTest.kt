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
 * Phase 6b on-device acceptance: CoMaps drape as the only map renderer (Samsung SM-P613, Navi only).
 *
 * Steps are ordered (a_, b_, ...). Step `a_` starts from an app data directory without any
 * maps (run `pm clear` first, see scripts/phase6-device-accept.sh) and downloads the Oestlandet
 * leaf set through the app's own downloader; no map data is pushed with adb. Later steps use
 * what that download installed.
 * Output: PNGs under /data/local/tmp/phase6/screenshots, JSON/text logs under
 * /data/local/tmp/phase6/logs (pulled to test-artifacts/phase6/ by the runner script).
 */
@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class ComapsPhase6bAcceptanceInstrumentedTest {
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
        assertTrue("session open (run step a_ first)", ComapsSessionBootstrap.ensureOpen(h.context, force = false))
        activityRule.launchActivity(null)
        // MainActivity resets the chrome hooks from the launch intent; set them afterwards.
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.hideUiChrome = chromeHidden
        assertTrue(
            "drape init",
            h.waitFor(90_000, 500) { NaviMapTestHooks.drapeState == "READY" || NaviMapTestHooks.drapeState == "FAILED" },
        )
        android.util.Log.i(h.TAG, "drape=${NaviMapTestHooks.drapeState} err=${NaviMapTestHooks.drapeError}")
        assertTrue("drape ready", NaviMapTestHooks.drapeState == "READY")
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

    // Start / end picked explicitly (a user choice among the candidates logged by z_resolve_places).
    private val espa = Triple("Espa E6", 60.562616550309635, 11.2546040956274)
    private val dombas = Triple("Dombås", 62.06881688813547, 9.125544347804379)
    private val litjfossen = Triple("Litjfossen", 62.66922983534084, 10.152202771247943)

    /**
     * Espa is the device position, not a map-picked point: Navi's own debug GPS hold (the same
     * mechanism as the in-app fake GPS) makes the app see Espa as the current GPS fix, and the
     * start of every route from Espa is read back from that fix. No third-party mock-location app.
     */
    private fun holdGps(point: Triple<String, Double, Double>): Triple<String, Double, Double> {
        NaviMapTestHooks.holdDebugGps(point.second, point.third)
        assertTrue(
            "app sees GPS at ${point.first}",
            h.waitFor(20_000, 250) { gpsIsAt(point) },
        )
        android.util.Log.i(h.TAG, "gps hold ${point.first} fix=${NaviMapTestHooks.lastGpsLat},${NaviMapTestHooks.lastGpsLon}")
        return Triple(point.first, NaviMapTestHooks.lastGpsLat, NaviMapTestHooks.lastGpsLon)
    }

    private fun gpsIsAt(point: Triple<String, Double, Double>): Boolean =
        NaviMapTestHooks.lastGpsLat.isFinite() &&
            Math.abs(NaviMapTestHooks.lastGpsLat - point.second) < 1e-4 &&
            Math.abs(NaviMapTestHooks.lastGpsLon - point.third) < 1e-4

    /** Routes that start at Espa must start from the GPS fix; fail loudly when no fix is held there. */
    private fun requireGpsStart(from: Triple<String, Double, Double>) {
        if (from.first == espa.first) assertTrue("route from Espa needs a GPS fix at Espa", gpsIsAt(from))
    }

    private fun plan(
        from: Triple<String, Double, Double>,
        to: Triple<String, Double, Double>,
        profile: TravelProfile = TravelProfile.CAR,
    ): uniffi.navi.CorridorRouteResult {
        requireGpsStart(from)
        return MwmProductionPlanner.plan(
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
    }

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
    fun m_visual_matrix() {
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
        rotation: MapRotationMode = MapRotationMode.NorthUp,
        tiltDeg: Double = 0.0,
        threeD: Boolean = false,
    ) {
        MwmClientBootstrap.ensureInitialized()
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
        val start = if (from.first == espa.first) holdGps(from) else from
        val planned = plan(start, to)
        android.util.Log.i(h.TAG, "$tag plan ${planned.report.replace('\n', ' ')}")
        assertTrue("plan PASS: ${planned.report}", planned.report.contains("PASS"))
        showRoute(planned, start, to)
        h.screenshot("${tag}_route_planned")
        NaviMapTestHooks.simulationTimeScale = timeScale
        NaviMapTestHooks.requestRotationMode = rotation
        if (tiltDeg > 0.0) NaviMapTestHooks.requestCameraTiltDeg = tiltDeg
        if (threeD) NaviMapTestHooks.requestOptIn3d = true
        NaviMapTestHooks.lastArrivedAtEnd = false
        val sampler = ComapsPhase6Harness.PerfSampler()
        if (perf) sampler.start()
        val t0 = System.currentTimeMillis()
        NaviMapTestHooks.requestStartRouteSimulation = true
        assertTrue("simulation active", h.waitFor(30_000, 250) { NaviMapTestHooks.simulatingActive })
        // Navigation view: follow the puck at street level instead of the route overview.
        assertTrue("first fix", h.waitFor(20_000, 250) { !NaviMapTestHooks.lastGpsLat.isNaN() })
        // Navigation view: follow the puck natively (the drape owns the camera; no camera pushes).
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
                    .put("drape_mode", NaviMapTestHooks.drapeMode)
                    .put("cam_level", NaviMapTestHooks.lastCameraZoom)
                    .put("cam_pitch", NaviMapTestHooks.lastCameraPitch)
                    .put("cat", if (cat) JSONObject(uniffi.navi.catPluginStatusJson()) else JSONObject.NULL),
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
        NaviMapTestHooks.clearDebugGpsInject()
        assertTrue("sim reached the end or ran to the cap", arrived || System.currentTimeMillis() >= deadline)
    }

    @Test
    fun n_cat_espa_dombas_auto_with_perf() {
        runCatRoute("espa_dombas_auto", espa, dombas, MapHudPrefs.CAT_NETWORK_AUTO, 12.0, 30, perf = true)
    }

    @Test
    fun o_cat_espa_litjfossen_auto() {
        runCatRoute("espa_litjfossen_auto", espa, litjfossen, MapHudPrefs.CAT_NETWORK_AUTO, 20.0, 30, perf = true)
    }

    @Test
    fun q_perf_drape_6min() {
        runCatRoute("perf_drape", espa, dombas, MapHudPrefs.CAT_NETWORK_OFF, 12.0, 6, perf = true, cat = false)
    }

    /** 20 min of real-speed (1x) navigation with heading-up, tilt and 3D: frame rate, memory, thermal. */
    @Test
    fun s_perf_20min_navigation() {
        runCatRoute(
            "perf_nav_20min",
            espa,
            dombas,
            MapHudPrefs.CAT_NETWORK_OFF,
            1.0,
            20,
            perf = true,
            cat = false,
            rotation = MapRotationMode.DirectionOfTravel,
            tiltDeg = 45.0,
            threeD = true,
        )
    }

    @Test
    fun p_cat_espa_dombas_standalone() {
        runCatRoute("espa_dombas_standalone", espa, dombas, MapHudPrefs.CAT_NETWORK_STANDALONE, 20.0, 30, perf = false)
    }

    // ---------------------------------------------------------------------------------------
    // Blockers (a-d)
    // ---------------------------------------------------------------------------------------

    private val ostlandetRegion = "europe_norway_ostlandet"

    /** Foot detour check points from docs/mwm-client-migration.md (Phase 4 sign-off). */
    private val detourFrom = Triple("detour start", 61.0500, 11.4700)
    private val detourTo = Triple("detour end", 61.0800, 11.5200)

    private fun waitInstall(
        label: String,
        timeoutMin: Long,
        timeline: JSONArray,
    ): MwmMapDownloader.State {
        val end = System.currentTimeMillis() + timeoutMin * 60_000L
        val t0 = System.currentTimeMillis()
        var last = ""
        while (System.currentTimeMillis() < end) {
            val st = MwmMapDownloader.state
            val line = "${st.label}|${st.message}|${st.percent}"
            if (line != last) {
                last = line
                timeline.put(
                    JSONObject()
                        .put("t_s", (System.currentTimeMillis() - t0) / 1000)
                        .put("label", st.label)
                        .put("progress", st.message)
                        .put("percent", st.percent ?: JSONObject.NULL),
                )
                android.util.Log.i(h.TAG, "install[$label] t=${(System.currentTimeMillis() - t0) / 1000}s ${st.message} ${st.percent}")
            }
            if (!st.running && st.ok != null) return st
            Thread.sleep(2_000)
        }
        return MwmMapDownloader.state
    }

    /** Phase 1 chain from an empty app dir, no adb push: download -> verify -> register -> route. */
    @Test
    fun a_download_from_scratch() {
        MwmClientBootstrap.ensureInitialized()
        assumeTrue("MWM flag off", MwmClientBootstrap.isEnabled())
        h.ensureShellDirs()
        val root = h.productionRoot()
        assertTrue("start from an empty mwm dir (pm clear first): ${root.list()?.toList()}", !File(root, "active.json").exists())
        val pid0 = android.os.Process.myPid()
        activityRule.launchActivity(null)
        NaviMapTestHooks.hideSearchChrome = true
        Thread.sleep(6_000)
        h.shell("screencap -p ${h.SHELL_DIR}/screenshots/a_no_maps_placeholder.png")
        h.shell("chmod 644 ${h.SHELL_DIR}/screenshots/a_no_maps_placeholder.png")
        assertTrue("no maps: drape must not be ready", NaviMapTestHooks.drapeState != "READY")

        val regions = uniffi.navi.mwmListRegions(uniffi.navi.defaultPackServerBaseUrl())
        assertTrue("catalog: ${regions.error}", regions.ok)
        val ost = regions.regions.first { it.id == ostlandetRegion }
        android.util.Log.i(h.TAG, "region ${ost.id} leaves=${ost.leafIds} bytes=${ost.totalBytes}")

        val timeline = JSONArray()
        val t0 = System.currentTimeMillis()
        android.os.Handler(android.os.Looper.getMainLooper()).post {
            MwmMapDownloader.install(h.context, ost.leafIds, ost.displayName)
        }
        val st = waitInstall("ostlandet", 90, timeline)
        val wallS = (System.currentTimeMillis() - t0) / 1000
        assertTrue("install ok=${st.ok} msg=${st.message}", st.ok == true)

        // Verify what is on disk.
        val inst = uniffi.navi.mwmInstalled(h.context.filesDir.absolutePath)
        val ver = inst.version ?: error("no active release")
        val sidecars =
            File(root, "releases/$ver/sidecars")
                .listFiles()
                .orEmpty()
                .map { it.name }
                .sorted()
        val leaves =
            File(root, "releases/$ver/leaves")
                .listFiles()
                .orEmpty()
                .map { it.name }
                .sorted()
        val sidecarBytes = File(root, "releases/$ver/sidecars").listFiles().orEmpty().sumOf { it.length() }
        val mwmBytes = File(root, "releases/$ver/leaves").listFiles().orEmpty().sumOf { it.length() }
        assertTrue("World.mwm + WorldCoasts.mwm at root", File(root, "World.mwm").isFile && File(root, "WorldCoasts.mwm").isFile)
        assertTrue("9 leaves", leaves.size == ost.leafIds.size)
        assertTrue("every leaf has a sidecar: $sidecars", sidecars.size == ost.leafIds.size)
        val sidecarRe = Regex("\\.navi-sidecar\\.v1\\.zst$")
        val leafSet = leaves.map { it.removeSuffix(".mwm") }.toSet()
        assertTrue("sidecar names match leaves", sidecars.all { leafSet.contains(it.replace(sidecarRe, "")) })

        // Live: the drape starts without an app restart and the router uses the sidecars.
        assertTrue("session open after install", ComapsSessionBootstrap.isOpen())
        assertTrue(
            "drape ready live (no restart)",
            h.waitFor(120_000, 500) { NaviMapTestHooks.drapeState == "READY" },
        )
        assertTrue("same process (no restart)", pid0 == android.os.Process.myPid())
        Thread.sleep(3_000)
        NaviMapTestHooks.hideSearchChrome = true
        h.gotoCamera(61.05, 11.47, 11.0)
        Thread.sleep(3_000)
        h.screenshot("a_after_download_live_drape")

        // Foot detour 13867 m (no wetland hook) -> 16245 m (hook), production planner.
        val noHook =
            uniffi.navi.comapsSessionRoute(
                uniffi.navi.FfiComapsProfile.PEDESTRIAN,
                detourFrom.second,
                detourFrom.third,
                detourTo.second,
                detourTo.third,
                false,
            )
        val planned = plan(detourFrom, detourTo, TravelProfile.HIKING)
        val hookM = Math.round(planned.distanceKm * 1000.0)
        val noHookM = Math.round(noHook.lengthM)
        android.util.Log.i(h.TAG, "detour noHook=$noHookM hook(planner)=$hookM report=${planned.report.replace('\n', ' ')}")
        writeLog(
            "wetland-chain.json",
            JSONObject()
                .put("wall_s", wallS)
                .put("version", ver)
                .put("leaves", JSONArray(leaves))
                .put("sidecars", JSONArray(sidecars))
                .put("mwm_bytes", mwmBytes)
                .put("sidecar_bytes", sidecarBytes)
                .put("download_timeline", timeline)
                .put("detour_no_hook_m", noHookM)
                .put("detour_hook_planner_m", hookM)
                .put("planner_report", planned.report)
                .toString(2),
        )
        assertTrue("foot detour without hook = 13867 m (got $noHookM)", noHookM == 13867L)
        assertTrue("foot detour with hook = 16245 m (got $hookM)", hookM == 16245L)
    }

    /**
     * Download another leaf while the drape is live and the router is open: no restart, resource
     * dir untouched (fonts / symbols still render), routing works, new leaf is drawn.
     */
    @Test
    fun b_live_download_no_restart() {
        launch(chromeHidden = false)
        val pid0 = android.os.Process.myPid()
        val before = uniffi.navi.mwmInstalled(h.context.filesDir.absolutePath)
        val leaf = "Norway_Rogaland"
        assertTrue("$leaf must not be installed before: ${before.leafIds}", leaf !in before.leafIds)
        val writable0 =
            org.json
                .JSONObject()
                .put(
                    "w",
                    app.organicmaps.sdk.Framework
                        .nativeGetWritableDir(),
                ).toString()
        h.gotoCamera(60.79, 11.07, 13.0)
        Thread.sleep(2_500)
        h.screenshot("b_before_live_download_hamar")
        val timeline = JSONArray()
        android.os.Handler(android.os.Looper.getMainLooper()).post {
            MwmMapDownloader.install(h.context, listOf(leaf), leaf)
        }
        val st = waitInstall("svalbard", 20, timeline)
        assertTrue("install ok=${st.ok} msg=${st.message}", st.ok == true)
        val after = uniffi.navi.mwmInstalled(h.context.filesDir.absolutePath)
        assertTrue("leaf installed", leaf in after.leafIds)
        assertTrue("old leaves kept", before.leafIds.all { it in after.leafIds })
        assertTrue("same process", pid0 == android.os.Process.myPid())
        assertTrue("drape still READY", NaviMapTestHooks.drapeState == "READY")
        assertTrue("session open", ComapsSessionBootstrap.isOpen())
        val writable1 =
            org.json
                .JSONObject()
                .put(
                    "w",
                    app.organicmaps.sdk.Framework
                        .nativeGetWritableDir(),
                ).toString()
        assertTrue("writable dir unchanged", writable0 == writable1)
        // Routing after the re-open.
        val hamar = placeNear("Hamar stasjon", 60.79, 11.07, "stasjon")
        holdGps(espa)
        val r = planWith(espa, hamar)
        NaviMapTestHooks.clearDebugGpsInject()
        assertTrue("route after live install: ${code(r)}", code(r) == "OK")
        // Drape text + symbols still render, and the new leaf draws.
        h.gotoCamera(60.79, 11.07, 13.0)
        Thread.sleep(2_500)
        h.screenshot("b_after_live_download_hamar")
        h.gotoCamera(58.97, 5.73, 11.0)
        Thread.sleep(4_000)
        h.screenshot("b_after_live_download_stavanger")
        writeLog(
            "live-download.json",
            JSONObject()
                .put("leaf", leaf)
                .put("pid", pid0)
                .put("leaves_before", JSONArray(before.leafIds))
                .put("leaves_after", JSONArray(after.leafIds))
                .put("route_after", km(r))
                .put("timeline", timeline)
                .toString(2),
        )
    }

    /** A point outside the installed maps must name the leaves to download, not "not on a road". */
    @Test
    fun c_missing_maps() {
        launch(chromeHidden = false)
        val out = JSONObject()

        fun leavesOf(r: uniffi.navi.CorridorRouteResult): String =
            Regex("missing_leaves=([^\\n]+)")
                .find(r.report)
                ?.groupValues
                ?.get(1)
                .orEmpty()
        val hamar = placeNear("Hamar stasjon", 60.79, 11.07, "stasjon")
        val cases =
            listOf(
                Triple("start_covered_end_stockholm", espa, Triple("Stockholm", 59.3293, 18.0686)),
                Triple("both_uncovered_berlin_potsdam", Triple("Berlin", 52.52, 13.405), Triple("Potsdam", 52.39, 13.06)),
                Triple("start_uncovered_end_covered", Triple("Trondheim", 63.4305, 10.3951), hamar),
            )
        var pass = 0
        holdGps(espa)
        for ((id, a, b) in cases) {
            val r = planWith(a, b)
            val c = code(r)
            val l = leavesOf(r)
            out.put(
                id,
                JSONObject().put("code", c).put("leaves", l).put(
                    "report",
                    r.report
                        .lineSequence()
                        .firstOrNull { it.startsWith("FAIL") }
                        .orEmpty(),
                ),
            )
            android.util.Log.i(h.TAG, "missing[$id] code=$c leaves=$l")
            if (c == "NEED_MORE_MAPS" && l.isNotBlank()) pass++
        }
        // Via point outside the installed maps.
        val via = planWith(espa, hamar, vias = listOf(59.3293 to 18.0686))
        out.put("via_uncovered", JSONObject().put("code", code(via)).put("leaves", leavesOf(via)))
        NaviMapTestHooks.clearDebugGpsInject()
        writeLog("missing-maps.json", out.toString(2))
        assertTrue("every uncovered case names its leaves: $out", pass == cases.size)
    }

    private fun samplesOf(r: uniffi.navi.CorridorRouteResult) = parseRouteSimSamples(r.simSamplesJson)

    /** Per-segment maxspeed from the MWM, on three routes with known limits, shown in the HUD. */
    @Test
    fun d_speed_limits() {
        launch(chromeHidden = false)
        val log = JSONObject()
        val hamar = placeNear("Hamar stasjon", 60.79, 11.07, "stasjon")
        val routes =
            listOf(
                // E6 motorway.
                Triple("e6", espa to hamar, setOf(80, 90, 100, 110)),
                // Rv3 Elverum - Rena.
                Triple("rv3", Triple("Elverum", 60.8814, 11.5624) to Triple("Rena", 61.1325, 11.3697), setOf(60, 70, 80)),
                // 30 / 50 km/h town streets (Lillehammer centre).
                Triple("town", Triple("Lillehammer stasjon", 61.1153, 10.4662) to Triple("Lillehammer torg", 61.1140, 10.4670), setOf(30, 40, 50)),
            )
        var passed = 0
        for ((id, ends0, expected) in routes) {
            // The E6 route starts at Espa: Espa is the GPS fix. The others start elsewhere.
            val ends =
                if (ends0.first.first == espa.first) {
                    holdGps(ends0.first) to ends0.second
                } else {
                    NaviMapTestHooks.clearDebugGpsInject()
                    ends0
                }
            val r = planWith(ends.first, ends.second)
            val c = code(r)
            val samples = samplesOf(r)
            val limits = samples.mapNotNull { it.maxspeedKmh?.toInt() }
            val distinct = limits.distinct().sorted()
            val covered = if (samples.isEmpty()) 0.0 else 100.0 * limits.size / samples.size
            val hit = distinct.any { it in expected }
            // HUD: play the route and read the limit / overspeed the HUD shows at a few points.
            val hud = JSONArray()
            if (c == "OK" && distinct.isNotEmpty()) {
                showRoute(r, ends.first, ends.second)
                NaviMapTestHooks.simulationTimeScale = 1.0
                NaviMapTestHooks.requestStartRouteSimulation = true
                h.waitFor(30_000, 250) { NaviMapTestHooks.simulatingActive }
                val total = samples.last().cumM
                for (f in listOf(0.1, 0.3, 0.5, 0.7, 0.9)) {
                    NaviMapTestHooks.requestSimSeekCumM = total * f
                    Thread.sleep(3_500)
                    hud.put(
                        JSONObject()
                            .put("at_pct", (f * 100).toInt())
                            .put("hud_limit_kmh", NaviMapTestHooks.lastCurrentSpeedLimitKmh ?: JSONObject.NULL)
                            .put("hud_speed_kmh", NaviMapTestHooks.lastSimSpeedKmh)
                            .put("overspeed", NaviMapTestHooks.lastOverspeed)
                            .put("sample_limit_kmh", samples.minByOrNull { Math.abs(it.cumM - total * f) }?.maxspeedKmh ?: JSONObject.NULL),
                    )
                    if (f == 0.5) h.screenshot("d_speed_limit_hud_$id", 8_000)
                }
                NaviMapTestHooks.requestStopRouteSimulation = true
                Thread.sleep(1_500)
            }
            log.put(
                id,
                JSONObject()
                    .put("code", c)
                    .put("km", km(r))
                    .put("samples", samples.size)
                    .put("samples_with_limit_pct", "%.1f".format(java.util.Locale.US, covered))
                    .put("distinct_limits_kmh", JSONArray(distinct))
                    .put("expected_any_of", JSONArray(expected.toList()))
                    .put("match", hit)
                    .put("hud", hud),
            )
            android.util.Log.i(h.TAG, "speed[$id] $c limits=$distinct covered=$covered hit=$hit")
            if (hit) passed++
        }
        NaviMapTestHooks.clearDebugGpsInject()
        writeLog("speed-limits.json", log.toString(2))
        assertTrue("limits found on all 3 routes: $log", passed == routes.size)
    }

    /**
     * Route + overlays (route line with turn arrows, start / end pins, repeater sites, DATEX items)
     * and the navigation views: north-up follow, heading-up follow, tilted heading-up (3D).
     */
    @Test
    fun r_nav_views() {
        provisionCat(MapHudPrefs.CAT_NETWORK_AUTO)
        MapHudPrefs.saveDatexPluginEnabled(h.context, true)
        launch(chromeHidden = false)
        assertTrue("cat plugin enabled", h.waitFor(60_000, 500) { uniffi.navi.catPluginIsEnabled() })
        val log = JSONArray()

        fun note(name: String) {
            val c = ComapsDrape.backend?.camera()
            log.put(
                JSONObject()
                    .put("shot", name)
                    .put("camera", c?.toString())
                    .put("mode", NaviMapTestHooks.drapeMode)
                    .put("zoom", NaviMapTestHooks.lastCameraZoom)
                    .put("bearing", NaviMapTestHooks.lastCameraBearing)
                    .put("pitch", NaviMapTestHooks.lastCameraPitch)
                    .put("off_route", NaviMapTestHooks.lastOffRoute)
                    .put("auto_reroutes", NaviMapTestHooks.autoRerouteTriggeredCount)
                    .put("approach", NaviMapTestHooks.lastApproachPhase.toString())
                    .put("gps", "${NaviMapTestHooks.lastGpsLat},${NaviMapTestHooks.lastGpsLon}"),
            )
        }

        val start = holdGps(espa)
        val planned = plan(start, dombas)
        assertTrue("plan PASS: ${planned.report}", planned.report.contains("PASS"))
        showRoute(planned, start, dombas)
        Thread.sleep(8_000)
        h.screenshot("r_route_overview_overlays")
        note("r_route_overview_overlays")

        // DATEX items near the start (Espa has a live roadwork item). Repeater sites are never on the map.
        h.gotoCamera(60.58, 11.27, 11.5)
        Thread.sleep(6_000)
        h.screenshot("r_overlay_datex_z11")
        note("r_overlay_datex_z11")
        h.gotoCamera(60.5826, 11.2733, 14.5)
        Thread.sleep(4_000)
        h.screenshot("r_overlay_datex_z14")
        note("r_overlay_datex_z14")

        // Navigation views start from a street-level view (follow keeps the current zoom above the
        // country scale; the navigation camera adds perspective only at street zoom).
        h.gotoCamera(60.5626, 11.2546, 16.0)
        Thread.sleep(3_000)
        NaviMapTestHooks.simulationTimeScale = 1.0
        NaviMapTestHooks.requestRotationMode = MapRotationMode.NorthUp
        NaviMapTestHooks.requestStartRouteSimulation = true
        assertTrue("simulation active", h.waitFor(30_000, 250) { NaviMapTestHooks.simulatingActive })
        assertTrue("first fix", h.waitFor(20_000, 250) { !NaviMapTestHooks.lastGpsLat.isNaN() })
        // No seek: a teleport along the route is not a drive and would trigger off-route handling.
        NaviMapTestHooks.requestRecenterGps = true
        Thread.sleep(25_000)
        h.screenshot("r_follow_north_up", 10_000)
        note("r_follow_north_up")

        NaviMapTestHooks.requestRotationMode = MapRotationMode.DirectionOfTravel
        Thread.sleep(12_000)
        h.screenshot("r_follow_heading_up", 10_000)
        note("r_follow_heading_up")

        NaviMapTestHooks.requestCameraTiltDeg = 45.0
        NaviMapTestHooks.requestOptIn3d = true
        Thread.sleep(15_000)
        h.screenshot("r_nav_tilted_heading_up_3d", 10_000)
        note("r_nav_tilted_heading_up_3d")

        NaviMapTestHooks.requestRotationMode = MapRotationMode.Compass
        Thread.sleep(8_000)
        h.screenshot("r_follow_compass_mode", 10_000)
        note("r_follow_compass_mode")

        NaviMapTestHooks.requestStopRouteSimulation = true
        NaviMapTestHooks.requestCameraTiltDeg = 0.0
        NaviMapTestHooks.requestOptIn3d = false
        NaviMapTestHooks.clearDebugGpsInject()
        writeLog("nav-views.json", log.toString(2))
    }

    /**
     * Activity recreation / configuration changes: rotation (handled in-process), explicit recreate, dark
     * mode switch (the system recreates the activity), automatic sunrise/sunset theme, and the
     * Vulkan or GLES engine that is actually running.
     */
    @Test
    fun t_recreation_and_config_changes() {
        launch(chromeHidden = false)
        val log = JSONArray()
        val pid = android.os.Process.myPid()
        val inst =
            androidx.test.platform.app.InstrumentationRegistry
                .getInstrumentation()

        fun snap(name: String) {
            val be = ComapsDrape.backend
            val c = be?.camera()
            log.put(
                JSONObject()
                    .put("step", name)
                    .put("pid", android.os.Process.myPid())
                    .put("drape_state", NaviMapTestHooks.drapeState)
                    .put("rendering", be?.renderingActive())
                    .put("night", be?.isNight())
                    .put("camera", c?.toString()),
            )
            android.util.Log.i(h.TAG, "t_snap $name rendering=${be?.renderingActive()} night=${be?.isNight()} cam=$c")
        }

        fun ui(block: () -> Unit) = inst.runOnMainSync(block)

        fun alive() = ComapsDrape.backend?.renderingActive() == true

        h.gotoCamera(60.79, 11.07, 15.0)
        Thread.sleep(3_000)
        h.screenshot("t_baseline")
        snap("baseline")
        val baseCam = ComapsDrape.backend?.camera()

        // Rotation: portrait -> landscape -> portrait.
        ui { activityRule.activity.requestedOrientation = android.content.pm.ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE }
        assertTrue("landscape rendering", h.waitFor(15_000, 250) { alive() })
        Thread.sleep(4_000)
        h.screenshot("t_landscape")
        snap("landscape")
        val landCam = ComapsDrape.backend?.camera()
        assertTrue(
            "camera centre kept across rotation: $baseCam -> $landCam",
            baseCam != null && landCam != null && Math.abs(baseCam.lat - landCam.lat) < 0.005 && Math.abs(baseCam.lon - landCam.lon) < 0.005,
        )
        ui { activityRule.activity.requestedOrientation = android.content.pm.ActivityInfo.SCREEN_ORIENTATION_PORTRAIT }
        assertTrue("portrait rendering", h.waitFor(15_000, 250) { alive() })
        Thread.sleep(3_000)
        snap("portrait_again")

        // Explicit recreate.
        ui { activityRule.activity.recreate() }
        Thread.sleep(6_000)
        assertTrue("rendering after recreate", h.waitFor(20_000, 250) { alive() })
        h.screenshot("t_after_recreate")
        snap("after_recreate")
        assertTrue("same process", pid == android.os.Process.myPid())

        // Dark mode switch (system recreates the activity: uiMode is not in configChanges).
        h.shell("cmd uimode night yes")
        assertTrue("drape night after dark mode", h.waitFor(20_000, 250) { ComapsDrape.backend?.isNight() == true })
        Thread.sleep(3_000)
        h.screenshot("t_dark_mode")
        snap("dark_mode")
        h.shell("cmd uimode night no")
        assertTrue("drape day after light mode", h.waitFor(20_000, 250) { ComapsDrape.backend?.isNight() == false })
        Thread.sleep(3_000)
        h.screenshot("t_light_mode")
        snap("light_mode")

        // Automatic by sunrise / sunset at the current position (system light, local night in November).
        MapHudPrefs.saveDrapeDayNight(h.context, DrapeDayNight.AUTO_SUN)
        ui { activityRule.activity.recreate() }
        Thread.sleep(6_000)
        val night = h.waitFor(20_000, 250) { ComapsDrape.backend?.isNight() == true }
        val hour =
            java.util.Calendar
                .getInstance()
                .get(java.util.Calendar.HOUR_OF_DAY)
        h.screenshot("t_auto_sun")
        snap("auto_sun hour=$hour night=$night")
        MapHudPrefs.saveDrapeDayNight(h.context, DrapeDayNight.SYSTEM)
        val sunLat = NaviMapTestHooks.lastGpsLat
        val sunLon = NaviMapTestHooks.lastGpsLon
        val expectNight = ComapsDrape.backend?.isDayAt(sunLat, sunLon, System.currentTimeMillis() / 1000L) == false
        assertTrue("auto sun follows CoMaps day/night at the position (expectNight=$expectNight night=$night)", night == expectNight)
        writeLog("recreation.json", log.toString(2))
    }

    /** Diagnostic: installed leaves with sizes and modification times. */
    @Test
    fun z_diag_installed() {
        val root = h.productionRoot()
        val ver = uniffi.navi.mwmInstalled(h.context.filesDir.absolutePath).version ?: "?"
        val fmt = java.text.SimpleDateFormat("HH:mm:ss", java.util.Locale.US)
        val rows =
            File(root, "releases/$ver/leaves").listFiles().orEmpty().sortedBy { it.name }.map {
                "${it.name} ${it.length()} ${fmt.format(java.util.Date(it.lastModified()))}"
            } +
                File(root, "releases/$ver/sidecars").listFiles().orEmpty().sortedBy { it.name }.map {
                    "${it.name} ${it.length()} ${fmt.format(java.util.Date(it.lastModified()))}"
                }
        rows.forEach { android.util.Log.i(h.TAG, "diag $it") }
        writeLog("diag-installed.txt", rows.joinToString("\n"))
    }

    /** Diagnostic: camera hook moves the drape. */
    @Test
    fun z_diag_camera() {
        launch(chromeHidden = false)
        for ((la, lo, z) in listOf(Triple(60.79, 11.07, 13.0), Triple(61.115, 10.466, 15.0), Triple(59.91, 10.75, 12.0))) {
            val ok = h.gotoCamera(la, lo, z)
            Thread.sleep(2_500)
            val c = ComapsDrape.backend?.camera()
            android.util.Log.i(h.TAG, "diagcam target=$la,$lo,$z ok=$ok cam=$c state=${NaviMapTestHooks.lastCameraLat},${NaviMapTestHooks.lastCameraLon}")
        }
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
    ): uniffi.navi.CorridorRouteResult {
        requireGpsStart(from)
        return planWithGpsChecked(from, to, profile, vias, motorways, tolls, ferries, bikeWetland)
    }

    private fun planWithGpsChecked(
        from: Triple<String, Double, Double>,
        to: Triple<String, Double, Double>,
        profile: TravelProfile,
        vias: List<Pair<Double, Double>>,
        motorways: Boolean,
        tolls: Boolean,
        ferries: Boolean,
        bikeWetland: Boolean,
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
        holdGps(espa)
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
            val distinct = samples.mapNotNull { it.maxspeedKmh?.toInt() }.distinct().sorted()
            if (posted > 0) "PASS: $posted of ${samples.size} samples carry a posted limit, distinct=$distinct (MWM maxspeeds section via the shim; HUD + overspeed use them)" else "FAIL: 0 of ${samples.size} samples carry a posted limit"
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
            val inst = uniffi.navi.mwmInstalled(h.context.filesDir.absolutePath)
            val noHook = uniffi.navi.comapsSessionRoute(uniffi.navi.FfiComapsProfile.PEDESTRIAN, detourFrom.second, detourFrom.third, detourTo.second, detourTo.third, false)
            val hook = plan(detourFrom, detourTo, TravelProfile.HIKING)
            val a = Math.round(noHook.lengthM)
            val b = Math.round(hook.distanceKm * 1000.0)
            when {
                inst.sidecarCount.toInt() == 0 -> "FAIL: no sidecars installed"
                a == 13867L && b == 16245L -> "PASS: foot detour $a m -> $b m with ${inst.sidecarCount} sidecars (production planner, from the in-app download)"
                else -> "FAIL: detour $a m -> $b m, expected 13867 -> 16245"
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
            val gothenburg = planWith(espa, Triple("Stockholm", 59.3293, 18.0686))
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
                code(berlin) == "NEED_MORE_MAPS" &&
                    code(gothenburg) == "NEED_MORE_MAPS" &&
                    berlin.report.contains("missing_leaves=") &&
                    gothenburg.report.contains("missing_leaves=") ->
                    "PASS: $detail"
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
            val json = JSONObject(out)
            val active = json.getJSONArray("active")
            val withGeometry = (0 until active.length()).count { active.getJSONObject(it).getJSONArray("geometry").length() > 0 }
            val source = json.optString("data_source", "none")
            if (source != "none" && active.length() > 0 && withGeometry > 0) {
                "PASS: data_source=$source, ${active.length()} active situations on the route ($withGeometry with geometry, drawn as drape marks); content correctness is the server's"
            } else {
                "PARTIAL: data_source=$source, ${active.length()} active ($withGeometry with geometry); nothing to draw on this route right now"
            }
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

        NaviMapTestHooks.clearDebugGpsInject()
        writeLog("function-matrix.json", m.rows.toString(2))
    }

    @Test
    fun z_resolve_places() {
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
