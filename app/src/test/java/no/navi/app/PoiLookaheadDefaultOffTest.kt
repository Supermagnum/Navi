package no.navi.app

import org.junit.Assert.assertFalse
import org.junit.Test

/**
 * Build-breaking guard: Nearby attractions must ship disabled (opt-in).
 * JSON HUD parsing uses `org.json` and is covered on-device in
 * [PoiLookaheadHardangerInstrumentedTest] (Android unit tests stub org.json).
 */
class PoiLookaheadDefaultOffTest {
    @Test
    fun poiLookaheadDefaultEnabledIsFalse() {
        assertFalse(
            "POI_LOOKAHEAD_DEFAULT_ENABLED must remain false (opt-in plugin)",
            MapHudPrefs.POI_LOOKAHEAD_DEFAULT_ENABLED,
        )
        assertFalse(MapHudPrefs.POI_LOOKAHEAD_STRICT_HOURS_UNKNOWN_DEFAULT)
    }
}
