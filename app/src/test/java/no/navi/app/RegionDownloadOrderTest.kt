package no.navi.app

import org.junit.Assert.assertTrue
import org.junit.Test

/** Documents Download-region phase order after the basemap-non-blocking reorder. */
class RegionDownloadOrderTest {
    @Test
    fun usable_status_prefix_is_stable_for_ui() {
        assertTrue(
            RegionDownloadBackground.USABLE_STATUS_PREFIX.startsWith("Place index ready"),
        )
    }
}
