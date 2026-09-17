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
import uniffi.navi.namedBuildingsInBbox
import java.io.File

/**
 * Device confirm: OSM way/435718754 (Reinsfjellhytta — user-linked as Hatthytta)
 * is queryable as kind=building and the named-building overlay paints at zoom 16+.
 */
@RunWith(AndroidJUnit4::class)
class NamedBuildingReinsfjellhyttaInstrumentedTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context
    private lateinit var dataDir: File
    private lateinit var outDir: File

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        outDir =
            File(context.cacheDir, "navi_named_building").also {
                it.mkdirs()
                it.listFiles()?.forEach { f -> f.delete() }
            }
        shell("mkdir -p /data/local/tmp/navi_named_building && chmod 777 /data/local/tmp/navi_named_building")
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.styleReady = false
        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
    }

    @After
    fun tearDown() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.disableGpsFollow = false
        runCatching { activityRule.finishActivity() }
    }

    @Test
    fun reinsfjellhytta_way_435718754_in_bbox_and_on_map() {
        val placeDb = File(dataDir, "place_index.db")
        assertTrue("missing place_index.db at ${placeDb.absolutePath}", placeDb.isFile)

        val hits =
            namedBuildingsInBbox(
                placeDb.absolutePath,
                61.88,
                10.73,
                61.89,
                10.74,
                32u,
            )
        val match =
            hits.firstOrNull {
                it.osmId == 435718754L ||
                    it.name.contains("Reinsfjellhytta", ignoreCase = true)
            }
        assertTrue(
            "expected Reinsfjellhytta (way/435718754) as kind=building; got $hits",
            match != null && match.kind == "building",
        )

        NaviMapTestHooks.pendingCamera = Triple(BUILDING_LAT, BUILDING_LON, 16.5)
        activityRule.launchActivity(null)

        val bootDeadline = System.currentTimeMillis() + 60_000
        while (System.currentTimeMillis() < bootDeadline) {
            if (NaviMapTestHooks.styleReady) break
            NaviMapTestHooks.pendingCamera = Triple(BUILDING_LAT, BUILDING_LON, 16.5)
            Thread.sleep(400)
        }
        assertTrue("style not ready", NaviMapTestHooks.styleReady)

        // Hold camera while overlay refreshes.
        repeat(8) {
            NaviMapTestHooks.pendingCamera = Triple(BUILDING_LAT, BUILDING_LON, 16.5)
            Thread.sleep(500)
        }

        val shot = File(outDir, "reinsfjellhytta_z16.png")
        val ui =
            InstrumentationRegistry.getInstrumentation().uiAutomation
        val bitmap = ui.takeScreenshot()
        java.io.FileOutputStream(shot).use { out ->
            bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, out)
        }
        assertTrue("screenshot missing ${shot.absolutePath}", shot.isFile && shot.length() > 10_000)
        // Host pull via run-as (app-private cache; /data/local/tmp may be EACCES).
        shell("run-as no.navi.app cp ${shot.absolutePath} /data/local/tmp/reinsfjellhytta_z16.png || true")
        shell("chmod 666 /data/local/tmp/reinsfjellhytta_z16.png || true")

        // Re-query after map idle — same bbox must still hit.
        val after =
            namedBuildingsInBbox(
                placeDb.absolutePath,
                61.88,
                10.73,
                61.89,
                10.74,
                32u,
            )
        assertTrue(
            "bbox empty after map settle: $after",
            after.any { it.osmId == 435718754L },
        )
    }

    private fun shell(cmd: String) {
        InstrumentationRegistry
            .getInstrumentation()
            .uiAutomation
            .executeShellCommand(cmd)
            .close()
    }

    companion object {
        // OSM way/435718754 centroid
        private const val BUILDING_LAT = 61.885468
        private const val BUILDING_LON = 10.737097
    }
}
