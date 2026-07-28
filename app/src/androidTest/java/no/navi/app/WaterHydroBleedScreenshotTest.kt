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
 * Evidence-only: lake fill vs river/creek line bleed across style / 3D / zoom.
 * No style fix — isolate shared vs separate root causes.
 *
 * Targets (Østlandet):
 * - Lake: Mjøsa / Hamar shoreline
 * - River: Glomma near Elverum (clear LineString waterway, not lake fill)
 * - Creek-scale: Flagstadelva corridor NE of Hamar at higher zoom
 */
@RunWith(AndroidJUnit4::class)
class WaterHydroBleedScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context
    private lateinit var dataDir: File
    private lateinit var cacheOut: File

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        runCatching {
            auto.grantRuntimePermission(
                context.packageName,
                android.Manifest.permission.ACCESS_FINE_LOCATION,
            )
        }
        runCatching {
            auto.grantRuntimePermission(
                context.packageName,
                android.Manifest.permission.ACCESS_COARSE_LOCATION,
            )
        }
        cacheOut =
            File(context.cacheDir, "navi_hydro_bleed").also {
                it.mkdirs()
                it.listFiles()?.forEach { f -> f.delete() }
            }
        shell("mkdir -p /data/local/tmp/navi_hydro_bleed && chmod 777 /data/local/tmp/navi_hydro_bleed")
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
    fun capture_3d_after_hills_below_water() {
        // Isolation check for hillshade reorder only (3D lake/river).
        OstlandetOfflineFixtures.ensureInstalled(dataDir)
        setNetworkEnabled(true)
        MapHudPrefs.saveOptIn3d(context, true)
        shoot("liberty_3d_lake_z11", LAKE_LAT, LAKE_LON, 11.0, forceOnline = true, wantTerrain = true)
        shoot("liberty_3d_river_z14", RIVER_LAT, RIVER_LON, 14.0, forceOnline = true, wantTerrain = true)

        activityRule.finishActivity()
        Thread.sleep(800)
        setNetworkEnabled(false)
        MapHudPrefs.saveOptIn3d(context, true)
        shoot("pm_3d_lake_z11", LAKE_LAT, LAKE_LON, 11.0, forceOnline = false, wantTerrain = true)
        shoot("pm_3d_river_z14", RIVER_LAT, RIVER_LON, 14.0, forceOnline = false, wantTerrain = true)

        setNetworkEnabled(true)
        MapHudPrefs.saveOptIn3d(context, false)
        assertTrue(
            "missing liberty_3d_lake_z11",
            shellFileOk("/data/local/tmp/navi_hydro_bleed/liberty_3d_lake_z11.png"),
        )
        assertTrue(
            "missing pm_3d_river_z14",
            shellFileOk("/data/local/tmp/navi_hydro_bleed/pm_3d_river_z14.png"),
        )
    }

    @Test
    fun capture_lake_vs_river_matrix() {
        OstlandetOfflineFixtures.ensureInstalled(dataDir)

        // --- Online Liberty, 3D off ---
        setNetworkEnabled(true)
        MapHudPrefs.saveOptIn3d(context, false)
        shoot("liberty_2d_lake_z10", LAKE_LAT, LAKE_LON, 10.0, forceOnline = true, wantTerrain = false)
        shoot("liberty_2d_lake_z13", LAKE_LAT, LAKE_LON, 13.0, forceOnline = true, wantTerrain = false)
        shoot("liberty_2d_river_z12", RIVER_LAT, RIVER_LON, 12.0, forceOnline = true, wantTerrain = false)
        shoot("liberty_2d_river_z14", RIVER_LAT, RIVER_LON, 14.0, forceOnline = true, wantTerrain = false)
        shoot("liberty_2d_river_z16", RIVER_LAT, RIVER_LON, 16.0, forceOnline = true, wantTerrain = false)
        shoot("liberty_2d_creek_z15", CREEK_LAT, CREEK_LON, 15.0, forceOnline = true, wantTerrain = false)
        // Tile-boundary nudge along Glomma
        shoot(
            "liberty_2d_river_z14_nudge",
            RIVER_LAT + 0.015,
            RIVER_LON + 0.02,
            14.0,
            forceOnline = true,
            wantTerrain = false,
        )

        // --- Online Liberty + hillshade ---
        MapHudPrefs.saveOptIn3d(context, true)
        shoot("liberty_3d_lake_z11", LAKE_LAT, LAKE_LON, 11.0, forceOnline = true, wantTerrain = true)
        shoot("liberty_3d_river_z14", RIVER_LAT, RIVER_LON, 14.0, forceOnline = true, wantTerrain = true)

        // --- Offline Protomaps, 3D off ---
        activityRule.finishActivity()
        Thread.sleep(800)
        setNetworkEnabled(false)
        MapHudPrefs.saveOptIn3d(context, false)
        shoot("pm_2d_lake_z10", LAKE_LAT, LAKE_LON, 10.0, forceOnline = false, wantTerrain = false)
        shoot("pm_2d_lake_z13", LAKE_LAT, LAKE_LON, 13.0, forceOnline = false, wantTerrain = false)
        shoot("pm_2d_river_z12", RIVER_LAT, RIVER_LON, 12.0, forceOnline = false, wantTerrain = false)
        shoot("pm_2d_river_z14", RIVER_LAT, RIVER_LON, 14.0, forceOnline = false, wantTerrain = false)
        shoot("pm_2d_river_z16", RIVER_LAT, RIVER_LON, 16.0, forceOnline = false, wantTerrain = false)
        shoot("pm_2d_creek_z15", CREEK_LAT, CREEK_LON, 15.0, forceOnline = false, wantTerrain = false)
        shoot(
            "pm_2d_river_z14_nudge",
            RIVER_LAT + 0.015,
            RIVER_LON + 0.02,
            14.0,
            forceOnline = false,
            wantTerrain = false,
        )

        // --- Offline Protomaps + DEM hillshade ---
        MapHudPrefs.saveOptIn3d(context, true)
        shoot("pm_3d_lake_z11", LAKE_LAT, LAKE_LON, 11.0, forceOnline = false, wantTerrain = true)
        shoot("pm_3d_river_z14", RIVER_LAT, RIVER_LON, 14.0, forceOnline = false, wantTerrain = true)

        setNetworkEnabled(true)
        MapHudPrefs.saveOptIn3d(context, false)
        assertTrue(
            "missing liberty_2d_lake_z10",
            shellFileOk("/data/local/tmp/navi_hydro_bleed/liberty_2d_lake_z10.png"),
        )
        assertTrue(
            "missing pm_3d_river_z14",
            shellFileOk("/data/local/tmp/navi_hydro_bleed/pm_3d_river_z14.png"),
        )
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
        Thread.sleep(400)
        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.lastBasemapKind = ""
        NaviMapTestHooks.lastTerrainAttached = false
        NaviMapTestHooks.forceOnlineBasemap = forceOnline
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        activityRule.launchActivity(null)

        waitCamera(lat, lon, zoom)
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
            Thread.sleep(350)
        }
        waitCamera(lat, lon, zoom)
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        waitCamera(lat, lon, zoom)
        assertTrue(
            "map render did not settle for $label",
            InstrumentedMapCapture.awaitRenderSettled(30_000),
        )

        val png = File(cacheOut, "$label.png")
        val req = NaviMapTestHooks.snapshotRequestId + 1
        NaviMapTestHooks.lastSnapshotPng = null
        NaviMapTestHooks.snapshotRequestId = req
        val snapDeadline = System.currentTimeMillis() + 20_000
        while (System.currentTimeMillis() < snapDeadline) {
            val bytes = NaviMapTestHooks.lastSnapshotPng
            if (NaviMapTestHooks.lastSnapshotId >= req && bytes != null && bytes.size > 1_000) {
                png.writeBytes(bytes)
                break
            }
            Thread.sleep(200)
        }
        // Host-pullable shell screencap after the same render-settle gate.
        InstrumentedMapCapture.screencapAfterSettle(
            "/data/local/tmp/navi_hydro_bleed/$label.png",
        )
        android.util.Log.i(
            TAG,
            "HYDRO_SHOT label=$label kind=${NaviMapTestHooks.lastBasemapKind} " +
                "terrain=${NaviMapTestHooks.lastTerrainAttached} zoom=$zoom " +
                "cam=${NaviMapTestHooks.lastCameraLat},${NaviMapTestHooks.lastCameraLon}," +
                "${NaviMapTestHooks.lastCameraZoom} settle=${NaviMapTestHooks.lastRenderSettleId}",
        )
        assertTrue(
            "missing screencap $label",
            shellFileOk("/data/local/tmp/navi_hydro_bleed/$label.png"),
        )
    }

    private fun waitCamera(
        lat: Double,
        lon: Double,
        zoom: Double,
        timeoutMs: Long = 25_000,
    ) {
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (abs(NaviMapTestHooks.lastCameraLat - lat) < 0.12 &&
                abs(NaviMapTestHooks.lastCameraLon - lon) < 0.12 &&
                abs((NaviMapTestHooks.lastCameraZoom ?: 0.0) - zoom) < 1.5
            ) {
                return
            }
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(300)
        }
    }

    private fun setNetworkEnabled(enabled: Boolean) {
        shell(if (enabled) "svc wifi enable" else "svc wifi disable")
        shell(if (enabled) "svc data enable" else "svc data disable")
        Thread.sleep(1_000)
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

    private fun shellFileOk(path: String): Boolean {
        val pfd =
            InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(
                "stat -c %s $path",
            )
        val text =
            java.io.FileInputStream(pfd.fileDescriptor).use { input ->
                input.readBytes().toString(Charsets.UTF_8).trim()
            }
        pfd.close()
        val size = text.lines().firstOrNull()?.toLongOrNull() ?: 0L
        return size > 1_000L
    }

    private fun shellLsCount(dir: String): Int {
        val pfd =
            InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(
                "ls $dir/*.png 2>/dev/null | wc -l",
            )
        val text =
            java.io.FileInputStream(pfd.fileDescriptor).use { input ->
                input.readBytes().toString(Charsets.UTF_8).trim()
            }
        pfd.close()
        return text.lines().firstOrNull()?.toIntOrNull() ?: 0
    }

    companion object {
        private const val TAG = "NaviHydroBleed"

        /** Mjøsa shoreline near Hamar — lake fill. */
        private const val LAKE_LAT = 60.7945
        private const val LAKE_LON = 11.0680

        /** Glomma at Elverum — major river line. */
        private const val RIVER_LAT = 60.8800
        private const val RIVER_LON = 11.5620

        /** Flagstadelva / creek-scale NE of Hamar. */
        private const val CREEK_LAT = 60.8200
        private const val CREEK_LON = 11.1500
    }
}
