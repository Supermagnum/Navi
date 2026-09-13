package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Planet PMTiles URL prefs must default to empty — never a hardcoded planet URL.
 */
class PmtilesBaseUrlDefaultTest {
    @Test
    fun prefs_key_default_is_empty_string() {
        // MapHudPrefs.loadPmtilesBaseUrl uses SharedPreferences default "".
        assertEquals("", MapHudPrefs.PMTILES_BASE_URL_DEFAULT)
        assertTrue(MapHudPrefs.PMTILES_BASE_URL_DEFAULT.isEmpty())
    }
}
