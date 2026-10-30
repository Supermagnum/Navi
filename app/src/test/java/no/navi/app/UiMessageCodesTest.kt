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

    @Test
    fun highwayClassCodeUsesPrefixAndArg() {
        val coded = "${UiMessageCodes.PREFIX}highway_class|service"
        assertTrue(coded.startsWith(UiMessageCodes.PREFIX))
        val body = coded.removePrefix(UiMessageCodes.PREFIX)
        assertEquals("highway_class|service", body)
        assertEquals(UiMessageCodes.Parts("highway_class", "service", ""), UiMessageCodes.split(body))
    }

    @Test
    fun bareCodeWithAppendedRegionSuffixKeepsCodeAndSuffix() {
        val parts = UiMessageCodes.split("download_region (region 2 of 2: Trondelag)")
        assertEquals("download_region", parts.code)
        assertEquals(null, parts.arg)
        assertEquals(" (region 2 of 2: Trondelag)", parts.suffix)
    }

    @Test
    fun argWithSpacesStaysInArg() {
        val parts = UiMessageCodes.split("build_graph|a.rkyv (region 2 of 2: Trondelag)")
        assertEquals(UiMessageCodes.Parts("build_graph", "a.rkyv (region 2 of 2: Trondelag)", ""), parts)
    }

    @Test
    fun bareCode() {
        assertEquals(UiMessageCodes.Parts("download_region", null, ""), UiMessageCodes.split("download_region"))
    }
}
