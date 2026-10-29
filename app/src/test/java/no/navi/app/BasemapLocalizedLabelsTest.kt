package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Locale

class BasemapLocalizedLabelsTest {
    @Test
    fun norwegianIncludesNoFallback() {
        val keys = BasemapLocalizedLabels.preferredNameKeys(Locale("nb", "NO"))
        assertTrue(keys.contains("name:nb"))
        assertTrue(keys.contains("name:no"))
        assertTrue(keys.contains("int_name"))
        assertTrue(keys.contains("name:en"))
    }

    @Test
    fun arabicSkipsLatinFallbacksBeforeLocal() {
        val keys = BasemapLocalizedLabels.preferredNameKeys(Locale("ar", "TN"))
        assertTrue(keys.contains("name:ar"))
        assertFalse(keys.contains("int_name"))
        assertFalse(keys.contains("name:en"))
    }

    @Test
    fun englishIncludesIntNameChain() {
        val keys = BasemapLocalizedLabels.preferredNameKeys(Locale.US)
        assertEquals("name:en-us", keys.first())
        assertTrue(keys.contains("int_name"))
        assertTrue(keys.contains("name:latin"))
    }

    @Test
    fun englishIncludesLatinNameXxAfterEnWithFrFirst() {
        val keys = BasemapLocalizedLabels.preferredNameKeys(Locale.US)
        assertTrue(keys.contains("name:fr"))
        assertTrue(keys.contains("name:de"))
        val latinIdx = keys.indexOf("name:latin")
        val frIdx = keys.indexOf("name:fr")
        assertTrue(latinIdx >= 0 && frIdx > latinIdx)
        assertEquals(
            BasemapLocalizedLabels.LATIN_NAME_FALLBACK_LANGS.first(),
            "fr",
        )
        // Arabic still omits the Latin xx chain.
        val ar = BasemapLocalizedLabels.preferredNameKeys(Locale("ar"))
        assertFalse(ar.contains("name:fr"))
    }
}
