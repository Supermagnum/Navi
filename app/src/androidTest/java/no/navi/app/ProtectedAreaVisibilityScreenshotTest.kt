package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import kotlin.math.abs

/**
 * Evidence: protected-area boundaries and names after the Liberty + Protomaps
 * style fixes. Rondane (primary) + Jotunheimen (generalization), z7/9/11/13.
 */
@RunWith(AndroidJUnit4::class)
class ProtectedAreaVisibilityScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context
    private lateinit var dataDir: File

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        shell("mkdir -p /data/local/tmp/navi_protected_area && chmod 777 /data/local/tmp/navi_protected_area")
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.styleReady = false
        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
    }

    @After
    fun tearDown() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.disableGpsFollow = false
        NaviMapTestHooks.followGps = true
        MapHudPrefs.saveOptIn3d(context, false)
        setNetworkEnabled(true)
        runCatching { activityRule.finishActivity() }
    }

    @Test
    fun capture_rondane_and_jotunheimen_zoom_ladder() {
        OstlandetOfflineFixtures.ensureInstalled(dataDir)

        setNetworkEnabled(true)
        for (site in SITES) {
            for (z in ZOOMS) {
                shoot(
                    "liberty_${site.label}_z${z.toInt()}",
                    site.lat,
                    site.lon,
                    z,
                    forceOnline = true,
                )
            }
        }

        activityRule.finishActivity()
        Thread.sleep(800)
        setNetworkEnabled(false)
        for (site in SITES) {
            for (z in ZOOMS) {
                shoot(
                    "offline_pm_${site.label}_z${z.toInt()}",
                    site.lat,
                    site.lon,
                    z,
                    forceOnline = false,
                )
            }
        }
        setNetworkEnabled(true)

        val n =
            File("/data/local/tmp/navi_protected_area")
                .listFiles()
                ?.count { it.extension == "png" } ?: 0
        assertTrue("expected protected-area shots, got $n", n >= SITES.size * ZOOMS.size * 2)
    }

    private fun shoot(
        label: String,
        lat: Double,
        lon: Double,
        zoom: Double,
        forceOnline: Boolean,
    ) {
        runCatching { activityRule.finishActivity() }
        Thread.sleep(500)
        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.lastBasemapKind = ""
        NaviMapTestHooks.forceOnlineBasemap = forceOnline
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        activityRule.launchActivity(null)

        val deadline = System.currentTimeMillis() + 90_000
        while (System.currentTimeMillis() < deadline) {
            val kind = NaviMapTestHooks.lastBasemapKind
            val kindOk =
                if (forceOnline) {
                    kind.startsWith("Online")
                } else {
                    kind == "OfflineProtomaps"
                }
            if (NaviMapTestHooks.styleReady && kindOk) break
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(400)
        }
        assertTrue(
            "styleReady label=$label kind=${NaviMapTestHooks.lastBasemapKind}",
            NaviMapTestHooks.styleReady,
        )
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        val camDeadline = System.currentTimeMillis() + 20_000
        while (System.currentTimeMillis() < camDeadline) {
            if (abs(NaviMapTestHooks.lastCameraZoom - zoom) < 0.1) break
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(250)
        }
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(30_000))
        Thread.sleep(2_000)
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(20_000))

        val path = "/data/local/tmp/navi_protected_area/$label.png"
        InstrumentedMapCapture.screencapAfterSettle(path, timeoutMs = 8_000)
        android.util.Log.i(
            TAG,
            "PA_SHOT label=$label kind=${NaviMapTestHooks.lastBasemapKind} " +
                "zoom=${NaviMapTestHooks.lastCameraZoom} path=$path",
        )
    }

    private fun setNetworkEnabled(enabled: Boolean) {
        shell(if (enabled) "svc wifi enable" else "svc wifi disable")
        shell(if (enabled) "svc data enable" else "svc data disable")
        Thread.sleep(1_200)
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

    private data class Site(
        val label: String,
        val lat: Double,
        val lon: Double,
    )

    companion object {
        private const val TAG = "ProtectedAreaVis"
        private val ZOOMS = listOf(7.0, 9.0, 11.0, 13.0)

        // Rondane NP interior; Jotunheimen NP (Ostlandet covers both).
        private val SITES =
            listOf(
                Site("rondane", 61.93, 9.72),
                Site("jotunheimen", 61.58, 8.50),
            )
    }
}
