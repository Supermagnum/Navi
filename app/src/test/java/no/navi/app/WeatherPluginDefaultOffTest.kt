package no.navi.app

import org.junit.Assert.assertFalse
import org.junit.Test

/**
 * Build-breaking guard: the weather plugin must ship with its enable toggle
 * defaulting to OFF (docs/plugins.md). Do not flip this without an explicit
 * product decision.
 */
class WeatherPluginDefaultOffTest {
    @Test
    fun weatherPluginDefaultEnabledIsFalse() {
        assertFalse(
            "WEATHER_PLUGIN_DEFAULT_ENABLED must remain false (opt-in plugin)",
            MapHudPrefs.WEATHER_PLUGIN_DEFAULT_ENABLED,
        )
    }

    @Test
    fun preferenceKeyDefaultConstantMatchesProductRule() {
        // loadWeatherPluginEnabled uses WEATHER_PLUGIN_DEFAULT_ENABLED as the
        // SharedPreferences default — keep that constant false.
        assertFalse(MapHudPrefs.WEATHER_PLUGIN_DEFAULT_ENABLED)
    }
}
