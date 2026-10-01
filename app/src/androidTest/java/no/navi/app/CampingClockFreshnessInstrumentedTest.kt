package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.campingPluginClearClockOverride
import uniffi.navi.campingPluginConfigure
import uniffi.navi.campingPluginPeekClock
import uniffi.navi.campingPluginSetClockYmd
import uniffi.navi.campingPluginSetTimezone
import uniffi.navi.initNativeLogging
import java.io.File

/**
 * Phase 5a fix 2: clock_read must sample date/timezone at every guest call,
 * not cache configure-time values.
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
class CampingClockFreshnessInstrumentedTest {
    @Before
    fun setUp() {
        initNativeLogging()
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val files = ctx.filesDir
        val data = File(files, "navi-data").also { it.mkdirs() }
        campingPluginConfigure(files.absolutePath, data.absolutePath, "Europe/Oslo")
    }

    @Test
    fun midnightCrossing_secondPeekSeesNewDate() {
        campingPluginSetTimezone("Europe/Oslo")
        campingPluginSetClockYmd(2026, 6u, 30u)
        val before = campingPluginPeekClock()
        assertNotNull(before)
        assertEquals(2026, before!!.year)
        assertEquals(6u, before.month)
        assertEquals(30u, before.day)
        assertEquals("Europe/Oslo", before.timezone)

        // Simulate device clock moving past midnight.
        campingPluginSetClockYmd(2026, 7u, 1u)
        val after = campingPluginPeekClock()
        assertNotNull(after)
        assertEquals(2026, after!!.year)
        assertEquals(7u, after.month)
        assertEquals(1u, after.day)
        assertNotEquals(before.day, after.day)
    }

    @Test
    fun timezoneChange_notStuckAtConfigure() {
        campingPluginSetTimezone("Europe/Oslo")
        campingPluginClearClockOverride()
        val oslo = campingPluginPeekClock()
        assertNotNull(oslo)
        assertEquals("Europe/Oslo", oslo!!.timezone)

        // DST / zone change: America/New_York vs Europe/Oslo (different offset).
        campingPluginSetTimezone("America/New_York")
        val nyc = campingPluginPeekClock()
        assertNotNull(nyc)
        assertEquals("America/New_York", nyc!!.timezone)
        assertNotEquals(oslo.timezone, nyc.timezone)
        // unix_secs still fresh (within a few seconds of each other).
        assertTrue(kotlin.math.abs(nyc.unixSecs - oslo.unixSecs) < 5)
    }
}
