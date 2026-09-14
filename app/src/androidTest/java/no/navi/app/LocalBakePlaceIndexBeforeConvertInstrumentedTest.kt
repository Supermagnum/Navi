package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.initNativeLogging
import java.io.File

/**
 * Step-5 gate: on the local-bake path, place index must start from the OSM
 * extract without waiting for a full IndexedMaps convert to finish.
 *
 * Stages a pre-downloaded PBF, resumes the region job at [Phase.PLACE_INDEX],
 * and asserts place-index progress appears while convert is either idle or only
 * started after place index (handed off, not a gate).
 */
@RunWith(AndroidJUnit4::class)
class LocalBakePlaceIndexBeforeConvertInstrumentedTest {
    private companion object {
        const val TAG = "LocalBakePlaceIdx"
        const val REGION = "europe/norway/vestlandet"
        const val FILENAME = "vestlandet-latest.osm.pbf"
        const val FIXTURE = "/data/local/tmp/navi_fixtures/espa-atnbrufossen-corridor.osm.pbf"
        const val TIMEOUT_MS = 20L * 60L * 1000L
    }

    @Test
    fun place_index_starts_before_convert_completes_on_local_bake_resume() {
        initNativeLogging()
        val fixture = File(FIXTURE)
        assumeTrue("corridor PBF fixture missing at $FIXTURE", fixture.isFile && fixture.length() > 1_000_000L)

        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = File(NaviAppData.resolve(context), "local_bake_place_idx_gate")
        dataDir.deleteRecursively()
        dataDir.mkdirs()

        val pbf = File(dataDir, FILENAME)
        fixture.copyTo(pbf, overwrite = true)
        Log.i(TAG, "staged pbf=${pbf.absolutePath} bytes=${pbf.length()}")

        // Resume mid-pipeline after extract exists: place index first, convert later.
        File(dataDir, RegionDownloadBackground.JOB_FILE).writeText(
            """
            {
              "url": "https://download.geofabrik.de/europe/norway/vestlandet-latest.osm.pbf",
              "filename": "$FILENAME",
              "geofabrikPath": "$REGION",
              "phase": "place_index"
            }
            """.trimIndent(),
        )

        RegionDownloadBackground.ensureStartedFromPending(context, dataDir)

        val t0 = System.currentTimeMillis()
        var sawPlaceIndex = false
        var sawConvertRunningDuringPlaceIndex = false
        var placeIndexReadyBeforeConvertDone = false
        var finished = false

        while (System.currentTimeMillis() - t0 < TIMEOUT_MS) {
            val status = RegionDownloadBackground.statusLine()
            val ui = RegionDownloadBackground.uiLine()
            val convertRunning = IndexedMapsBackground.isRunning()
            val convertStatus = IndexedMapsBackground.statusLine()

            if (status.contains("place index", ignoreCase = true) ||
                ui.contains("Place index", ignoreCase = true) ||
                ui.contains("building place index", ignoreCase = true)
            ) {
                if (!sawPlaceIndex) {
                    Log.i(
                        TAG,
                        "PLACE_INDEX_STARTED status=$status ui=$ui convertRunning=$convertRunning " +
                            "convertStatus=$convertStatus",
                    )
                }
                sawPlaceIndex = true
                if (convertRunning) {
                    sawConvertRunningDuringPlaceIndex = true
                }
            }

            if (sawPlaceIndex &&
                (
                    status.startsWith(RegionDownloadBackground.USABLE_STATUS_PREFIX) ||
                        status == "done" ||
                        status.startsWith("Place index ready")
                )
            ) {
                // Convert may have been handed off, but must not have been a gate:
                // region job can finish while convert is still running.
                placeIndexReadyBeforeConvertDone = true
                Log.i(
                    TAG,
                    "PLACE_INDEX_READY status=$status convertRunning=$convertRunning " +
                        "convertStatus=$convertStatus",
                )
            }

            if (!RegionDownloadBackground.isRunning() &&
                (status == "done" || status.startsWith("failed") || status.startsWith("Place index ready"))
            ) {
                finished = true
                Log.i(
                    TAG,
                    "FINISHED status=$status sawPlace=$sawPlaceIndex " +
                        "convertDuringPlace=$sawConvertRunningDuringPlaceIndex " +
                        "placeReadyBeforeConvertDone=$placeIndexReadyBeforeConvertDone " +
                        "convertRunning=$convertRunning",
                )
                break
            }

            val elapsed = System.currentTimeMillis() - t0
            if (elapsed % 15_000L < 2_000L) {
                Log.i(
                    TAG,
                    "progress status=$status ui=$ui convertRunning=$convertRunning " +
                        "elapsed_s=${elapsed / 1000}",
                )
            }
            Thread.sleep(1_500)
        }

        assertTrue("timed out waiting for region job", finished)
        assertTrue("expected place index to start from extract", sawPlaceIndex)
        assertTrue(
            "expected place index to become ready without convert gating the region job",
            placeIndexReadyBeforeConvertDone ||
                RegionDownloadBackground.statusLine() == "done",
        )
        // Convert must not have been running as a prerequisite of place index.
        assertFalse(
            "convert must not gate place index (saw convert running during place-index phase)",
            sawConvertRunningDuringPlaceIndex,
        )
        Log.i(TAG, "PASS local-bake place index before convert")
    }
}
