package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import kotlin.math.abs

/**
 * SM-P613: 2D/3D × tilt presets (0 / 35 / 45 / 60°) at Gjendebu valley.
 *
 * With Ostlandet basemap + DEM downloads present: 2D and 3D both use
 * OfflineProtomaps; 3D attaches local Mapterhorn DEM hillshade.
 * Soft hydro fringe in PNGs is the documented capture artifact.
 */
@RunWith(AndroidJUnit4::class)
class Tilt3dMatrixScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var dataDir: File
    private lateinit var context: android.content.Context

    private val lat = 61.493
    private val lon = 8.351
    private val zoom = 12.0

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        OstlandetOfflineFixtures.ensureInstalled(dataDir)
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.gpsAltitudeM = 1000.0
        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
    }

    private fun waitCamera() {
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        val deadline = System.currentTimeMillis() + 25_000
        while (System.currentTimeMillis() < deadline) {
            if (abs(NaviMapTestHooks.lastCameraLat - lat) < 0.15 &&
                abs(NaviMapTestHooks.lastCameraLon - lon) < 0.15 &&
                abs(NaviMapTestHooks.lastCameraZoom - zoom) < 1.5
            ) {
                return
            }
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(400)
        }
    }

    private fun applyMode(
        want3d: Boolean,
        tilt: Double,
    ) {
        NaviMapTestHooks.requestOptIn3d = want3d
        NaviMapTestHooks.requestCameraTiltDeg = tilt
        MapHudPrefs.saveOptIn3d(context, want3d)
        MapHudPrefs.saveCameraTiltDeg(context, tilt)
        waitCamera()
        val deadline = System.currentTimeMillis() + 45_000
        while (System.currentTimeMillis() < deadline) {
            val pitchOk = abs(NaviMapTestHooks.lastCameraPitch - tilt) <= 2.0
            val kind = NaviMapTestHooks.lastBasemapKind
            val kindOk =
                if (want3d) {
                    kind == "OfflineProtomaps" && NaviMapTestHooks.lastTerrainAttached
                } else {
                    kind == "OfflineProtomaps" && !NaviMapTestHooks.lastTerrainAttached
                }
            if (pitchOk && kindOk && NaviMapTestHooks.styleReady) return
            NaviMapTestHooks.requestOptIn3d = want3d
            NaviMapTestHooks.requestCameraTiltDeg = tilt
            if (System.currentTimeMillis() % 4000 < 500) {
                NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            }
            Thread.sleep(400)
        }
    }

    private fun capture(
        label: String,
        want3d: Boolean,
        tilt: Double,
    ): File {
        applyMode(want3d, tilt)
        assertTrue(
            "pitch for $label: expected ~$tilt got ${NaviMapTestHooks.lastCameraPitch}",
            abs(NaviMapTestHooks.lastCameraPitch - tilt) <= 2.5,
        )
        assertEquals("OfflineProtomaps", NaviMapTestHooks.lastBasemapKind)
        if (want3d) {
            assertTrue(
                "local Mapterhorn hillshade must attach for $label",
                NaviMapTestHooks.lastTerrainAttached,
            )
        } else {
            assertFalse(
                "terrain must detach for $label",
                NaviMapTestHooks.lastTerrainAttached,
            )
        }
        assertTrue("style ready $label", InstrumentedMapCapture.awaitRenderSettled(30_000))
        val shot = InstrumentedMapCapture.takeScreenshotAfterSettle(8_000)
        assertTrue("null shot $label", shot != null)
        assertNotEquals(0, shot!!.width)
        val name = "tilt3d_$label.png"
        val out = File(dataDir, name)
        out.outputStream().use { shot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        assertTrue("$name too small", out.length() > 5_000)
        InstrumentedMapCapture.screencapAfterSettle("/data/local/tmp/$name", 5_000)
        android.util.Log.i(
            "Tilt3dMatrix",
            "PASS label=$label want3d=$want3d tilt=$tilt " +
                "pitch=${NaviMapTestHooks.lastCameraPitch} " +
                "terrain=${NaviMapTestHooks.lastTerrainAttached} " +
                "kind=${NaviMapTestHooks.lastBasemapKind} " +
                "cam=${NaviMapTestHooks.lastCameraLat},${NaviMapTestHooks.lastCameraLon} " +
                "bytes=${out.length()} note=soft_hydro_fringe_in_png_is_known_capture_artifact",
        )
        return out
    }

    @Test
    fun gjendebu_2d_3d_tilt_matrix_eight_shots() {
        activityRule.launchActivity(null)
        Thread.sleep(4_000)
        assertTrue(InstrumentedMapCapture.awaitStyleReady(60_000))
        waitCamera()

        val tilts = MapHudPrefs.CAMERA_TILT_PRESETS
        assertEquals(4, tilts.size)
        assertEquals(60.0, tilts.last(), 0.01)

        val outs = mutableListOf<File>()
        for (want3d in listOf(false, true)) {
            for (tilt in tilts) {
                val tag = if (want3d) "3d" else "2d"
                val label = "${tag}_tilt${tilt.toInt()}"
                outs += capture(label, want3d, tilt)
            }
        }
        assertEquals(8, outs.size)
        android.util.Log.i("Tilt3dMatrix", "MATRIX_DONE files=${outs.map { it.name }}")
    }
}
