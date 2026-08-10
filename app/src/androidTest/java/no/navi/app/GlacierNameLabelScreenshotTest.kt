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
import kotlin.math.abs

/**
 * Investigation only: glacier name labels across Liberty + offline Protomaps
 * at Hellstugubrean (named glacier), zoom ladder z8–z14.
 */
@RunWith(AndroidJUnit4::class)
class GlacierNameLabelScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        shell("mkdir -p /data/local/tmp/navi_glacier_names && chmod 777 /data/local/tmp/navi_glacier_names")
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.requestOptIn3d = false
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
        setNetworkEnabled(true)
        runCatching { activityRule.finishActivity() }
    }

    @Test
    fun captureHellstugubreanNameLadder() {
        OstlandetOfflineFixtures.ensureInstalled(NaviAppData.resolve(context))

        setNetworkEnabled(true)
        for (z in listOf(8.0, 10.0, 12.0, 14.0)) {
            shoot("liberty_hellstugu_z${z.toInt()}", LAT, LON, z, forceOnline = true)
        }

        activityRule.finishActivity()
        Thread.sleep(800)
        setNetworkEnabled(false)
        for (z in listOf(8.0, 10.0, 12.0, 14.0)) {
            shoot("offline_pm_hellstugu_z${z.toInt()}", LAT, LON, z, forceOnline = false)
        }
        setNetworkEnabled(true)

        val n =
            java.io
                .File("/data/local/tmp/navi_glacier_names")
                .listFiles()
                ?.count { it.extension == "png" } ?: 0
        assertTrue("expected glacier name ladder shots, got $n", n >= 8)
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
        NaviMapTestHooks.requestOptIn3d = false
        NaviMapTestHooks.requestCameraTiltDeg = 0.0
        MapHudPrefs.saveOptIn3d(context, false)
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        activityRule.launchActivity(null)

        val deadline = System.currentTimeMillis() + 90_000
        while (System.currentTimeMillis() < deadline) {
            val kind = NaviMapTestHooks.lastBasemapKind
            val kindOk =
                if (forceOnline) kind.startsWith("Online") else kind == "OfflineProtomaps"
            if (NaviMapTestHooks.styleReady && kindOk) break
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(400)
        }
        assertTrue("styleReady $label kind=${NaviMapTestHooks.lastBasemapKind}", NaviMapTestHooks.styleReady)
        if (!forceOnline) {
            assertTrue(NaviMapTestHooks.lastBasemapKind == "OfflineProtomaps")
        }
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        val camDeadline = System.currentTimeMillis() + 20_000
        while (System.currentTimeMillis() < camDeadline) {
            if (abs(NaviMapTestHooks.lastCameraZoom - zoom) < 0.15 &&
                abs(NaviMapTestHooks.lastCameraLat - lat) < 0.08
            ) {
                break
            }
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(250)
        }
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(30_000))
        Thread.sleep(2_000)
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(20_000))
        val path = "/data/local/tmp/navi_glacier_names/$label.png"
        InstrumentedMapCapture.screencapAfterSettle(path, timeoutMs = 8_000)
        android.util.Log.i(TAG, "NAME_SHOT $label kind=${NaviMapTestHooks.lastBasemapKind} z=${NaviMapTestHooks.lastCameraZoom}")
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

    companion object {
        private const val TAG = "GlacierNameLabels"

        // Hellstugubrean (named) — way/relation near Gjende; not unnamed way 380644665
        private const val LAT = 61.5622319
        private const val LON = 8.4390741
    }
}
