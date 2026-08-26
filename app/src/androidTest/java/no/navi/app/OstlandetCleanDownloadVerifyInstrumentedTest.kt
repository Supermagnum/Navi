package no.navi.app

import android.util.Log
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.pmtilesListJobs
import java.io.File
import java.io.FileInputStream

/**
 * Clean-slate verification: Tools → Download basemap (PMTiles) for
 * `europe/norway/ostlandet` must produce a maxzoom-15 extract (~1.16 GB),
 * not the staged maxzoom-12 fixture (~192 MB).
 *
 * Drives the real Tools button ([btn_download_pmtiles]), not
 * [OfflinePmtilesBootstrap].
 */
@RunWith(AndroidJUnit4::class)
class OstlandetCleanDownloadVerifyInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var dataDir: File

    @Before
    fun setUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        // Preconditions: no residual basemap; staged fixtures must not be present
        // under the restore path (host should park /data/local/tmp/navi_fixtures).
        val pm = File(dataDir, "pmtiles/europe_norway_ostlandet.pmtiles")
        check(!pm.isFile || pm.length() < 1_000L) {
            "expected empty basemap before clean download, got bytes=${pm.length()}"
        }
        check(!File("/data/local/tmp/navi_fixtures/europe_norway_ostlandet.pmtiles").isFile) {
            "staged mz12 fixture still at restore path — park it before this test"
        }
    }

    private fun waitForToolsButton(timeoutMs: Long = 90_000) {
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
    fun tools_ostlandet_download_is_maxzoom_15_full_size() {
        waitForToolsButton()
        Thread.sleep(1_000)

        composeRule
            .onNodeWithTag("btn_tools", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        composeRule.onNodeWithTag("tools_menu", useUnmergedTree = true).assertIsDisplayed()

        NaviMapTestHooks.pendingGeofabrikPath = "europe/norway/ostlandet"
        Thread.sleep(1_000)

        val started = System.currentTimeMillis()
        composeRule
            .onNodeWithTag("btn_download_pmtiles", useUnmergedTree = true)
            .performScrollTo()
            .performClick()

        // Full Ostlandet range-extract is typically 10–40+ minutes on device Wi‑Fi.
        val deadline = System.currentTimeMillis() + 3 * 60 * 60_000L
        var completed = false
        while (System.currentTimeMillis() < deadline) {
            val jobs = pmtilesListJobs(dataDir.absolutePath)
            val hit =
                jobs.firstOrNull {
                    it.regionKey.contains("ostlandet") &&
                        !it.regionKey.contains("dem") &&
                        it.status == "completed"
                }
            if (hit != null) {
                completed = true
                Log.i(
                    TAG,
                    "completed elapsed_ms=${System.currentTimeMillis() - started} " +
                        "bytes=${hit.bytesReceived} path=${hit.localPath}",
                )
                break
            }
            val running =
                jobs.firstOrNull {
                    it.regionKey.contains("ostlandet") && !it.regionKey.contains("dem")
                }
            if (running != null) {
                Log.i(
                    TAG,
                    "progress status=${running.status} " +
                        "recv=${running.bytesReceived} total=${running.totalBytes}",
                )
            }
            Thread.sleep(5_000)
        }
        assertTrue("Ostlandet Tools download did not complete", completed)

        val basemap = File(dataDir, "pmtiles/europe_norway_ostlandet.pmtiles")
        assertTrue("basemap missing after completed job", basemap.isFile)
        val bytes = basemap.length()
        Log.i(TAG, "basemap bytes=$bytes")
        // Samsung known-good was ~1.16 GB; reject fixture-sized ~192 MB.
        assertTrue(
            "basemap too small (fixture-sized?): $bytes",
            bytes >= 500_000_000L,
        )
        assertTrue(
            "basemap unexpectedly huge: $bytes",
            bytes < 3_000_000_000L,
        )

        val maxzoom = readPmtilesMaxZoom(basemap)
        assertEquals("PMTiles header maxzoom", 15, maxzoom)

        val elapsedMs = System.currentTimeMillis() - started
        Log.i(TAG, "VERIFY_OK bytes=$bytes maxzoom=$maxzoom elapsed_ms=$elapsedMs")
    }

    private fun readPmtilesMaxZoom(pmFile: File): Int {
        val header = ByteArray(127)
        FileInputStream(pmFile).use { input ->
            var off = 0
            while (off < header.size) {
                val n = input.read(header, off, header.size - off)
                check(n > 0) { "short PMTiles header" }
                off += n
            }
        }
        check(String(header, 0, 7, Charsets.US_ASCII) == "PMTiles") { "not PMTiles" }
        return header[101].toInt() and 0xFF
    }

    companion object {
        private const val TAG = "OstlandetCleanDlVerify"
    }
}
