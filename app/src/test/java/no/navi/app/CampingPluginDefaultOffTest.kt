package no.navi.app

import org.junit.Assert.assertFalse
import org.junit.Test

class CampingPluginDefaultOffTest {
    @Test
    fun campingPluginDefaultEnabledIsFalse() {
        assertFalse(
            "CAMPING_PLUGIN_DEFAULT_ENABLED must remain false (opt-in plugin)",
            MapHudPrefs.CAMPING_PLUGIN_DEFAULT_ENABLED,
        )
    }
}
