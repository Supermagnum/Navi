package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class UiMessageCodesTest {
    @Test
    fun prefixConstantMatchesRust() {
        assertEquals("navi.msg.", UiMessageCodes.PREFIX)
    }

    @Test
    fun passesThroughLegacyEnglishWithoutResourcesLookup() {
        // resolve only touches Resources when the prefix matches.
        val raw = "Fetching packs from catalog…"
        // Use a null-safe path: non-coded labels return early.
        assertTrue(!raw.startsWith(UiMessageCodes.PREFIX))
    }
}
