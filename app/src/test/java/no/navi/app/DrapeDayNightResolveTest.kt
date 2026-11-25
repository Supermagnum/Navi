package no.navi.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DrapeDayNightResolveTest {
    @Test
    fun systemFollowsUiNight() {
        assertTrue(
            resolveDrapeNight(
                mode = DrapeDayNight.SYSTEM,
                systemNight = true,
                gpsLat = 60.0,
                gpsLon = 11.0,
                utcSeconds = 0L,
                isDayAt = { _, _, _ -> true },
                hourOfDay = 12,
            ),
        )
        assertFalse(
            resolveDrapeNight(
                mode = DrapeDayNight.SYSTEM,
                systemNight = false,
                gpsLat = 60.0,
                gpsLon = 11.0,
                utcSeconds = 0L,
                isDayAt = { _, _, _ -> false },
                hourOfDay = 12,
            ),
        )
    }

    @Test
    fun autoSunUsesSolarWhenGpsPresent() {
        assertFalse(
            resolveDrapeNight(
                mode = DrapeDayNight.AUTO_SUN,
                systemNight = true,
                gpsLat = 60.79,
                gpsLon = 11.09,
                utcSeconds = 1_700_000_000L,
                isDayAt = { _, _, _ -> true },
                hourOfDay = 12,
            ),
        )
        assertTrue(
            resolveDrapeNight(
                mode = DrapeDayNight.AUTO_SUN,
                systemNight = false,
                gpsLat = 60.79,
                gpsLon = 11.09,
                utcSeconds = 1_700_000_000L,
                isDayAt = { _, _, _ -> false },
                hourOfDay = 12,
            ),
        )
    }

    @Test
    fun autoSunWithoutGpsUsesClockNotSystemUi() {
        // Phone UI dark, but local daytime → day map.
        assertFalse(
            resolveDrapeNight(
                mode = DrapeDayNight.AUTO_SUN,
                systemNight = true,
                gpsLat = 0.0,
                gpsLon = 0.0,
                utcSeconds = 0L,
                isDayAt = { _, _, _ -> error("unused") },
                hourOfDay = 9,
            ),
        )
        // Local night → night map even if system UI is light.
        assertTrue(
            resolveDrapeNight(
                mode = DrapeDayNight.AUTO_SUN,
                systemNight = false,
                gpsLat = 0.0,
                gpsLon = 0.0,
                utcSeconds = 0L,
                isDayAt = { _, _, _ -> error("unused") },
                hourOfDay = 22,
            ),
        )
    }
}
