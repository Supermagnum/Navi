package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class MapLongPressTest {
    @Test
    fun holdDurationIsFourSeconds() {
        assertEquals(4_000L, MAP_LONG_PRESS_HOLD_MS)
    }

    @Test
    fun moveSlopAllowsMinorDriftButNotPanScale() {
        assertTrue(MAP_LONG_PRESS_MOVE_SLOP_PX in 16f..48f)
    }

    @Test
    fun mapMarkFallbackUsesCoords() {
        val name = formatMapMarkFallback(60.849476, 11.368314)
        assertTrue(name.startsWith("Marked ("))
        assertTrue(name.contains("60.84948") || name.contains("60.84947"))
    }
}
