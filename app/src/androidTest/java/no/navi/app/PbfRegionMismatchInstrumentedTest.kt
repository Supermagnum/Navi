package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/** Step-4 gate: refuse pairing Østlandet PBF with Vestlandet region id. */
@RunWith(AndroidJUnit4::class)
class PbfRegionMismatchInstrumentedTest {
    private companion object {
        const val TAG = "PbfRegionMismatch"
    }

    @Test
    fun indexed_maps_refuses_ostlandet_pbf_for_vestlandet() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dir = File(NaviAppData.resolve(ctx), "pbf_mismatch_gate")
        dir.mkdirs()
        val ost = File(dir, "ostlandet-latest.osm.pbf")
        ost.writeBytes(ByteArray(20_000))

        IndexedMapsBackground.ensureStarted(
            ost,
            dir,
            null,
            "europe/norway/vestlandet",
        )
        // Give the sync reject path a tick (mismatch returns before coroutine work).
        Thread.sleep(200)
        val status = IndexedMapsBackground.statusLine()
        Log.i(
            TAG,
            "local-bake pbf resolved region_id=europe/norway/vestlandet pbf=${ost.absolutePath} " +
                "expected_prefix=europe/norway/vestlandet status=$status",
        )
        assertTrue(
            "expected mismatch failure status, got $status",
            status.contains("mismatch", ignoreCase = true),
        )
        assertFalse(IndexedMapsBackground.isRunning())
    }
}
