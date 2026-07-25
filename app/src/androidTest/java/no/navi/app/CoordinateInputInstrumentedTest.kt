package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class CoordinateInputInstrumentedTest {
    @Test
    fun parsesCommaSeparatedLatLon() {
        val parsed = parseLatLonQuery("60.562480, 11.256282")
        assertEquals(60.562480, parsed!!.first, 1e-9)
        assertEquals(11.256282, parsed.second, 1e-9)
    }

    @Test
    fun parsesSpaceSeparatedLatLon() {
        val parsed = parseLatLonQuery("67.2804 14.4050")
        assertEquals(67.2804, parsed!!.first, 1e-9)
        assertEquals(14.4050, parsed.second, 1e-9)
    }

    @Test
    fun rejectsPlaceNames() {
        assertNull(parseLatLonQuery("Bodø"))
        assertNull(parseLatLonQuery("Oslo Sentralstasjon"))
    }

    @Test
    fun rejectsIntegersWithoutDecimal() {
        assertNull(parseLatLonQuery("60 11"))
    }

    @Test
    fun rejectsOutOfRange() {
        assertNull(parseLatLonQuery("91.0, 10.0"))
        assertNull(parseLatLonQuery("60.0, 181.0"))
    }
}
