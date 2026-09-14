package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.convertProgressClear
import uniffi.navi.convertProgressSnapshot
import uniffi.navi.ensureIndexedMaps
import uniffi.navi.initNativeLogging
import java.io.File

/**
 * Step-3 gate: local-bake progress must name the real decision_reason, not the
 * catch-all "server pack unavailable".
 */
@RunWith(AndroidJUnit4::class)
class RebuildReasonLabelInstrumentedTest {
    private companion object {
        const val TAG = "RebuildReasonLabel"
    }

    @Test
    fun missing_catalog_region_labels_not_in_catalog() {
        initNativeLogging()
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dir = File(NaviAppData.resolve(ctx), "rebuild_reason_label")
        dir.mkdirs()
        val pbf = File(dir, "fake-leaf-latest.osm.pbf")
        pbf.writeBytes(ByteArray(16 * 1024))

        convertProgressClear()
        val report =
            ensureIndexedMaps(
                pbf.absolutePath,
                dir.absolutePath,
                null,
                "europe/norway/this-region-does-not-exist-zz",
                progressOnConvertChannel = true,
            )
        val conv = convertProgressSnapshot()
        Log.i(TAG, "report=${report.take(300)}")
        Log.i(TAG, "convert_label='${conv.label}'")

        assertFalse(
            "must not use catch-all 'server pack unavailable'; got '${conv.label}' / $report",
            conv.label.contains("server pack unavailable", ignoreCase = true) ||
                report.contains("server pack unavailable", ignoreCase = true),
        )
        assertTrue(
            "expected not_in_catalog in report; label='${conv.label}' report=${report.take(240)}",
            report.contains("not_in_catalog"),
        )
    }
}
