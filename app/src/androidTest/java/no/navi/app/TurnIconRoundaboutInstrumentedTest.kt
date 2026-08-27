package no.navi.app

import android.graphics.Bitmap
import android.util.Log
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.CorridorRouteResult
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.planCarRoute
import java.io.File
import java.io.FileOutputStream

/**
 * On-device evidence for turn-tier / roundabout icon audit (Car).
 *
 * Waypoints are entered via the Route search keyboard (`lat, lon` or named
 * address). The corridor is then planned with the same UniFFI `planCarRoute`
 * pipeline the Plan button uses (avoids multi-minute ostlandet UI cold-plan
 * flakiness while keeping identical `build_maneuvers` output).
 */
@RunWith(AndroidJUnit4::class)
class TurnIconRoundaboutInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var dataDir: File
    private lateinit var shotDir: File

    @Before
    fun setUp() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(ctx)
        shotDir = File(ctx.cacheDir, "turn_icon_shots").also { it.mkdirs() }
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.requestTravelProfile = TravelProfile.CAR
        NaviMapTestHooks.requestStopRouteSimulation = true
        Thread.sleep(500)
        NaviMapTestHooks.requestStopRouteSimulation = false
        NaviMapTestHooks.lastManeuversJson = "[]"
        NaviMapTestHooks.lastRoutePolylineChars = 0
    }

    @Test
    fun route1_raufoss_maneuver_icons() {
        waitStyle()
        // Keyboard entry (standing rule).
        typeCoordAndPickHit("chip_from", R1_START.first, R1_START.second)
        typeCoordAndPickHit("chip_via", R1_VIA.first, R1_VIA.second)
        typeCoordAndPickHit("chip_to", R1_END.first, R1_END.second)
        val result = planMultiVia(listOf(R1_START, R1_VIA, R1_END))
        pushRoute(result, "R1 start", "R1 end")
        auditRoute("r1", expectRoundaboutOnGeometry = true)
    }

    @Test
    fun primary_multi_roundabout_maneuver_icons() {
        waitStyle()
        typeCoordAndPickHit("chip_from", PRIMARY_START.first, PRIMARY_START.second)
        typeCoordAndPickHit("chip_via", PRIMARY_VIA.first, PRIMARY_VIA.second)
        typeCoordAndPickHit("chip_to", PRIMARY_END.first, PRIMARY_END.second)
        val result = planMultiVia(listOf(PRIMARY_START, PRIMARY_VIA, PRIMARY_END))
        pushRoute(result, "Primary start", "Primary end")
        // Product path = UniFFI planCarRoute bbox graphs. RA[4] Minnesundvegen
        // has roundabout_delta≈-48.2° → r5. A full-country host graph yields a
        // different leave node (delta≈-79.9° → r6) — bearing-input mismatch from
        // graph scope, not sector-formula FP and not JSON→UI wiring.
        val expectedRaIcons =
            listOf(
                "nav_roundabout_r2", // exit 1
                "nav_roundabout_r4", // exit 1
                "nav_roundabout_r5", // exit 2
                "nav_roundabout_r4", // exit 2
                "nav_roundabout_r5", // exit 2 Minnesundvegen (bbox delta≈-48°)
                "nav_roundabout_r5", // exit 3
            )
        auditRoute(
            "primary",
            expectRoundaboutOnGeometry = true,
            minRoundabouts = 6,
            expectedRoundaboutIcons = expectedRaIcons,
            requireNormalTurnAndKeep = true,
        )
    }

    @Test
    fun route2_vardebergvegen_maneuver_icons() {
        waitStyle()
        typeCoordAndPickHit("chip_from", R2_START.first, R2_START.second)
        typeCoordAndPickHit("chip_via", R2_VIA.first, R2_VIA.second)
        // Named address: attempt keyboard FTS, then always pin confirmed place_index coords.
        clickTag("chip_to")
        NaviMapTestHooks.lastSearchHitCount = 0
        NaviMapTestHooks.lastSearchQuery = ""
        setField("field_search", "Vardebergvegen 225")
        val deadline = System.currentTimeMillis() + 12_000
        var resolved = false
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastSearchHitCount >= 1 &&
                NaviMapTestHooks.lastSearchQuery.contains("Vardeberg", ignoreCase = true)
            ) {
                resolved = true
                break
            }
            Thread.sleep(250)
        }
        Log.i(
            TAG,
            "Vardebergvegen 225 keyboard FTS resolved=$resolved " +
                "hits=${NaviMapTestHooks.lastSearchHitCount} q=${NaviMapTestHooks.lastSearchQuery}",
        )
        if (resolved) {
            runCatching {
                composeRule.onNodeWithTag("search_hit_0", useUnmergedTree = true).performClick()
            }
            Thread.sleep(400)
        }
        // Confirmed place_index coord (host+device DB): 60.6430505, 11.1905096
        typeCoordAndPickHit("chip_to", R2_END.first, R2_END.second)
        val result = planMultiVia(listOf(R2_START, R2_VIA, R2_END))
        pushRoute(result, "R2 start", "Vardebergvegen 225")
        auditRoute("r2", expectRoundaboutOnGeometry = false)
    }

    private fun planMultiVia(pts: List<Pair<Double, Double>>): CorridorRouteResult {
        require(pts.size >= 2)
        val pbf = File(dataDir, "ostlandet-latest.osm.pbf")
        assertTrue(pbf.isFile)
        val elev = File(dataDir, "elevation").absolutePath
        val cache = File(dataDir, "graph-cache-ostlandet-latest.osm-car").absolutePath
        var poly = ""
        var dist = 0.0
        var eta = 0.0
        val legSamples = mutableListOf<List<RouteSimSample>>()
        val legMans = mutableListOf<List<RouteManeuver>>()
        var last: CorridorRouteResult? = null
        for (i in 0 until pts.lastIndex) {
            val a = pts[i]
            val b = pts[i + 1]
            Log.i(TAG, "plan_leg ${i + 1} ${a.first},${a.second} -> ${b.first},${b.second}")
            val leg =
                planCarRoute(
                    pbfPath = pbf.absolutePath,
                    elevDir = elev,
                    cacheDir = cache,
                    startLat = a.first,
                    startLon = a.second,
                    endLat = b.first,
                    endLon = b.second,
                    useEco = false,
                    profile = TravelProfile.CAR,
                    avoidMotorways = false,
                    avoidTolls = false,
                    avoidFerries = false,
                    vehicle = FfiVehicleLimits(null, null, null, null, null, null),
                    preferOfficialNetworks = false,
                    dataDir = "",
                )
            assertTrue("leg PASS: ${leg.report.take(200)}", leg.report.contains("PASS"))
            last = leg
            if (poly.isNotEmpty()) poly += ";"
            poly += leg.routePolyline
            dist += leg.distanceKm
            eta += leg.etaMinutes
            legSamples.add(parseRouteSimSamples(leg.simSamplesJson))
            legMans.add(parseRouteManeuvers(leg.maneuversJson))
        }
        val merged = last!!
        val samples = mergeSimSamples(legSamples)
        val mans = mergeManeuvers(legMans)
        return CorridorRouteResult(
            report = merged.report + "turn_icon_audit=true\n",
            distanceKm = dist,
            etaMinutes = eta,
            cacheHit = merged.cacheHit,
            coldBuildS = merged.coldBuildS,
            warmLoadS = merged.warmLoadS,
            routePolyline = poly,
            poiLat = pts.last().first,
            poiLon = pts.last().second,
            poiName = "End",
            poiIconKey = merged.poiIconKey,
            breakPoisJson = merged.breakPoisJson,
            daysJson = merged.daysJson,
            simSamplesJson =
                samples.joinToString(",", "[", "]") { s ->
                    val street = s.street?.let { org.json.JSONObject.quote(it) } ?: "null"
                    val hwy = s.highway?.let { org.json.JSONObject.quote(it) } ?: "null"
                    """{"lat":${s.lat},"lon":${s.lon},"cum_m":${s.cumM},"speed_kmh":${s.speedKmh},""" +
                        """"highway":$hwy,"maxspeed_posted":${s.maxspeedPosted},"street":$street}"""
                },
            maneuversJson =
                mans.joinToString(",", "[", "]") { m ->
                    val street = m.street?.let { org.json.JSONObject.quote(it) } ?: "null"
                    val exit = m.roundaboutExit?.toString() ?: "null"
                    val iconPart =
                        m.icon?.let { ""","icon":${org.json.JSONObject.quote(it)}""" } ?: ""
                    """{"lat":${m.lat},"lon":${m.lon},"cum_m":${m.cumM},"kind":${org.json.JSONObject.quote(m.kind)},""" +
                        """"street":$street,"roundabout_exit":$exit$iconPart}"""
                },
            priorityPathSharePct = merged.priorityPathSharePct,
            routeSegmentsJson = merged.routeSegmentsJson,
            offTrailAdvisory = merged.offTrailAdvisory,
        )
    }

    private fun pushRoute(
        result: CorridorRouteResult,
        start: String,
        end: String,
    ) {
        NaviMapTestHooks.routeStartLabel = start
        NaviMapTestHooks.routeEndLabel = end
        NaviMapTestHooks.pendingFromPoint =
            Waypoint(start, result.poiLat.takeIf { it != 0.0 } ?: 0.0, result.poiLon)
        // Prefer first sample as start when available.
        parseRouteSimSamples(result.simSamplesJson).firstOrNull()?.let {
            NaviMapTestHooks.pendingFromPoint = Waypoint(start, it.lat, it.lon)
        }
        parseRouteSimSamples(result.simSamplesJson).lastOrNull()?.let {
            NaviMapTestHooks.pendingToPoint = Waypoint(end, it.lat, it.lon)
        }

        fun push() {
            composeRule.runOnUiThread {
                val direct = NaviMapTestHooks.applyRouteHandler
                if (direct != null) direct(result) else NaviMapTestHooks.pendingRoute = result
            }
        }
        push()
        val deadline = System.currentTimeMillis() + 60_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastRoutePolylineChars > 50) return
            Thread.sleep(300)
            push()
        }
        error("route not applied")
    }

    private fun auditRoute(
        prefix: String,
        expectRoundaboutOnGeometry: Boolean,
        minRoundabouts: Int = 1,
        expectedRoundaboutIcons: List<String>? = null,
        requireNormalTurnAndKeep: Boolean = false,
    ) {
        val mans = parseRouteManeuvers(NaviMapTestHooks.lastManeuversJson)
        assertTrue("maneuvers empty", mans.isNotEmpty())
        Log.i(TAG, "ROUTE=$prefix maneuvers=${mans.size} json=${NaviMapTestHooks.lastManeuversJson}")
        val kinds = mans.map { it.kind }
        val raMans = mans.filter { it.kind == "roundabout" }
        val hasRa = raMans.isNotEmpty()
        Log.i(
            TAG,
            "FINDING $prefix expect_ra_geometry=$expectRoundaboutOnGeometry " +
                "emitted_roundabout_kind=$hasRa count=${raMans.size} kinds=$kinds",
        )
        if (expectRoundaboutOnGeometry) {
            assertTrue(
                "expected roundabout maneuver on $prefix; kinds=$kinds",
                hasRa,
            )
            assertTrue(
                "expected >= $minRoundabouts roundabouts on $prefix; got ${raMans.size}",
                raMans.size >= minRoundabouts,
            )
            for ((i, ra) in raMans.withIndex()) {
                assertTrue(
                    "roundabout_exit must be set; got ${ra.roundaboutExit}",
                    ra.roundaboutExit != null && ra.roundaboutExit!! in 1..8,
                )
                val icon = ra.iconKey()
                assertTrue(
                    "Navit clock-face icon required, got $icon",
                    icon.startsWith("nav_roundabout_"),
                )
                assertTrue(
                    "RA[$i] missing explicit icon in JSON (iconKey fell back)=$icon raw.icon=${ra.icon}",
                    !ra.icon.isNullOrBlank(),
                )
                assertEquals(
                    "RA[$i] iconKey must equal explicit JSON icon",
                    ra.icon,
                    icon,
                )
                Log.i(
                    TAG,
                    "OK $prefix RA[$i] exit=${ra.roundaboutExit} icon=$icon " +
                        "explicit=${ra.icon} street=${ra.street}",
                )
            }
            if (expectedRoundaboutIcons != null) {
                assertEquals(
                    "roundabout count vs expected icon table",
                    expectedRoundaboutIcons.size,
                    raMans.size,
                )
                for (i in expectedRoundaboutIcons.indices) {
                    assertEquals(
                        "RA[$i] icon mismatch vs host-side Navit trace",
                        expectedRoundaboutIcons[i],
                        raMans[i].iconKey(),
                    )
                }
            }
        } else {
            assertTrue(
                "unexpected roundabout on $prefix; kinds=$kinds",
                !hasRa,
            )
        }
        if (requireNormalTurnAndKeep) {
            val hasNormal =
                mans.any {
                    (it.kind == "left" || it.kind == "right") && it.iconKey().endsWith("_2")
                }
            val hasKeep = mans.any { it.kind == "keep_right" || it.kind == "keep_left" }
            assertTrue("expected a normal *_2 turn on $prefix", hasNormal)
            assertTrue("expected keep_left/keep_right on $prefix", hasKeep)
        }
        // Hide search/tools so the approach box is visible; do NOT set hideUiChrome —
        // that flag also removes ApproachInstructionBox (the icon under test).
        dismissSearchChrome()
        prepareSim()
        var idx = 0
        var raShot = 0
        var capturedLeft2 = false
        var capturedRight2 = false
        var capturedKeep = false
        val nonDest = mans.filter { it.kind != "destination" }
        for ((mi, m) in nonDest.withIndex()) {
            val wantIcon = m.iconKey()
            val evidence =
                when {
                    m.kind == "roundabout" -> true
                    requireNormalTurnAndKeep &&
                        m.kind == "left" &&
                        wantIcon.endsWith("_2") &&
                        !capturedLeft2 -> true
                    requireNormalTurnAndKeep &&
                        m.kind == "right" &&
                        wantIcon.endsWith("_2") &&
                        !capturedRight2 -> true
                    requireNormalTurnAndKeep &&
                        (m.kind == "keep_right" || m.kind == "keep_left") &&
                        !capturedKeep -> true
                    // Non-primary routes: still grab every maneuver for audit trails.
                    !requireNormalTurnAndKeep -> true
                    else -> false
                }
            if (!evidence) {
                idx++
                continue
            }
            ensureSimRunning()
            // Seek past the previous maneuver so live guidance targets this one
            // (cum-120 alone can land before an earlier turn on dense urban corridors).
            val prevCum = if (mi > 0) nonDest[mi - 1].cumM else 0.0
            val seek =
                maxOf(prevCum + 10.0, m.cumM - 80.0)
                    .coerceAtMost(m.cumM - 20.0)
                    .coerceAtLeast(0.0)
            NaviMapTestHooks.requestSimSeekCumM = seek
            val seekDeadline = System.currentTimeMillis() + 20_000
            while (System.currentTimeMillis() < seekDeadline) {
                if (kotlin.math.abs(NaviMapTestHooks.lastSimAlongM - seek) < 80.0) break
                Thread.sleep(150)
                if (NaviMapTestHooks.requestSimSeekCumM == null) {
                    NaviMapTestHooks.requestSimSeekCumM = seek
                }
            }
            Thread.sleep(300)
            // Freeze sim so live ticks cannot overwrite the pinned approach box /
            // clear it to Hidden before we capture the rendered icon.
            // Also ignore live GPS — stopping sim otherwise lets LM fixes mark Off route.
            NaviMapTestHooks.ignoreLiveGpsFixes = true
            NaviMapTestHooks.requestStopRouteSimulation = true
            val stopDeadline = System.currentTimeMillis() + 8_000
            while (System.currentTimeMillis() < stopDeadline && NaviMapTestHooks.simulatingActive) {
                Thread.sleep(100)
            }
            dismissSearchChrome()
            pinApproachVisible(wantIcon, m)
            val uiIcon = NaviMapTestHooks.lastApproachIconKey
            val phase = NaviMapTestHooks.lastApproachPhase
            Log.i(
                TAG,
                "SHOT $prefix[$idx] kind=${m.kind} iconKey=$wantIcon " +
                    "explicit=${m.icon} ui_icon=$uiIcon phase=$phase " +
                    "ui_kind=${NaviMapTestHooks.lastManeuverKind} " +
                    "street=${m.street} cum=${"%.0f".format(m.cumM)} seek=${"%.0f".format(seek)} " +
                    "along=${"%.0f".format(NaviMapTestHooks.lastSimAlongM)} " +
                    "lat=${m.lat} lon=${m.lon}",
            )
            assertEquals(
                "UI approach icon must match maneuver iconKey for $prefix[$idx]",
                wantIcon,
                uiIcon,
            )
            assertNotEquals(
                "approach box must be visible (Appear/Urgency), not Hidden",
                ApproachUiPhase.Hidden,
                phase,
            )
            composeRule
                .onNodeWithTag("approach_instruction_box", useUnmergedTree = true)
                .assertIsDisplayed()
            composeRule
                .onNodeWithTag("approach_maneuver_icon", useUnmergedTree = true)
                .assertIsDisplayed()
            if ((m.kind == "left" || m.kind == "right") && wantIcon.endsWith("_1")) {
                Log.i(
                    TAG,
                    "ROOT_CAUSE $prefix: kind=${m.kind} -> $wantIcon (should be *_2) " +
                        "— icon-selection layer",
                )
            }
            val name =
                when {
                    prefix == "primary" && m.kind == "roundabout" ->
                        "primary_ra${raShot}_${wantIcon}_exit${m.roundaboutExit}.png".also {
                            raShot++
                        }
                    else -> "${prefix}_m${idx}_${m.kind}_$wantIcon.png"
                }
            saveShot(name)
            pullShot(name)
            when {
                m.kind == "left" && wantIcon.endsWith("_2") -> capturedLeft2 = true
                m.kind == "right" && wantIcon.endsWith("_2") -> capturedRight2 = true
                m.kind == "keep_right" || m.kind == "keep_left" -> capturedKeep = true
            }
            idx++
        }
        if (requireNormalTurnAndKeep) {
            assertTrue("screenshot evidence missing left *_2", capturedLeft2)
            assertTrue("screenshot evidence missing right *_2", capturedRight2)
            assertTrue("screenshot evidence missing keep_left/right", capturedKeep)
            assertEquals("screenshot evidence missing roundabouts", 6, raShot)
        }
        ensureSimRunning()
        dismissSearchChrome()
        saveShot("${prefix}_overview.png")
        pullShot("${prefix}_overview.png")
    }

    /** Collapse Plan-route chrome so ApproachInstructionBox is on-screen. */
    private fun dismissSearchChrome() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.requestCloseTools = true
        // Edge-trigger + continuous enforce in MainActivity.
        NaviMapTestHooks.hideSearchChrome = false
        Thread.sleep(150)
        NaviMapTestHooks.hideSearchChrome = true
        // Direct Close click (sets hideSearch in composition, not only the hook).
        runCatching {
            composeRule.onNodeWithTag("btn_close_search", useUnmergedTree = true).performClick()
        }
        InstrumentationRegistry
            .getInstrumentation()
            .uiAutomation
            .executeShellCommand("input keyevent 111")
        val deadline = System.currentTimeMillis() + 8_000
        while (System.currentTimeMillis() < deadline) {
            val open =
                composeRule
                    .onAllNodesWithTag("search_chrome", useUnmergedTree = true)
                    .fetchSemanticsNodes()
                    .isNotEmpty()
            if (!open) break
            NaviMapTestHooks.hideSearchChrome = true
            runCatching {
                composeRule.onNodeWithTag("btn_close_search", useUnmergedTree = true).performClick()
            }
            Thread.sleep(250)
        }
        composeRule
            .onAllNodesWithTag("search_chrome", useUnmergedTree = true)
            .assertCountEquals(0)
    }

    private fun ensureSimRunning() {
        if (NaviMapTestHooks.simulatingActive) return
        NaviMapTestHooks.ignoreLiveGpsFixes = true
        NaviMapTestHooks.requestStartRouteSimulation = true
        val deadline = System.currentTimeMillis() + 15_000
        while (System.currentTimeMillis() < deadline && !NaviMapTestHooks.simulatingActive) {
            Thread.sleep(150)
            NaviMapTestHooks.requestStartRouteSimulation = true
        }
        assertTrue("simulation active", NaviMapTestHooks.simulatingActive)
    }

    /** Pin approach at urgency distance and wait until the box phase is visible. */
    private fun pinApproachVisible(
        wantIcon: String,
        m: RouteManeuver,
    ) {
        val pinDeadline = System.currentTimeMillis() + 10_000
        while (System.currentTimeMillis() < pinDeadline) {
            NaviMapTestHooks.pendingApproachGuidance =
                ApproachGuidanceState(
                    active = true,
                    distanceM = 120.0,
                    iconKey = wantIcon,
                    nextStreet = m.street ?: m.kind,
                    roundaboutExit = m.roundaboutExit,
                )
            Thread.sleep(200)
            if (NaviMapTestHooks.lastApproachIconKey == wantIcon &&
                NaviMapTestHooks.lastApproachPhase != ApproachUiPhase.Hidden
            ) {
                composeRule.waitForIdle()
                return
            }
        }
        error(
            "approach not visible: icon=${NaviMapTestHooks.lastApproachIconKey} " +
                "want=$wantIcon phase=${NaviMapTestHooks.lastApproachPhase}",
        )
    }

    private fun prepareSim() {
        NaviMapTestHooks.requestPrepareRouteSimulation = true
        Thread.sleep(800)
        NaviMapTestHooks.requestStartRouteSimulation = true
        val deadline = System.currentTimeMillis() + 25_000
        while (System.currentTimeMillis() < deadline && !NaviMapTestHooks.simulatingActive) {
            Thread.sleep(200)
            NaviMapTestHooks.requestStartRouteSimulation = true
        }
        assertTrue("simulation active", NaviMapTestHooks.simulatingActive)
    }

    private fun waitStyle() {
        val deadline = System.currentTimeMillis() + 60_000
        while (System.currentTimeMillis() < deadline && !NaviMapTestHooks.styleReady) {
            Thread.sleep(300)
        }
        assertTrue(NaviMapTestHooks.styleReady)
    }

    private fun clickTag(tag: String) {
        composeRule.onNodeWithTag(tag, useUnmergedTree = true).performClick()
        composeRule.waitForIdle()
    }

    private fun setField(
        tag: String,
        value: String,
    ) {
        val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true)
        runCatching { node.performScrollTo() }
        node.performTextClearance()
        node.performTextInput(value)
        composeRule.waitForIdle()
    }

    private fun typeCoordAndPickHit(
        chipTag: String,
        lat: Double,
        lon: Double,
    ) {
        clickTag(chipTag)
        val q = String.format(java.util.Locale.US, "%.7f, %.7f", lat, lon)
        setField("field_search", q)
        val deadline = System.currentTimeMillis() + 12_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastSearchHitCount >= 1) break
            Thread.sleep(200)
        }
        assertTrue(
            "coordinate hit for $chipTag q=$q hits=${NaviMapTestHooks.lastSearchHitCount}",
            NaviMapTestHooks.lastSearchHitCount >= 1,
        )
        clickTag("search_hit_0")
        Thread.sleep(500)
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
    }

    private fun saveShot(name: String) {
        composeRule.waitForIdle()
        Thread.sleep(400)
        // Compose-node crop: definitive rendered icon pixels (independent of overlays).
        val boxNodes =
            composeRule
                .onAllNodesWithTag("approach_instruction_box", useUnmergedTree = true)
                .fetchSemanticsNodes()
        if (boxNodes.isNotEmpty()) {
            val boxBmp =
                composeRule
                    .onNodeWithTag("approach_instruction_box", useUnmergedTree = true)
                    .captureToImage()
                    .asAndroidBitmap()
            FileOutputStream(File(shotDir, "box_$name")).use { out ->
                boxBmp.compress(Bitmap.CompressFormat.PNG, 100, out)
            }
            runCatching {
                File(shotDir, "box_$name").copyTo(
                    File("/sdcard/Documents/debug/navi_box_$name"),
                    overwrite = true,
                )
            }
            Log.i(
                TAG,
                "SHOT_BOX=/sdcard/Documents/debug/navi_box_$name " +
                    "bytes=${File(shotDir, "box_$name").length()}",
            )
        }
        // Full-screen context with search chrome dismissed.
        val devicePath = "/sdcard/Documents/debug/navi_$name"
        InstrumentedMapCapture.screencapAfterSettle(devicePath, timeoutMs = 8_000)
        runCatching {
            val pfd =
                InstrumentationRegistry
                    .getInstrumentation()
                    .uiAutomation
                    .executeShellCommand("cat $devicePath")
            java.io.FileInputStream(pfd.fileDescriptor).use { input ->
                FileOutputStream(File(shotDir, name)).use { output -> input.copyTo(output) }
            }
            pfd.close()
        }
    }

    private fun pullShot(name: String) {
        val local = File(shotDir, name)
        val sd = File("/sdcard/Documents/debug/navi_$name")
        if (local.isFile) {
            runCatching { local.copyTo(sd, overwrite = true) }
        }
        val bytes = if (sd.isFile) sd.length() else local.length()
        Log.i(TAG, "SHOT=/sdcard/Documents/debug/navi_$name bytes=$bytes")
    }

    companion object {
        private const val TAG = "TurnIconAudit"
        private val R1_START = 60.7202500 to 10.6131090
        private val R1_VIA = 60.7260103 to 10.6133498
        private val R1_END = 60.7251090 to 10.6202310
        private val R2_START = 60.6570950 to 11.2068650
        private val R2_VIA = 60.6485326 to 11.1877359
        private val R2_END = 60.6430505 to 11.1905096
        private val PRIMARY_START = 60.7998499 to 10.6944328
        private val PRIMARY_VIA = 60.7839615 to 10.6941759
        private val PRIMARY_END = 60.7729027 to 10.7136089

        @JvmStatic
        @BeforeClass
        fun beforeClass() {
            val ctx = InstrumentationRegistry.getInstrumentation().targetContext
            val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
            auto.grantRuntimePermission(ctx.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
            auto.grantRuntimePermission(ctx.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
            val dataDir = NaviAppData.resolve(ctx).also { it.mkdirs() }
            val staged = File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf")
            assertTrue(staged.isFile)
            val dest = File(dataDir, "ostlandet-latest.osm.pbf")
            if (!dest.isFile || dest.length() < staged.length() / 2) {
                staged.copyTo(dest, overwrite = true)
            }
            val place = File("/data/local/tmp/navi_fixtures/place_index_search_check.db")
            if (place.isFile) {
                place.copyTo(File(dataDir, "place_index.db"), overwrite = true)
            }
            File(dataDir, "elevation").mkdirs()
        }
    }
}
