package no.navi.app

import org.junit.Assert.assertFalse
import org.junit.Test

/**
 * Build-breaking guard: the DATEX plugin must ship with its enable toggle
 * defaulting to OFF (opt-in), matching weather and plugins.md.
 */
class DatexPluginDefaultOffTest {
    @Test
    fun datexPluginDefaultEnabledIsFalse() {
        assertFalse(
            "DATEX_PLUGIN_DEFAULT_ENABLED must remain false (opt-in plugin)",
            MapHudPrefs.DATEX_PLUGIN_DEFAULT_ENABLED,
        )
        assertFalse(MapHudPrefs.DATEX_PLUGIN_DEFAULT_ENABLED)
    }
}
