package no.navi.app

import org.junit.Assert.assertFalse
import org.junit.Test

class CatPluginDefaultOffTest {
    @Test
    fun cat_plugin_defaults_off() {
        assertFalse(MapHudPrefs.CAT_PLUGIN_DEFAULT_ENABLED)
    }
}
