package no.navi.app

import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.pmtilesCancelJob
import uniffi.navi.pmtilesListCovering
import uniffi.navi.pmtilesListJobs
import uniffi.navi.pmtilesQueueRegion
import uniffi.navi.pmtilesRunJob
import java.io.File

/**
 * Tools UI: standalone basemap button is gone; Download region owns packs+basemap.
 * DEM remains a separate control. Uses FFI for a fast Oslo PMTiles covering check
 * (`test/oslo` is not a Geofabrik extract).
 */
@RunWith(AndroidJUnit4::class)
class ToolsDownloadUiInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var dataDir: File

    @Before
    fun setUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        File(dataDir, "pmtiles/test_oslo.pmtiles").delete()
        File(dataDir, "pmtiles/test_oslo_dem.pmtiles").delete()
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

    private fun waitForToolsButton(timeoutMs: Long = 60_000) {
        val deadline = System.currentTimeMillis() + timeoutMs
        var last: Throwable? = null
        while (System.currentTimeMillis() < deadline) {
            try {
                composeRule.waitForIdle()
                composeRule.onNodeWithTag("btn_tools", useUnmergedTree = true).assertExists()
                return
            } catch (t: Throwable) {
                last = t
                Thread.sleep(500)
            }
        }
        throw IllegalStateException("btn_tools never appeared", last)
    }

    @Test
    fun tools_region_download_owns_basemap_dem_still_separate() {
        waitForToolsButton()
        Thread.sleep(1_000)

        composeRule
            .onNodeWithTag("btn_tools", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        composeRule.onNodeWithTag("tools_menu", useUnmergedTree = true).assertIsDisplayed()

        composeRule
            .onNodeWithTag("field_geofabrik_path", useUnmergedTree = true)
            .performScrollTo()
            .assertIsDisplayed()

        composeRule
            .onNodeWithTag("btn_download_region", useUnmergedTree = true)
            .performScrollTo()
            .assertIsDisplayed()

        composeRule
            .onAllNodesWithTag("btn_download_pmtiles", useUnmergedTree = true)
            .assertCountEquals(0)

        NaviMapTestHooks.pendingGeofabrikPath = "test/oslo"
        Thread.sleep(800)

        // Fast covering check via FFI (same extract Download region runs after packs).
        val job = pmtilesQueueRegion(dataDir.absolutePath, "test/oslo", null)
        assertTrue("queue failed: ${job.status}", job.id.isNotBlank())
        val done = pmtilesRunJob(dataDir.absolutePath, job.id)
        assertTrue("extract failed: ${done.status}", done.status == "completed")
        val covering = pmtilesListCovering(dataDir.absolutePath, 59.91, 10.75)
        assertTrue(
            "Oslo camera should be covered after PMTiles extract",
            covering.any { File(it.localPath).isFile },
        )

        // DEM button: queue via Tools UI, then cancel (full DEM extract is large).
        composeRule
            .onNodeWithTag("btn_download_dem", useUnmergedTree = true)
            .performScrollTo()
            .assertIsDisplayed()
            .performClick()
        Thread.sleep(2_500)
        val demJobs = pmtilesListJobs(dataDir.absolutePath)
        assertTrue(
            "DEM download should create a job for oslo dem",
            demJobs.any { it.regionKey.contains("oslo") && it.regionKey.contains("dem") },
        )
        demJobs.firstOrNull { it.regionKey.contains("dem") && it.id.isNotBlank() }?.let { jobDem ->
            pmtilesCancelJob(jobDem.id)
        }

        shell("screencap -p /data/local/tmp/tools_download_basemap_done.png")
        shell("chmod 644 /data/local/tmp/tools_download_basemap_done.png")
    }
}
