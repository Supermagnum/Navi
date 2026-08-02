package no.navi.app

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.CorridorRouteResult
import java.io.File

/**
 * Injects a multi-day `daysJson` corridor via [NaviMapTestHooks] and captures
 * the day-card UI under search chrome (same shot pattern as [RouteHudScreenshotTest]).
 *
 * Ready without a live routing run; requires an emulator/device for the shot.
 * If the emulator is unavailable, leave this test ready and skip the run.
 */
@RunWith(AndroidJUnit4::class)
class MultiDayDayCardsScreenshotTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var dataDir: File

    @Before
    fun setUp() {
        dataDir =
            NaviAppData
                .resolve(
                    InstrumentationRegistry.getInstrumentation().targetContext,
                ).also { it.mkdirs() }
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
    }

    private fun multiDayRoute(): CorridorRouteResult {
        // Corridor labels match the live-GPS Minnesund→Bodø truck confirmation;
        // day metrics are representative multi-day EC 561 segments including a
        // reduced-weekly compensation ledger line on day 1.
        val days =
            """
            [
              {"day_index":1,"date":"2026-07-24","start_km":0.0,"end_km":720.0,"distance_km":720.0,
               "driving_hours":9.0,"profile":"truck","rest_kind":"weekly_reduced","rest_hours":24.0,
               "rest_label":"Weekly rest 24 h (reduced)","overnight_name":"Services Minnesund","overnight_found":true,
               "not_in_cab":false,"compensation":"Compensation pending: 21 h by 2026-08-16","is_final":false},
              {"day_index":2,"date":"2026-07-25","start_km":720.0,"end_km":1068.0,"distance_km":348.0,
               "driving_hours":4.4,"profile":"truck","rest_kind":"","rest_hours":0.0,
               "rest_label":"","overnight_name":"","overnight_found":false,
               "not_in_cab":false,"compensation":"","is_final":true}
            ]
            """.trimIndent()
        return CorridorRouteResult(
            report =
                "TEST_KIND=MULTI_DAY_CARDS\nhos_pack=ec561\n" +
                    "truck_compensation: pending=true; shortfall_h=21; " +
                    "compensate_by=2026-08-16\nPASS\n",
            distanceKm = 1068.0,
            etaMinutes = 15.85 * 60.0,
            cacheHit = true,
            coldBuildS = 0.0,
            warmLoadS = 0.0,
            routePolyline = "11.256,60.562;11.5,61.5;12.5,63.5;14.405,67.280",
            poiLat = 67.2804,
            poiLon = 14.4049,
            poiName = "Bodø",
            poiIconKey = "fuel",
            breakPoisJson = """[{"name":"Services Minnesund","lat":60.8,"lon":11.2,"kind":"rest_area"}]""",
            daysJson = days,
            simSamplesJson = "[]",
            maneuversJson = "[]",
            priorityPathSharePct = 0.0,
            routeSegmentsJson = "[]",
            offTrailAdvisory = "",
        )
    }

    private fun hikingMultiDayRoute(): CorridorRouteResult {
        val days =
            """
            [
              {"day_index":1,"date":"","start_km":0.0,"end_km":38.5,"distance_km":38.5,
               "driving_hours":0.0,"profile":"hiking","rest_kind":"overnight_hut","rest_hours":0.0,
               "rest_label":"Overnight hut","overnight_name":"Jammerdalsbu","overnight_found":true,
               "not_in_cab":false,"compensation":"","is_final":false},
              {"day_index":2,"date":"","start_km":38.5,"end_km":78.0,"distance_km":39.5,
               "driving_hours":0.0,"profile":"hiking","rest_kind":"","rest_hours":0.0,
               "rest_label":"","overnight_name":"","overnight_found":false,
               "not_in_cab":false,"compensation":"","is_final":true}
            ]
            """.trimIndent()
        return CorridorRouteResult(
            report = "TEST_KIND=MULTI_DAY_HIKING_CARDS\nhiking_multi_day: days=2\nPASS\n",
            distanceKm = 78.0,
            etaMinutes = 78.0 * 16.0,
            cacheHit = true,
            coldBuildS = 0.0,
            warmLoadS = 0.0,
            routePolyline = "10.0,61.0;10.1,61.2;10.2,61.4",
            poiLat = 61.4,
            poiLon = 10.2,
            poiName = "Rondvassbu",
            poiIconKey = "cabin",
            breakPoisJson = """[{"name":"Jammerdalsbu","lat":61.2,"lon":10.1,"kind":"hut"}]""",
            daysJson = days,
            simSamplesJson = "[]",
            maneuversJson = "[]",
            priorityPathSharePct = 0.0,
            routeSegmentsJson = "[]",
            offTrailAdvisory = "",
        )
    }

    @Test
    fun multiDay_dayCards_visible_and_screenshot() {
        composeRule.waitForIdle()
        run {
            val styleDeadline = System.currentTimeMillis() + 45_000
            while (System.currentTimeMillis() < styleDeadline) {
                if (NaviMapTestHooks.styleReady || NaviMapTestHooks.lastReportedLayerCount >= 1) break
                Thread.sleep(400)
            }
        }

        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.routeStartLabel = "Minnesund"
        NaviMapTestHooks.routeEndLabel = "Bodø"
        NaviMapTestHooks.pendingRoute = multiDayRoute()

        val deadline = System.currentTimeMillis() + 60_000
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
                NaviMapTestHooks.pendingRoute = multiDayRoute()
                NaviMapTestHooks.hideSearchChrome = false
                Thread.sleep(400)
            }
        }
        assertTrue("multi-day day cards should be visible in search chrome", cardsVisible)
        composeRule.onNodeWithTag("multi_day_card_1", useUnmergedTree = true).assertIsDisplayed()
        composeRule.onNodeWithTag("multi_day_card_2", useUnmergedTree = true).assertIsDisplayed()

        fun shell(cmd: String) {
            val pfd = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
            java.io.FileInputStream(pfd.fileDescriptor).use { input ->
                val buf = ByteArray(4096)
                while (input.read(buf) >= 0) {
                }
            }
            pfd.close()
        }

        Thread.sleep(1_200)
        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertTrue("null screenshot", shot != null)
        assertNotEquals(0, shot!!.width)
        val out = File(dataDir, "multi_day_day_cards.png")
        out.outputStream().use { shot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        assertTrue("shot too small (${out.length()})", out.length() > 5_000)
        shell("screencap -p /data/local/tmp/multi_day_day_cards.png")
        shell("chmod 644 /data/local/tmp/multi_day_day_cards.png")
        // Host may pull into docs/images/ after the run:
        // adb pull /data/local/tmp/multi_day_day_cards.png docs/images/
        android.util.Log.i(
            "MultiDayDayCardsScreenshotTest",
            "shot=multi_day_day_cards.png bytes=${out.length()}",
        )
    }

    @Test
    fun hikingMultiDay_dayCards_visible_and_screenshot() {
        composeRule.waitForIdle()
        run {
            val styleDeadline = System.currentTimeMillis() + 45_000
            while (System.currentTimeMillis() < styleDeadline) {
                if (NaviMapTestHooks.styleReady || NaviMapTestHooks.lastReportedLayerCount >= 1) break
                Thread.sleep(400)
            }
        }

        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.routeStartLabel = "Aakersaetra"
        NaviMapTestHooks.routeEndLabel = "Rondvassbu"
        NaviMapTestHooks.pendingRoute = hikingMultiDayRoute()

        val deadline = System.currentTimeMillis() + 60_000
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
                NaviMapTestHooks.pendingRoute = hikingMultiDayRoute()
                Thread.sleep(400)
            }
        }
        assertTrue("hiking multi-day day cards should be visible", cardsVisible)

        Thread.sleep(1_200)
        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertTrue(shot != null)
        val out = File(dataDir, "multi_day_day_cards_hiking.png")
        out.outputStream().use { shot!!.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        assertTrue(out.length() > 5_000)
        val pfd =
            InstrumentationRegistry
                .getInstrumentation()
                .uiAutomation
                .executeShellCommand("screencap -p /data/local/tmp/multi_day_day_cards_hiking.png")
        java.io.FileInputStream(pfd.fileDescriptor).use { input ->
            val buf = ByteArray(4096)
            while (input.read(buf) >= 0) {
            }
        }
        pfd.close()
    }
}
