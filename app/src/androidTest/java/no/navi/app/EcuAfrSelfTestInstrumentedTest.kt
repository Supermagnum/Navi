package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.ecuAfrSelfTest

/**
 * On-device ICE AFR / fuel-rate self-test via navi-ffi (no live adapter).
 */
@RunWith(AndroidJUnit4::class)
class EcuAfrSelfTestInstrumentedTest {
    @Test
    fun ecuAfrSelfTest_pureDecode_labeledPass() {
        val report = ecuAfrSelfTest()
        android.util.Log.i("NaviEcu", report)
        assertTrue("must label ECU_AFR_FUEL_RATE: $report", report.contains("TEST_KIND=ECU_AFR_FUEL_RATE"))
        assertTrue("must state DATA_SOURCE=none: $report", report.contains("DATA_SOURCE=none"))
        assertTrue("must PASS: $report", report.trim().endsWith("PASS"))
        assertTrue("must not claim real adapter: $report", !report.contains("DATA_SOURCE=real"))
    }
}
