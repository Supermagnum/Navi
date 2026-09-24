package no.navi.app

import org.junit.Assert.assertFalse
import org.junit.Test

/**
 * Build-breaking guard: Stay in Country must ship off by default (same opt-in
 * convention as Long trip / weather plugins).
 */
class StayInCountryPrefsTest {
    @Test
    fun stayInCountryDefaultIsOff() {
        assertFalse(
            "STAY_IN_COUNTRY_DEFAULT must remain false (opt-in route option)",
            MapHudPrefs.STAY_IN_COUNTRY_DEFAULT,
        )
    }
}
