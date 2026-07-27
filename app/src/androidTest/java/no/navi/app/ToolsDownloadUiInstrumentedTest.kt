package no.navi.app

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
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
import java.io.File

/**
 * Exercises the real Tools UI download buttons (not UniFFI-only shortcuts).
 * Uses `test/oslo` for a fast Protomaps extract via the on-screen controls.
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
    fun tools_basemap_download_button_completes_oslo_extract() {
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

        NaviMapTestHooks.pendingGeofabrikPath = "test/oslo"
        Thread.sleep(800)

        composeRule
            .onNodeWithTag("btn_download_pmtiles", useUnmergedTree = true)
            .performScrollTo()
            .performClick()

        val deadline = System.currentTimeMillis() + 180_000
        var completed = false
        while (System.currentTimeMillis() < deadline) {
            val jobs = pmtilesListJobs(dataDir.absolutePath)
            if (jobs.any { it.regionKey.contains("oslo") && it.status == "completed" }) {
                completed = true
                break
            }
            Thread.sleep(1_000)
        }
        assertTrue(
            "basemap download via Tools button did not complete for test/oslo",
            completed,
        )
        val covering = pmtilesListCovering(dataDir.absolutePath, 59.91, 10.75)
        assertTrue(
            "Oslo camera should be covered after UI download",
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
        demJobs.firstOrNull { it.regionKey.contains("dem") && it.id.isNotBlank() }?.let { job ->
            pmtilesCancelJob(job.id)
        }

        shell("screencap -p /data/local/tmp/tools_download_basemap_done.png")
        shell("chmod 644 /data/local/tmp/tools_download_basemap_done.png")
    }
}
