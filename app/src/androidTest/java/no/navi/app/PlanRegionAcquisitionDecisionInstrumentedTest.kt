package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiRegionSourceKind
import uniffi.navi.decideRegionAcquisition
import uniffi.navi.initNativeLogging
import java.io.File

/**
 * Step-1 gate: catalog probe must classify timeout vs not_in_catalog, and a
 * healthy host must keep execute_local=false for a published region.
 */
@RunWith(AndroidJUnit4::class)
class PlanRegionAcquisitionDecisionInstrumentedTest {
    private companion object {
        const val TAG = "PlanRegionAcq"
        const val MONACO = "europe/monaco"
        const val MISSING = "europe/norway/this-region-does-not-exist-zz"
    }

    @Test
    fun published_region_installs_with_reason_ok() {
        initNativeLogging()
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = File(NaviAppData.resolve(ctx), "plan_acq_step1_monaco")
        dataDir.mkdirs()

        val d =
            decideRegionAcquisition(
                regionId = MONACO,
                packServerBaseUrl = null,
                dataDir = dataDir.absolutePath,
            )
        Log.i(
            TAG,
            "monaco source=${d.source} execute_local=${d.executeLocalConvert} " +
                "decision_reason=${d.decisionReason} data_source=${d.dataSource} " +
                "reason=${d.reason.take(240)}",
        )
        assertEquals(FfiRegionSourceKind.SERVER, d.source)
        assertEquals("ok", d.decisionReason)
        assertFalse(
            "healthy pack host must not force local bake for published monaco; reason=${d.reason}",
            d.executeLocalConvert,
        )
        assertEquals("server-duckdns", d.dataSource)
    }

    @Test
    fun second_decide_after_first_still_ok_not_timeout() {
        initNativeLogging()
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = File(NaviAppData.resolve(ctx), "plan_acq_step1_monaco2")
        dataDir.mkdirs()

        val first =
            decideRegionAcquisition(
                regionId = MONACO,
                packServerBaseUrl = null,
                dataDir = dataDir.absolutePath,
            )
        Log.i(
            TAG,
            "first execute_local=${first.executeLocalConvert} decision_reason=${first.decisionReason}",
        )
        // Immediate second probe mimics region-2 decide under residual load.
        val second =
            decideRegionAcquisition(
                regionId = MONACO,
                packServerBaseUrl = null,
                dataDir = dataDir.absolutePath,
            )
        Log.i(
            TAG,
            "second execute_local=${second.executeLocalConvert} decision_reason=${second.decisionReason} " +
                "reason=${second.reason.take(240)}",
        )
        assertEquals("ok", second.decisionReason)
        assertFalse(second.executeLocalConvert)
        assertTrue(
            "timeout must not be reported as not_in_catalog",
            second.decisionReason != "not_in_catalog" && second.decisionReason != "timeout",
        )
    }

    @Test
    fun missing_region_is_not_in_catalog_not_timeout() {
        initNativeLogging()
        val d =
            decideRegionAcquisition(
                regionId = MISSING,
                packServerBaseUrl = null,
                dataDir = null,
            )
        Log.i(
            TAG,
            "missing source=${d.source} execute_local=${d.executeLocalConvert} " +
                "decision_reason=${d.decisionReason} reason=${d.reason.take(240)}",
        )
        assertEquals(FfiRegionSourceKind.LOCAL, d.source)
        assertTrue(d.executeLocalConvert)
        assertEquals("not_in_catalog", d.decisionReason)
    }
}
