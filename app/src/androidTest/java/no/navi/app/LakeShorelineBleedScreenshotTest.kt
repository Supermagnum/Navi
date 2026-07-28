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

/**
 * Evidence-only lake shoreline screenshots across style / 3D / zoom (no fix).
 * Target: Mjøsa near Hamar.
 */
@RunWith(AndroidJUnit4::class)
class LakeShorelineBleedScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context
    private lateinit var dataDir: File
    private lateinit var outDir: File

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        outDir = File(context.cacheDir, "navi_lake_bleed").also {
            it.mkdirs()
            it.listFiles()?.forEach { f -> f.delete() }
        }
        // Host-pullable mirror created via shell after each shot.
        shell("mkdir -p /data/local/tmp/navi_lake_bleed && chmod 777 /data/local/tmp/navi_lake_bleed")
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.forceOnlineBasemap = false
        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
    }

    @After
    fun tearDown() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.forceOnlineBasemap = false
        MapHudPrefs.saveOptIn3d(context, false)
        setNetworkEnabled(true)
        runCatching { activityRule.finishActivity() }
    }

    @Test
    fun capture_lake_bleed_matrix() {
        OstlandetOfflineFixtures.ensureInstalled(dataDir)

        setNetworkEnabled(true)
        MapHudPrefs.saveOptIn3d(context, false)
        shoot("online_liberty_3d_off_z10", LAKE_LAT, LAKE_LON, 10.0, forceOnline = true, wantTerrain = false)
        shoot("online_liberty_3d_off_z13", LAKE_LAT, LAKE_LON, 13.0, forceOnline = true, wantTerrain = false)
        shoot(
            "online_liberty_3d_off_z13_nudge",
            LAKE_LAT + 0.02,
            LAKE_LON + 0.03,
            13.0,
            forceOnline = true,
            wantTerrain = false,
        )

        MapHudPrefs.saveOptIn3d(context, true)
        shoot("online_liberty_3d_on_z11", LAKE_LAT, LAKE_LON, 11.0, forceOnline = true, wantTerrain = true)

        activityRule.finishActivity()
        Thread.sleep(800)
        setNetworkEnabled(false)
        MapHudPrefs.saveOptIn3d(context, false)
        shoot("offline_protomaps_3d_off_z10", LAKE_LAT, LAKE_LON, 10.0, forceOnline = false, wantTerrain = false)
        shoot("offline_protomaps_3d_off_z13", LAKE_LAT, LAKE_LON, 13.0, forceOnline = false, wantTerrain = false)
        shoot(
            "offline_protomaps_3d_off_z13_nudge",
            LAKE_LAT + 0.02,
            LAKE_LON + 0.03,
            13.0,
            forceOnline = false,
            wantTerrain = false,
        )

        MapHudPrefs.saveOptIn3d(context, true)
        shoot("offline_protomaps_3d_on_z11", LAKE_LAT, LAKE_LON, 11.0, forceOnline = false, wantTerrain = true)

        setNetworkEnabled(true)
        MapHudPrefs.saveOptIn3d(context, false)
        val n = outDir.listFiles()?.count { it.extension == "png" } ?: 0
        assertTrue("expected lake shots in $outDir, got $n", n >= 4)
    }

    private fun shoot(
        label: String,
        lat: Double,
        lon: Double,
        zoom: Double,
        forceOnline: Boolean,
        wantTerrain: Boolean,
    ) {
        runCatching { activityRule.finishActivity() }
        Thread.sleep(500)
        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.lastBasemapKind = ""
        NaviMapTestHooks.lastTerrainAttached = false
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
            val terrainOk = !wantTerrain || NaviMapTestHooks.lastTerrainAttached
            if (NaviMapTestHooks.styleReady && kindOk && terrainOk) break
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(400)
        }
        Thread.sleep(3_500)

        val png = File(outDir, "$label.png")
        val req = NaviMapTestHooks.snapshotRequestId + 1
        NaviMapTestHooks.lastSnapshotPng = null
        NaviMapTestHooks.snapshotRequestId = req
        val snapDeadline = System.currentTimeMillis() + 25_000
        while (System.currentTimeMillis() < snapDeadline) {
            val bytes = NaviMapTestHooks.lastSnapshotPng
            if (NaviMapTestHooks.lastSnapshotId >= req && bytes != null && bytes.size > 1_000) {
                png.writeBytes(bytes)
                break
            }
            Thread.sleep(250)
        }
        if (!png.isFile || png.length() < 1_000) {
            shell("screencap -p ${png.absolutePath}")
        }
        assertTrue(
            "missing $label kind=${NaviMapTestHooks.lastBasemapKind} " +
                "terrain=${NaviMapTestHooks.lastTerrainAttached}",
            png.isFile && png.length() > 1_000,
        )
        // Shell-owned path for host adb pull (app uid cannot write there directly).
        shell("screencap -p /data/local/tmp/navi_lake_bleed/$label.png")
        shell("chmod 644 /data/local/tmp/navi_lake_bleed/$label.png")
        android.util.Log.i(
            TAG,
            "LAKE_SHOT label=$label kind=${NaviMapTestHooks.lastBasemapKind} " +
                "terrain=${NaviMapTestHooks.lastTerrainAttached} zoom=$zoom " +
                "path=/data/local/tmp/navi_lake_bleed/$label.png",
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
        private const val TAG = "NaviLakeBleed"
        private const val LAKE_LAT = 60.7945
        private const val LAKE_LON = 11.0680
    }
}
