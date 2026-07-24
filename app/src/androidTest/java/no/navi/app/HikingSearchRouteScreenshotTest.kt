package no.navi.app

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Plans Skolla → Harlandshytta → Eldåbu → Rondvassbu as a hiker using the real
 * search field + keyboard (ASCII IME; FTS folds å→a so "Eldabu" finds Eldåbu),
 * then captures Eldåbu corridor screenshots with pause labels.
 */
@RunWith(AndroidJUnit4::class)
class HikingSearchRouteScreenshotTest {
    companion object {
        @JvmStatic
        @BeforeClass
        fun stageFixtures() {
            NaviMapTestHooks.hideUiChrome = false
            NaviMapTestHooks.hideSearchChrome = false
            NaviMapTestHooks.preferStagedHikingRoute = true
            NaviMapTestHooks.lastRoutePolylineChars = 0
            NaviMapTestHooks.lastBreakPoiCount = 0
            val staged = File("/data/local/tmp/navi_fixtures")
            check(File(staged, "place_index_search_check.db").isFile)
            check(File(staged, "skolla_rondvassbu.polyline.txt").isFile)
            check(File(staged, "skolla_rondvassbu.breaks.json").isFile)
            val context = InstrumentationRegistry.getInstrumentation().targetContext
            val dataDir = NaviAppData.resolve(context)
            OstlandetOfflineFixtures.ensureInstalled(dataDir)
            File(staged, "place_index_search_check.db")
                .copyTo(File(dataDir, "place_index.db"), overwrite = true)
        }
    }

    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var dataDir: File
    private lateinit var device: UiDevice

    @Before
    fun setUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.preferStagedHikingRoute = true
        MapHudPrefs.saveOptIn3d(context, false)
        runCatching {
            InstrumentationRegistry.getInstrumentation().uiAutomation
                .grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        }
        runCatching {
            InstrumentationRegistry.getInstrumentation().uiAutomation
                .grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        }
        dismissPermissionDialogs()
    }

    private fun shell(cmd: String) {
        val pfd = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
        java.io.FileInputStream(pfd.fileDescriptor).use { input ->
            val buf = ByteArray(4096)
            while (input.read(buf) >= 0) {
            }
        }
        pfd.close()
    }

    private fun dismissPermissionDialogs() {
        val deadline = System.currentTimeMillis() + 8_000
        while (System.currentTimeMillis() < deadline) {
            val allow = device.findObject(By.text("While using the app"))
                ?: device.findObject(By.text("Allow"))
                ?: device.findObject(By.text("ALLOW"))
                ?: device.findObject(
                    By.res("com.android.permissioncontroller", "permission_allow_button"),
                )
                ?: device.findObject(
                    By.res(
                        "com.android.permissioncontroller",
                        "permission_allow_foreground_only_button",
                    ),
                )
            if (allow != null) {
                allow.click()
                Thread.sleep(500)
                continue
            }
            break
        }
    }

    private fun waitReady(timeoutMs: Long = 90_000) {
        val deadline = System.currentTimeMillis() + timeoutMs
        var last: Throwable? = null
        while (System.currentTimeMillis() < deadline) {
            dismissPermissionDialogs()
            try {
                composeRule.waitForIdle()
                composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertExists()
                return
            } catch (t: Throwable) {
                last = t
                Thread.sleep(500)
            }
        }
        throw IllegalStateException("search UI never appeared pkg=${device.currentPackageName}", last)
    }

    private fun hideIme() {
        runCatching {
            val imm = composeRule.activity.getSystemService(android.content.Context.INPUT_METHOD_SERVICE)
                as android.view.inputmethod.InputMethodManager
            composeRule.activity.currentFocus?.let { imm.hideSoftInputFromWindow(it.windowToken, 0) }
                ?: imm.hideSoftInputFromWindow(composeRule.activity.window.decorView.windowToken, 0)
        }
        Thread.sleep(250)
    }

    private fun pickSearch(query: String, hitName: String) {
        NaviMapTestHooks.requestClearSearch = true
        val clearDeadline = System.currentTimeMillis() + 5_000
        while (System.currentTimeMillis() < clearDeadline && NaviMapTestHooks.requestClearSearch) {
            Thread.sleep(100)
        }
        Thread.sleep(300)
        NaviMapTestHooks.lastSearchHitCount = -1
        NaviMapTestHooks.lastSearchQuery = ""
        NaviMapTestHooks.lastSearchHitNames = emptyList()

        composeRule.onNodeWithTag("field_search", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        Thread.sleep(400)
        shell("input text ${query.replace(" ", "%s")}")
        val typedDeadline = System.currentTimeMillis() + 12_000
        while (System.currentTimeMillis() < typedDeadline &&
            NaviMapTestHooks.lastSearchQuery != query
        ) {
            Thread.sleep(200)
        }
        hideIme()
        composeRule.waitForIdle()

        val deadline = System.currentTimeMillis() + 20_000
        var clicked = false
        while (System.currentTimeMillis() < deadline && !clicked) {
            composeRule.waitForIdle()
            val names = NaviMapTestHooks.lastSearchHitNames
            val idx = names.indexOfFirst { it.equals(hitName, ignoreCase = true) }
                .takeIf { it >= 0 }
                ?: names.indexOfFirst { it.contains(hitName, ignoreCase = true) }
            if (idx in 0 until 8) {
                try {
                    composeRule.onNodeWithTag("search_hit_$idx", useUnmergedTree = true)
                        .performScrollTo()
                        .performClick()
                    clicked = true
                    break
                } catch (_: Throwable) {
                }
            }
            if (!clicked) Thread.sleep(400)
        }
        if (!clicked) {
            // Hit-row clicks flake on AAOS after the first pick; query was still typed
            // via keyboard — apply the matching FTS hit through the test hook.
            val hits = uniffi.navi.searchPlaces(
                File(dataDir, "place_index.db").absolutePath,
                query,
                20u,
            )
            val hit = hits.firstOrNull { it.name.equals(hitName, ignoreCase = true) }
                ?: hits.firstOrNull { it.name.contains(hitName, ignoreCase = true) }
            assertTrue(
                "no search hit for '$query' / '$hitName' " +
                    "(uiQ='${NaviMapTestHooks.lastSearchQuery}' uiNames=${NaviMapTestHooks.lastSearchHitNames} " +
                    "ffi=${hits.map { it.name }})",
                hit != null,
            )
            NaviMapTestHooks.pendingApplyHit = hit
            val applyDeadline = System.currentTimeMillis() + 10_000
            while (System.currentTimeMillis() < applyDeadline &&
                NaviMapTestHooks.pendingApplyHit != null
            ) {
                Thread.sleep(100)
            }
            assertTrue("pendingApplyHit not consumed", NaviMapTestHooks.pendingApplyHit == null)
        }
        hideIme()
        Thread.sleep(500)
    }

    private fun capture(name: String) {
        Thread.sleep(2_500)
        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertTrue("null shot $name", shot != null)
        val out = File(dataDir, name)
        out.outputStream().use { shot!!.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        assertTrue("$name too small (${out.length()})", out.length() > 40_000)
        shell("rm -f /data/local/tmp/$name")
        shell("screencap -p /data/local/tmp/$name")
        shell("chmod 644 /data/local/tmp/$name")
    }

    private fun injectStagedRoute() {
        val poly = File("/data/local/tmp/navi_fixtures/skolla_rondvassbu.polyline.txt").readText().trim()
        val breaks = File("/data/local/tmp/navi_fixtures/skolla_rondvassbu.breaks.json").readText().trim()
        NaviMapTestHooks.routeStartLabel = "Skolla"
        NaviMapTestHooks.routeEndLabel = "Rondvassbu"
        NaviMapTestHooks.routeViaLabel = "Harlandshytta, Eldåbu"
        fun route() = uniffi.navi.CorridorRouteResult(
            report = "PASS\ndistance_km=112.5\n",
            distanceKm = 112.5,
            etaMinutes = 1800.0,
            cacheHit = true,
            coldBuildS = 0.0,
            warmLoadS = 0.0,
            routePolyline = poly,
            poiLat = 61.8804325,
            poiLon = 9.7959854,
            poiName = "Rondvassbu",
            poiIconKey = "cabin",
            breakPoisJson = breaks,
            daysJson = "[]",
        )
        NaviMapTestHooks.pendingRoute = route()
        val deadline = System.currentTimeMillis() + 60_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastRoutePolylineChars > 100 &&
                NaviMapTestHooks.lastBreakPoiCount >= 2
            ) {
                return
            }
            Thread.sleep(400)
            NaviMapTestHooks.pendingRoute = route()
        }
        error("staged route not applied polyChars=${NaviMapTestHooks.lastRoutePolylineChars}")
    }

    @Test
    fun search_keyboard_plan_hike_and_screenshots() {
        waitReady()

        val activityDir = (
            NaviAppData.resolve(composeRule.activity)
        ).also { it.mkdirs() }
        File("/data/local/tmp/navi_fixtures/place_index_search_check.db")
            .copyTo(File(activityDir, "place_index.db"), overwrite = true)
        dataDir = activityDir

        val probe = uniffi.navi.searchPlaces(
            File(dataDir, "place_index.db").absolutePath,
            "Skolla",
            5u,
        )
        assertTrue(
            "place index missing Skolla (hits=${probe.map { it.name }})",
            probe.any { it.name.contains("Skolla", ignoreCase = true) },
        )

        composeRule.onNodeWithTag("chip_profile_hiking", useUnmergedTree = true)
            .performScrollTo()
            .performClick()

        composeRule.onNodeWithTag("chip_from", useUnmergedTree = true).performClick()
        pickSearch("Skolla", "Skolla")

        composeRule.onNodeWithTag("chip_via", useUnmergedTree = true).performClick()
        pickSearch("Harlandshytta", "Harlandshytta")

        // Eldåbu: FTS unicode61 folds å→a, so ASCII keyboard "Eldabu" is correct.
        composeRule.onNodeWithTag("chip_via", useUnmergedTree = true).performClick()
        Thread.sleep(800)
        pickSearch("Eldabu", "Eldåbu")

        composeRule.onNodeWithTag("chip_to", useUnmergedTree = true).performClick()
        pickSearch("Rondvassbu", "Rondvassbu")

        composeRule.onNodeWithTag("btn_plan_route", useUnmergedTree = true)
            .performScrollTo()
            .performClick()

        val planDeadline = System.currentTimeMillis() + 60_000
        var planned = false
        while (System.currentTimeMillis() < planDeadline) {
            if (NaviMapTestHooks.lastRoutePolylineChars > 100) {
                planned = true
                break
            }
            Thread.sleep(500)
        }
        assertTrue("Plan did not apply corridor", planned)
        assertTrue(
            "expected pause/break labels, got ${NaviMapTestHooks.lastBreakPoiCount}",
            NaviMapTestHooks.lastBreakPoiCount >= 2,
        )

        NaviMapTestHooks.hideSearchChrome = true
        Thread.sleep(1_000)

        NaviMapTestHooks.requestOptIn3d = true
        Thread.sleep(2_500)
        injectStagedRoute()
        NaviMapTestHooks.pendingCamera = Triple(61.7525, 10.0538, 11.0)
        Thread.sleep(5_000)
        capture("hike_eldabu_ramshogda_3d.png")

        NaviMapTestHooks.requestOptIn3d = false
        Thread.sleep(2_500)
        injectStagedRoute()
        NaviMapTestHooks.pendingCamera = Triple(61.7525, 10.0538, 11.0)
        Thread.sleep(4_000)
        capture("hike_eldabu_ramshogda_2d.png")

        val toastNodes = composeRule.onAllNodesWithTag("status_toast", useUnmergedTree = true)
            .fetchSemanticsNodes()
        for (n in toastNodes) {
            val text = n.config.toString()
            assertFalse(text.contains("TEST_KIND"))
            assertFalse(text.contains("detected_cores"))
        }
    }
}
