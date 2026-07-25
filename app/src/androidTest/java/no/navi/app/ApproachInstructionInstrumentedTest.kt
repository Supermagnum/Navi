package no.navi.app

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
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
import java.io.File

/**
 * Approach-instruction box with a **host-planned** car route (not a synthetic stub):
 * Grimåsfeltet (Raufoss) → Nysethvegen / Tollerud from ostlandet via
 * `raufoss_approach_route`. Local device shots only — do **not** commit approach
 * PNGs under `docs/images/` (GitHub image allowlist is in `docs/pictures.md`).
 */
@RunWith(AndroidJUnit4::class)
class ApproachInstructionInstrumentedTest {

    companion object {
        @JvmStatic
        @BeforeClass
        fun beforeClass() {
            val pkg = InstrumentationRegistry.getInstrumentation().targetContext.packageName
            runCatching {
                InstrumentationRegistry.getInstrumentation().uiAutomation
                    .grantRuntimePermission(pkg, android.Manifest.permission.ACCESS_FINE_LOCATION)
            }
            NaviMapTestHooks.hideUiChrome = false
            NaviMapTestHooks.hideSearchChrome = true
            NaviMapTestHooks.pendingApproachGuidance = null
            NaviMapTestHooks.pendingRoute = null
            NaviMapTestHooks.styleReady = false
        }

        // Grimåsfeltet suburb (OSM place) — FTS has no "2368" housenumber there.
        private const val START_LAT = 60.7163834
        private const val START_LON = 10.6202916
        // Nysethvegen 10, Tollerud / Raufoss
        private const val END_LAT = 60.7278207
        private const val END_LON = 10.6049538
    }

    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var dataDir: File

    @Before
    fun setUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.gpsAltitudeM = 412.0
        NaviMapTestHooks.pendingApproachGuidance = null
        NaviMapTestHooks.pendingRoute = null
        // Mid-route camera between Grimåsfeltet and Nysethvegen
        NaviMapTestHooks.pendingCamera = Triple(
            (START_LAT + END_LAT) / 2.0,
            (START_LON + END_LON) / 2.0,
            13.5,
        )
    }

    private fun waitStyle() {
        val deadline = System.currentTimeMillis() + 25_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady) break
            Thread.sleep(200)
        }
        assertTrue("MapLibre style not ready", NaviMapTestHooks.styleReady)
        Thread.sleep(1_500)
    }

    private fun loadPlannedPolyline(): String {
        val ctx = InstrumentationRegistry.getInstrumentation().context
        return ctx.assets.open("raufoss_grimafeltet_nysethvegen.polyline.txt")
            .bufferedReader()
            .use { it.readText().trim() }
            .also { assertTrue("planned polyline empty", it.contains(';')) }
    }

    private fun injectPlannedRoute() {
        val polyline = loadPlannedPolyline()
        NaviMapTestHooks.pendingRoute = CorridorRouteResult(
            report = "PLANNED Grimåsfeltet → Nysethvegen (Raufoss / Tollerud)",
            distanceKm = 1.953,
            etaMinutes = 1.953 / 50.0 * 60.0,
            cacheHit = true,
            coldBuildS = 0.0,
            warmLoadS = 0.0,
            routePolyline = polyline,
            poiLat = END_LAT,
            poiLon = END_LON,
            poiName = "Nysethvegen",
            poiIconKey = "fuel",
            breakPoisJson = "[]",
            daysJson = "[]",
            simSamplesJson = "[]",
            maneuversJson = "[]",
            priorityPathSharePct = 0.0,
        )
        NaviMapTestHooks.pendingCamera = Triple(
            (START_LAT + END_LAT) / 2.0,
            (START_LON + END_LON) / 2.0,
            13.5,
        )
        Thread.sleep(1_500)
    }

    private fun bitmapHasRouteRed(bmp: Bitmap): Boolean {
        val stepX = (bmp.width / 48).coerceAtLeast(1)
        val stepY = (bmp.height / 48).coerceAtLeast(1)
        var y = 0
        while (y < bmp.height) {
            var x = 0
            while (x < bmp.width) {
                val c = bmp.getPixel(x, y)
                val r = Color.red(c)
                val g = Color.green(c)
                val b = Color.blue(c)
                if (r > 160 && g < 90 && b < 90) return true
                x += stepX
            }
            y += stepY
        }
        return false
    }

    private fun shot(name: String) {
        composeRule.waitForIdle()
        Thread.sleep(800)
        val bmp = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertTrue("screenshot null for $name", bmp != null)
        assertNotEquals(0, bmp!!.width)
        assertNotEquals(0, bmp.height)
        assertTrue(
            "expected visible red planned route in $name",
            bitmapHasRouteRed(bmp),
        )
        val out = File(dataDir, "$name.png")
        out.outputStream().use { os ->
            bmp.compress(Bitmap.CompressFormat.PNG, 90, os)
        }
        bmp.recycle()
        assertTrue("$name written", out.isFile && out.length() > 3_000)
        runCatching {
            val pub = File("/sdcard/Download/navi_approach").also { it.mkdirs() }
            out.copyTo(File(pub, "$name.png"), overwrite = true)
        }
        runCatching {
            val values = android.content.ContentValues().apply {
                put(android.provider.MediaStore.MediaColumns.DISPLAY_NAME, "$name.png")
                put(android.provider.MediaStore.MediaColumns.MIME_TYPE, "image/png")
                put(
                    android.provider.MediaStore.MediaColumns.RELATIVE_PATH,
                    android.os.Environment.DIRECTORY_DOWNLOADS + "/navi_approach",
                )
                put(android.provider.MediaStore.MediaColumns.IS_PENDING, 1)
            }
            val resolver = InstrumentationRegistry.getInstrumentation().targetContext.contentResolver
            val uri = resolver.insert(android.provider.MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
            if (uri != null) {
                resolver.openOutputStream(uri)?.use { os ->
                    out.inputStream().use { it.copyTo(os) }
                }
                val done = android.content.ContentValues().apply {
                    put(android.provider.MediaStore.MediaColumns.IS_PENDING, 0)
                }
                resolver.update(uri, done, null, null)
            }
        }
    }

    @Test
    fun approachHiddenWithoutPlannedRoute() {
        waitStyle()
        // Guidance active, but no corridor polyline — box must stay gone.
        NaviMapTestHooks.pendingApproachGuidance = ApproachGuidanceState(
            active = true,
            distanceM = 450.0,
            iconKey = "nav_right_1",
            nextStreet = "Nysethvegen",
            preferMetric = true,
        )
        Thread.sleep(1_000)
        composeRule.onAllNodesWithTag("approach_instruction_box").assertCountEquals(0)
    }

    @Test
    fun approachAppearAndUrgencyScreenshots() {
        waitStyle()
        injectPlannedRoute()

        // Approaching destination street Nysethvegen (end of planned route).
        NaviMapTestHooks.pendingApproachGuidance = ApproachGuidanceState(
            active = true,
            distanceM = 450.0,
            iconKey = "nav_right_1",
            nextStreet = "Ommangsgutua",
            houseNumber = "12",
            postcode = "2312",
            preferMetric = true,
        )
        Thread.sleep(800)
        composeRule.onNodeWithTag("approach_instruction_box").assertIsDisplayed()
        composeRule.onNodeWithTag("approach_distance").assertIsDisplayed()
        composeRule.onNodeWithTag("approach_street").assertIsDisplayed()
        composeRule.onNodeWithTag("approach_housenumber").assertIsDisplayed()
        composeRule.onNodeWithTag("approach_postcode").assertIsDisplayed()
        val streetNode = composeRule.onNodeWithTag("approach_street").fetchSemanticsNode()
        // Single-line street: height should stay near one text line, not two wrapped lines.
        assertTrue(
            "street name must stay on one line (height=${streetNode.size.height})",
            streetNode.size.height < 80,
        )
        assertEquals(ApproachUiPhase.Appear, NaviMapTestHooks.lastApproachPhase)
        val appearW = composeRule.onNodeWithTag("approach_instruction_box")
            .fetchSemanticsNode().size.width
        val screenW = InstrumentationRegistry.getInstrumentation()
            .targetContext.resources.displayMetrics.widthPixels.toFloat()
        assertTrue(
            "approach box must be compact (was ${appearW}px of ${screenW}px screen)",
            appearW < screenW * 0.45f,
        )
        shot("approach_appear_450m")

        NaviMapTestHooks.pendingApproachGuidance = ApproachGuidanceState(
            active = true,
            distanceM = 120.0,
            iconKey = "nav_right_1",
            nextStreet = "Ommangsgutua",
            houseNumber = "12",
            postcode = "2312",
            preferMetric = true,
        )
        Thread.sleep(800)
        composeRule.onNodeWithTag("approach_instruction_box").assertIsDisplayed()
        assertEquals(ApproachUiPhase.Urgency, NaviMapTestHooks.lastApproachPhase)
        shot("approach_urgency_120m")
    }
}
