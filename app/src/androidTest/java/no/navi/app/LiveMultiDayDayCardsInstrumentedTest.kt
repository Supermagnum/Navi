package no.navi.app

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
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
import java.util.concurrent.TimeUnit
import java.util.regex.Pattern

/**
 * End-to-end multi-day day cards from a **live** UniFFI truck plan — not injected JSON.
 *
 * Pipeline: dumpsys GPS start → Norway PBF `planCarRoute` (Truck) → real `daysJson`
 * → [NaviMapTestHooks.pendingRoute] → [MultiDayPlanCards] → screenshot.
 *
 * Destination (Bodø) is chosen only after the live start is known and requires
 * multi-day EC 561 segmentation on the Norway extract (~1068 km class).
 *
 * **Hardware:** This test calls [planCarRoute] directly on `norway-latest.osm.pbf`
 * and deliberately bypasses the Tools UI country-download low-RAM warning. On
 * ~4 GB Automotive AVDs that path is expected to OOM (see README / architecture
 * country-scale constraint). The [BeforeClass] planner therefore assumes more
 * than 5 GB total RAM and skips on constrained devices instead of crashing the
 * instrumentation process.
 */
@RunWith(AndroidJUnit4::class)
class LiveMultiDayDayCardsInstrumentedTest {
    companion object {
        @JvmStatic
        lateinit var planned: CorridorRouteResult

        @JvmStatic
        var startLat: Double = 0.0

        @JvmStatic
        var startLon: Double = 0.0

        @JvmStatic
        var planElapsedMs: Long = 0L

        /** Skip direct Norway-PBF planning below this total RAM (bytes). */
        private const val MIN_TOTAL_RAM_FOR_NORWAY_PLAN = 5L * 1024L * 1024L * 1024L

        @JvmStatic
        @BeforeClass
        fun livePlanFromGpsAndPbf() {
            val context = InstrumentationRegistry.getInstrumentation().targetContext
            val am = context.getSystemService(android.app.ActivityManager::class.java)
            val mi = android.app.ActivityManager.MemoryInfo()
            am.getMemoryInfo(mi)
            org.junit.Assume.assumeTrue(
                "Live Norway truck planCarRoute on norway-latest.osm.pbf needs >5 GB RAM; " +
                    "this device reports totalMem=${mi.totalMem} (~${mi.totalMem / (1024 * 1024)} MiB). " +
                    "Country-scale loads are opt-in with a Tools UI low-RAM warning; this test " +
                    "exercises the unwarned direct UniFFI path and is skipped on constrained hardware.",
                mi.totalMem > MIN_TOTAL_RAM_FOR_NORWAY_PLAN,
            )

            val dataDir = NaviAppData.resolve(context).also { it.mkdirs() }

            // --- Live GPS from LocationManager dumpsys (device shell) ---
            val dumpsys = shellOutput("dumpsys location")
            val fix =
                parseDumpsysGpsFix(dumpsys)
                    ?: error(
                        "FATAL: no gps/fused last location in dumpsys — refuse hardcoded start\n" +
                            dumpsys.take(2000),
                    )
            startLat = fix.first
            startLon = fix.second
            android.util.Log.i(
                "LiveMultiDayDayCards",
                "dumpsys_gps start_lat=$startLat start_lon=$startLon",
            )

            // Destination after start is known: Bodø (requires Norway extract / multi-day).
            val endLat = 67.2804
            val endLon = 14.4049

            // Plan against the staged PBF in place — do not copy ~1.3G into app files
            // (OOM / disk pressure on the emulator).
            val stagedPbf = File("/data/local/tmp/navi_fixtures/norway-latest.osm.pbf")
            check(stagedPbf.isFile && stagedPbf.length() > 500_000_000L) {
                "missing Norway PBF at ${stagedPbf.absolutePath} (need ~1.3G push)"
            }
            check(stagedPbf.canRead()) {
                "staged Norway PBF not readable at ${stagedPbf.absolutePath}"
            }
            val pbf = stagedPbf

            val elevDir = File(dataDir, "elevation").also { it.mkdirs() }
            val stagedTar = File("/data/local/tmp/navi_fixtures/elevation-corridor.tar")
            if (stagedTar.isFile && !File(elevDir, "copernicus").exists()) {
                val tarProc =
                    ProcessBuilder(
                        "tar",
                        "-xf",
                        stagedTar.absolutePath,
                        "-C",
                        dataDir.absolutePath,
                    ).redirectErrorStream(true).start()
                tarProc.waitFor(120, TimeUnit.SECONDS)
            }

            val stagedCache = File("/data/local/tmp/navi_fixtures/truck-live-cache")
            // Writable cache dir under app files; seed from staged host cache when present.
            val cacheDir = File(dataDir, "graph-cache-norway-truck")
            cacheDir.mkdirs()
            if (stagedCache.isDirectory) {
                stagedCache.listFiles()?.forEach { f ->
                    val dest = File(cacheDir, f.name)
                    if (!dest.exists() || dest.length() != f.length()) {
                        f.copyRecursively(dest, overwrite = true)
                    }
                }
            }

            val vehicle =
                FfiVehicleLimits(
                    axleWeightKg = null,
                    bogieWeightKg = null,
                    heightM = null,
                    widthM = null,
                    lengthM = null,
                    totalWeightKg = null,
                )

            val t0 = System.currentTimeMillis()
            planned =
                planCarRoute(
                    pbfPath = pbf.absolutePath,
                    elevDir = elevDir.absolutePath,
                    cacheDir = cacheDir.absolutePath,
                    startLat = startLat,
                    startLon = startLon,
                    endLat = endLat,
                    endLon = endLon,
                    useEco = false,
                    profile = TravelProfile.TRUCK,
                    avoidMotorways = false,
                    avoidTolls = false,
                    avoidFerries = false,
                    vehicle = vehicle,
                    preferOfficialNetworks = false,
                )
            planElapsedMs = System.currentTimeMillis() - t0
            android.util.Log.i(
                "LiveMultiDayDayCards",
                "planCarRoute elapsed_ms=$planElapsedMs distance_km=${planned.distanceKm} " +
                    "daysJson_chars=${planned.daysJson.length}",
            )
            android.util.Log.i(
                "LiveMultiDayDayCards",
                "report_head=\n${planned.report.lines().take(40).joinToString("\n")}",
            )

            check(planned.report.contains("DATA_SOURCE=real_pbf") || planned.report.contains("PASS")) {
                planned.report.take(1500)
            }
            check(planned.report.contains("PASS")) { planned.report.take(1500) }
            check(planned.report.contains("hos_pack=ec561")) {
                "expected EC 561 pack for Norway start: ${planned.report}"
            }
            check(planned.report.contains("truck_multi_day:")) {
                "expected truck_multi_day in report (live segmentation)"
            }
            check(planned.distanceKm > 800.0) {
                "distance too short for multi-day Bodø corridor: ${planned.distanceKm}"
            }
            check(planned.daysJson.length > 10 && planned.daysJson != "[]") {
                "daysJson empty from live plan — UI schema gap: ${planned.daysJson}"
            }
            val cards = parseDaysJson(planned.daysJson)
            check(cards.size > 1) {
                "live daysJson must yield >1 day cards, got ${cards.size}: ${planned.daysJson.take(500)}"
            }
            // Cold or warm: must not be "instant inject" (< ~2s for Norway truck).
            // Warm cache can still be several seconds of POI/path work.
            check(planElapsedMs > 1_500L) {
                "plan finished too fast ($planElapsedMs ms) — suspect inject/bypass"
            }
        }

        private fun shellOutput(cmd: String): String {
            val pfd =
                InstrumentationRegistry
                    .getInstrumentation()
                    .uiAutomation
                    .executeShellCommand(cmd)
            return java.io
                .FileInputStream(pfd.fileDescriptor)
                .bufferedReader()
                .use { it.readText() }
                .also { pfd.close() }
        }

        /** Parse last gps or fused Location line from dumpsys location. */
        fun parseDumpsysGpsFix(dumpsys: String): Pair<Double, Double>? {
            // Examples: "last location=Location[gps 60.562480,11.256282 ...]"
            val patterns =
                listOf(
                    Pattern.compile(
                        """last location=Location\[(?:gps|fused)\s+(-?\d+\.\d+),(-?\d+\.\d+)""",
                        Pattern.CASE_INSENSITIVE,
                    ),
                    Pattern.compile(
                        """Location\[(?:gps|fused)\s+(-?\d+\.\d+),(-?\d+\.\d+)""",
                        Pattern.CASE_INSENSITIVE,
                    ),
                )
            for (p in patterns) {
                val m = p.matcher(dumpsys)
                var last: Pair<Double, Double>? = null
                while (m.find()) {
                    last = m.group(1).toDouble() to m.group(2).toDouble()
                }
                if (last != null) return last
            }
            return null
        }
    }

    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    @Before
    fun setUp() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.routeStartLabel = "Live GPS"
        NaviMapTestHooks.routeEndLabel = "Bodø"
    }

    @Test
    fun liveTruckPlan_dayCardsFromRealDaysJson_screenshot() {
        composeRule.waitForIdle()
        run {
            val styleDeadline = System.currentTimeMillis() + 45_000
            while (System.currentTimeMillis() < styleDeadline) {
                if (NaviMapTestHooks.styleReady || NaviMapTestHooks.lastReportedLayerCount >= 1) {
                    break
                }
                Thread.sleep(400)
            }
        }

        // Apply the **live** CorridorRouteResult (daysJson from planCarRoute), not synthetic.
        NaviMapTestHooks.pendingRoute = planned

        val deadline = System.currentTimeMillis() + 90_000
        var cardsVisible = false
        while (System.currentTimeMillis() < deadline) {
            composeRule.waitForIdle()
            try {
                composeRule
                    .onNodeWithTag("multi_day_plan_cards", useUnmergedTree = true)
                    .assertIsDisplayed()
                cardsVisible = true
                break
            } catch (_: Throwable) {
                NaviMapTestHooks.pendingRoute = planned
                NaviMapTestHooks.hideSearchChrome = false
                Thread.sleep(400)
            }
        }
        assertTrue(
            "day cards must render from live daysJson (${planned.daysJson.take(200)})",
            cardsVisible,
        )
        composeRule.onNodeWithTag("multi_day_card_1", useUnmergedTree = true).assertIsDisplayed()

        val cards = parseDaysJson(planned.daysJson)
        assertTrue(cards.size > 1)
        val day1 = cards.first()
        // First duty day can be short (remaining hours before daily rest); the
        // corridor as a whole must still be multi-day and long-haul.
        assertTrue(
            "some day should be a substantial drive segment",
            cards.any { it.distanceKm > 100.0 },
        )
        assertTrue(
            "day driving hours should respect ~9–10 h EC daily cap (got ${day1.drivingHours})",
            day1.drivingHours in 0.1..10.5,
        )
        assertTrue(
            "all day driving hours within EC daily extension cap",
            cards.all { it.drivingHours in 0.0..10.5 },
        )
        assertFalse("live daysJson must not be the old synthetic 720 km stub", day1.endKm == 720.0 && day1.distanceKm == 720.0)

        Thread.sleep(1_500)
        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertTrue(shot != null)
        assertNotEquals(0, shot!!.width)
        val dataDir =
            NaviAppData.resolve(
                InstrumentationRegistry.getInstrumentation().targetContext,
            )
        val out = File(dataDir, "multi_day_day_cards_live.png")
        out.outputStream().use { shot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        assertTrue(out.length() > 5_000)

        val pfd =
            InstrumentationRegistry
                .getInstrumentation()
                .uiAutomation
                .executeShellCommand("screencap -p /data/local/tmp/multi_day_day_cards_live.png")
        java.io.FileInputStream(pfd.fileDescriptor).use { input ->
            val buf = ByteArray(4096)
            while (input.read(buf) >= 0) {
            }
        }
        pfd.close()

        android.util.Log.i(
            "LiveMultiDayDayCards",
            "LIVE_OK start=$startLat,$startLon distance_km=${planned.distanceKm} " +
                "days=${cards.size} elapsed_ms=$planElapsedMs shot_bytes=${out.length()} " +
                "daysJson=${planned.daysJson.take(800)}",
        )
    }
}
