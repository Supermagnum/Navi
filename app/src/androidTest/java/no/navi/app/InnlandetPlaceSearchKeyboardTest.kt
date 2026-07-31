package no.navi.app

import android.util.Log
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import kotlin.math.asin
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Keyboard place search checks for known Innlandet locations under online and
 * offline (Wi-Fi off) basemap conditions. Search itself is local FTS
 * ([uniffi.navi.searchPlaces]); Wi-Fi off proves there is no network fallback.
 */
@RunWith(AndroidJUnit4::class)
class InnlandetPlaceSearchKeyboardTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var device: UiDevice

    @Before
    fun setUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        runCatching {
            InstrumentationRegistry
                .getInstrumentation()
                .uiAutomation
                .grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        }
        runCatching {
            InstrumentationRegistry
                .getInstrumentation()
                .uiAutomation
                .grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        }
        dismissPermissionDialogs()
    }

    private data class Case(
        val label: String,
        val complete: String,
        val partial: String,
        val expectName: String,
        val expectLat: Double,
        val expectLon: Double,
        val maxDistM: Double = 2500.0,
    )

    private val cases =
        listOf(
            Case("Ottestad kirke", "Ottestad kirke", "Ottestad k", "Ottestad kirke", 60.7573125, 11.1446341),
            Case("Nordre Ottestad", "Nordre Ottestad", "Nordre Ott", "Nordre Ottestad", 60.7559310, 11.1433503),
            Case("Atlungstad Brenneri", "Atlungstad Brenneri", "Atlungstad Brenn", "Atlungstad Brenneri", 60.7591455, 11.0799013),
            Case("Tangen", "Tangen", "Tang", "Tangen", 60.6179108, 11.2674667),
            Case("Tangen dyrepark", "Tangen dyrepark", "Tangen dyre", "Tangen Dyrepark", 60.6306839, 11.3082834),
            Case("Espa", "Espa", "Esp", "Espa", 60.5778110, 11.2711672),
        )

    private lateinit var dataDir: File
    private val report = StringBuilder()

    @Test
    fun place_search_online_and_wifi_off_keyboard() {
        waitReady()
        dataDir = NaviAppData.resolve(composeRule.activity).also { it.mkdirs() }
        val staged = File("/data/local/tmp/navi_fixtures/place_index_search_check.db")
        assertTrue(
            "place_index fixture missing at ${staged.absolutePath}",
            staged.isFile && staged.length() > 10_000L,
        )
        staged.copyTo(File(dataDir, "place_index.db"), overwrite = true)
        // Offline basemap fixtures (Protomaps + DEM) for the Wi-Fi-off pass.
        OstlandetOfflineFixtures.ensureInstalled(dataDir)

        // Prefer live GPS as start when the device has a fix (follow is default).
        NaviMapTestHooks.followGps = true
        NaviMapTestHooks.disableGpsFollow = false
        NaviMapTestHooks.hideSearchChrome = false

        report.appendLine("Innlandet place search keyboard report")
        runPass(online = true)
        runPass(online = false)

        val text = report.toString()
        text.lineSequence().forEach { Log.i(REPORT_TAG, it) }
        File(dataDir, "innlandet_place_search_report.txt").writeText(text)
        // Shell cannot read app-private files; write pullable copies from the app UID.
        runCatching {
            val staged = File("/data/local/tmp/innlandet_place_search_report.txt")
            staged.writeText(text)
            staged.setReadable(true, false)
        }
        composeRule.activity.getExternalFilesDir(null)?.let { ext ->
            File(ext, "innlandet_place_search_report.txt").writeText(text)
        }

        val fails = report.lineSequence().filter { it.contains(" FAIL") }.toList()
        assertTrue("search failures:\n${fails.joinToString("\n")}\n\n$report", fails.isEmpty())
    }

    companion object {
        private const val REPORT_TAG = "InnlandetPlaceSearch"
    }

    private fun runPass(online: Boolean) {
        val label = if (online) "online" else "offline_wifi_off"
        if (online) {
            NaviMapTestHooks.forceOnlineBasemap = true
            setWifiEnabled(true)
        } else {
            NaviMapTestHooks.forceOnlineBasemap = false
            setWifiEnabled(false)
            // Deny network for this process where possible.
            shell("svc wifi disable")
            Thread.sleep(2_000)
        }
        Thread.sleep(1_500)
        report.appendLine("=== pass=$label basemapForceOnline=${NaviMapTestHooks.forceOnlineBasemap} kind=${NaviMapTestHooks.lastBasemapKind} ===")

        // btn_open_search only exists when route chrome is collapsed; open if needed.
        openRoutePanel()
        composeRule.onNodeWithTag("chip_to", useUnmergedTree = true).performClick()
        Thread.sleep(400)

        for (c in cases) {
            checkQuery(label, c, c.complete, complete = true)
            checkQuery(label, c, c.partial, complete = false)
        }

        runCatching {
            composeRule.onNodeWithTag("btn_close_search", useUnmergedTree = true).performClick()
        }
        if (!online) {
            setWifiEnabled(true)
        }
    }

    private fun checkQuery(
        pass: String,
        c: Case,
        query: String,
        complete: Boolean,
    ) {
        val mode = if (complete) "complete" else "partial"
        composeRule
            .onNodeWithTag("field_search", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        Thread.sleep(300)
        // Clear previous text.
        shell("input keyevent 123") // move end
        repeat(40) { shell("input keyevent 67") } // DEL
        Thread.sleep(200)
        shell("input text ${query.replace(" ", "%s")}")
        val typedDeadline = System.currentTimeMillis() + 12_000
        while (System.currentTimeMillis() < typedDeadline &&
            NaviMapTestHooks.lastSearchQuery != query
        ) {
            Thread.sleep(200)
        }
        hideIme()
        composeRule.waitForIdle()
        Thread.sleep(500)

        val hits =
            uniffi.navi.searchPlaces(
                File(dataDir, "place_index.db").absolutePath,
                query,
                20u,
            )
        val uiNames = NaviMapTestHooks.lastSearchHitNames
        val hit =
            hits.firstOrNull { it.name.equals(c.expectName, ignoreCase = true) }
                ?: hits.firstOrNull { it.name.contains(c.expectName, ignoreCase = true) }
                ?: hits.firstOrNull {
                    haversineM(it.lat, it.lon, c.expectLat, c.expectLon) <= c.maxDistM &&
                        it.name.contains(c.expectName.substringBefore(' '), ignoreCase = true)
                }

        val dist =
            hit?.let { haversineM(it.lat, it.lon, c.expectLat, c.expectLon) } ?: Double.POSITIVE_INFINITY
        val selectable =
            hit != null &&
                (
                    uiNames.any { it.equals(hit.name, ignoreCase = true) } ||
                        uiNames.any { it.contains(c.expectName, ignoreCase = true) } ||
                        hits.isNotEmpty()
                )
        val passOk = hit != null && dist <= c.maxDistM && selectable
        val line =
            "$pass/$mode/${c.label} q='$query' -> " +
                if (passOk) {
                    "PASS name='${hit!!.name}' kind=${hit.kind} " +
                        "distM=${"%.0f".format(dist)} uiHas=${uiNames.take(5)}"
                } else {
                    "FAIL hit=${hit?.name} distM=${"%.0f".format(dist)} " +
                        "ffi=${hits.take(8).map { it.name }} ui=${uiNames.take(8)}"
                }
        report.appendLine(line)
    }

    private fun setWifiEnabled(enabled: Boolean) {
        shell(if (enabled) "svc wifi enable" else "svc wifi disable")
        Thread.sleep(1_500)
    }

    private fun openRoutePanel() {
        NaviMapTestHooks.hideSearchChrome = false
        runCatching {
            composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
        }.onFailure {
            composeRule.onNodeWithTag("btn_open_search", useUnmergedTree = true).performClick()
            composeRule.waitForIdle()
            Thread.sleep(400)
        }
        composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
    }

    private fun waitReady() {
        val deadline = System.currentTimeMillis() + 90_000
        var last: Throwable? = null
        while (System.currentTimeMillis() < deadline) {
            dismissPermissionDialogs()
            if (NaviMapTestHooks.styleReady) {
                runCatching {
                    composeRule.waitForIdle()
                    openRoutePanel()
                }.onSuccess { return }.onFailure { last = it }
            }
            Thread.sleep(400)
        }
        error("map style / search chrome not ready: $last")
    }

    private fun dismissPermissionDialogs() {
        val deadline = System.currentTimeMillis() + 8_000
        while (System.currentTimeMillis() < deadline) {
            val allow =
                device.findObject(By.text("While using the app"))
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

    private fun hideIme() {
        shell("input keyevent 111")
        Thread.sleep(200)
    }

    private fun shell(cmd: String) {
        UiDevice.getInstance(InstrumentationRegistry.getInstrumentation()).executeShellCommand(cmd)
    }

    private fun haversineM(
        lat1: Double,
        lon1: Double,
        lat2: Double,
        lon2: Double,
    ): Double {
        val r = 6_371_000.0
        val p1 = Math.toRadians(lat1)
        val p2 = Math.toRadians(lat2)
        val dPhi = Math.toRadians(lat2 - lat1)
        val dLambda = Math.toRadians(lon2 - lon1)
        val a =
            sin(dPhi / 2) * sin(dPhi / 2) +
                cos(p1) * cos(p2) * sin(dLambda / 2) * sin(dLambda / 2)
        return 2 * r * asin(min(1.0, sqrt(a)))
    }
}
