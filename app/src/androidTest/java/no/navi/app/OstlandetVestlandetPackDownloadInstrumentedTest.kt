package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.geofabrikLatestPbfUrl
import uniffi.navi.initNativeLogging
import java.io.File

/**
 * ADB / connected-device check: pack-server install for Østlandet then Vestlandet,
 * asserting each region reaches place-index start and finishes the background job.
 */
@RunWith(AndroidJUnit4::class)
class OstlandetVestlandetPackDownloadInstrumentedTest {
    private companion object {
        const val TAG = "OstVestPackDl"
        const val OST = "europe/norway/ostlandet"
        const val VEST = "europe/norway/vestlandet"

        // Pack payloads are multi-GB; allow a long wall clock on Wi-Fi tablet.
        const val PER_REGION_TIMEOUT_MS = 3L * 60L * 60L * 1000L
        const val START_GRACE_MS = 120_000L
    }

    @Test
    fun download_ostlandet_then_vestlandet_packs_and_place_index_starts() {
        initNativeLogging()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = File(NaviAppData.resolve(context), "ost_vest_pack_dl")
        dataDir.mkdirs()
        Log.i(TAG, "dataDir=${dataDir.absolutePath}")

        runRegion(context, dataDir, OST, "ostlandet-latest.osm.pbf")
        runRegion(context, dataDir, VEST, "vestlandet-latest.osm.pbf")
    }

    private fun runRegion(
        context: android.content.Context,
        dataDir: File,
        path: String,
        filename: String,
    ) {
        val url = geofabrikLatestPbfUrl(path)
        Log.i(TAG, "START path=$path url=$url")
        RegionDownloadBackground.ensureStarted(context, dataDir, url, filename, path)

        val t0 = System.currentTimeMillis()
        var sawRunning = false
        var sawPlaceIndex = false
        var sawPackProgress = false
        var finished = false

        // Wait until the background job actually starts (avoid reading prior "done").
        while (System.currentTimeMillis() - t0 < START_GRACE_MS) {
            if (RegionDownloadBackground.isRunning()) {
                sawRunning = true
                break
            }
            Thread.sleep(200)
        }
        assertTrue("background job did not start for $path", sawRunning)
        Log.i(TAG, "RUNNING path=$path status=${RegionDownloadBackground.statusLine()}")

        while (System.currentTimeMillis() - t0 < PER_REGION_TIMEOUT_MS) {
            val running = RegionDownloadBackground.isRunning()
            val status = RegionDownloadBackground.statusLine()
            val ui = RegionDownloadBackground.uiLine()
            if (ui.contains("Fetching packs", ignoreCase = true) ||
                status.contains("Fetching", ignoreCase = true) ||
                ui.contains("Installing packs", ignoreCase = true)
            ) {
                sawPackProgress = true
            }
            if (ui.contains("Place index", ignoreCase = true) ||
                ui.contains("Downloading extract", ignoreCase = true) ||
                status.contains("Downloading extract", ignoreCase = true) ||
                status.contains("place index", ignoreCase = true) ||
                ui.contains("building place index", ignoreCase = true)
            ) {
                if (!sawPlaceIndex) {
                    Log.i(TAG, "PLACE_INDEX_STARTED path=$path status=$status ui=$ui")
                }
                sawPlaceIndex = true
            }
            if (sawRunning && !running && (status == "done" || status.startsWith("failed"))) {
                finished = true
                Log.i(
                    TAG,
                    "FINISHED path=$path status=$status sawPack=$sawPackProgress " +
                        "sawPlace=$sawPlaceIndex",
                )
                break
            }
            val elapsed = System.currentTimeMillis() - t0
            if (elapsed % 30_000L < 2_500L) {
                Log.i(
                    TAG,
                    "progress path=$path running=$running status=$status ui=$ui " +
                        "elapsed_s=${elapsed / 1000}",
                )
            }
            Thread.sleep(2_500)
        }

        assertTrue(
            "timed out waiting for $path (placeIndexStarted=$sawPlaceIndex)",
            finished,
        )
        assertTrue("expected place index to start for $path", sawPlaceIndex)
        val finalStatus = RegionDownloadBackground.statusLine()
        assertTrue(
            "expected done for $path, got $finalStatus",
            finalStatus == "done",
        )
        Log.i(TAG, "PASS path=$path")
    }
}
