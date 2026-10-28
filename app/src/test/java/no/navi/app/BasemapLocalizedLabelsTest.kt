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
}
