package no.navi.app

import org.junit.Assert.assertFalse
import org.junit.Test

/**
 * Build-breaking guard: map city weather symbols must default OFF, independently
 * of the main weather plugin toggle.
 */
class MapWeatherSymbolsDefaultOffTest {
    @Test
    fun mapWeatherSymbolsDefaultEnabledIsFalse() {
        assertFalse(
            "WEATHER_MAP_SYMBOLS_DEFAULT_ENABLED must remain false",
            MapHudPrefs.WEATHER_MAP_SYMBOLS_DEFAULT_ENABLED,
        )
    }

    @Test
    fun mainWeatherPluginDefaultStillFalse() {
        assertFalse(MapHudPrefs.WEATHER_PLUGIN_DEFAULT_ENABLED)
    }
}
