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
 * Evidence: Protomaps landuse military (muted red #c96a5a) + glacier at hiking zooms;
 * Liberty military left as upstream omission; Liberty glacier via landcover_ice.
 */
@RunWith(AndroidJUnit4::class)
class MilitaryGlacierLanduseScreenshotTest {
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
        shell("mkdir -p /data/local/tmp/navi_military_glacier && chmod 777 /data/local/tmp/navi_military_glacier")
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
        NaviMapTestHooks.requestOptIn3d = false
        MapHudPrefs.saveOptIn3d(context, false)
        setNetworkEnabled(true)
        runCatching { activityRule.finishActivity() }
    }

    @Test
    fun captureRenaMilitaryAndGjendeGlacier() {
        OstlandetOfflineFixtures.ensureInstalled(dataDir)

        // Liberty: military (expected: no special fill — upstream omission) + glacier ice.
        setNetworkEnabled(true)
        shoot("liberty_military_rena_z13", RENA_LAT, RENA_LON, 13.0, forceOnline = true, want3d = false)
        shoot("liberty_glacier_gjende_z11", GJENDE_LAT, GJENDE_LON, 11.0, forceOnline = true, want3d = false)

        // Offline Protomaps: military muted red + glacier at hiking zooms (landuse path).
        activityRule.finishActivity()
        Thread.sleep(800)
        setNetworkEnabled(false)
        for (z in listOf(8.0, 10.0, 12.0)) {
            shoot(
                "offline_pm_military_rena_z${z.toInt()}",
                RENA_LAT,
                RENA_LON,
                z,
                forceOnline = false,
                want3d = false,
            )
        }
        for (z in listOf(8.0, 10.0, 12.0)) {
            shoot(
                "offline_pm_glacier_gjende_z${z.toInt()}",
                GJENDE_LAT,
                GJENDE_LON,
                z,
                forceOnline = false,
                want3d = false,
            )
        }
        shoot(
            "offline_pm_glacier_gjende_z12_3d",
            GJENDE_LAT,
            GJENDE_LON,
            12.0,
            forceOnline = false,
            want3d = true,
        )
        setNetworkEnabled(true)

        val n =
            File("/data/local/tmp/navi_military_glacier")
                .listFiles()
                ?.count { it.extension == "png" } ?: 0
        assertTrue("expected military/glacier shots, got $n", n >= 9)
    }

    private fun shoot(
        label: String,
        lat: Double,
        lon: Double,
        zoom: Double,
        forceOnline: Boolean,
        want3d: Boolean,
    ) {
        runCatching { activityRule.finishActivity() }
        Thread.sleep(500)
        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.lastBasemapKind = ""
        NaviMapTestHooks.lastTerrainAttached = false
        NaviMapTestHooks.forceOnlineBasemap = forceOnline
        NaviMapTestHooks.requestOptIn3d = want3d
        NaviMapTestHooks.requestCameraTiltDeg = if (want3d) 45.0 else 0.0
        MapHudPrefs.saveOptIn3d(context, want3d)
        MapHudPrefs.saveCameraTiltDeg(context, if (want3d) 45.0 else 0.0)
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
            val terrainOk = !want3d || NaviMapTestHooks.lastTerrainAttached
            if (NaviMapTestHooks.styleReady && kindOk && terrainOk) break
            NaviMapTestHooks.requestOptIn3d = want3d
            NaviMapTestHooks.requestCameraTiltDeg = if (want3d) 45.0 else 0.0
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(400)
        }
        assertTrue(
            "styleReady label=$label kind=${NaviMapTestHooks.lastBasemapKind} " +
                "terrain=${NaviMapTestHooks.lastTerrainAttached}",
            NaviMapTestHooks.styleReady,
        )
        if (!forceOnline) {
            assertTrue(
                "expected OfflineProtomaps got ${NaviMapTestHooks.lastBasemapKind}",
                NaviMapTestHooks.lastBasemapKind == "OfflineProtomaps",
            )
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

        val path = "/data/local/tmp/navi_military_glacier/$label.png"
        InstrumentedMapCapture.screencapAfterSettle(path, timeoutMs = 8_000)
        android.util.Log.i(
            TAG,
            "MG_SHOT label=$label kind=${NaviMapTestHooks.lastBasemapKind} " +
                "terrain=${NaviMapTestHooks.lastTerrainAttached} " +
                "pitch=${NaviMapTestHooks.lastCameraPitch} " +
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

    companion object {
        private const val TAG = "MilitaryGlacierLU"

        // Rena leir OSM way 962221904
        private const val RENA_LAT = 61.153425
        private const val RENA_LON = 11.3961692

        // Glacier near Gjende OSM way 380644665
        private const val GJENDE_LAT = 61.5207662
        private const val GJENDE_LON = 8.4060213
    }
}
