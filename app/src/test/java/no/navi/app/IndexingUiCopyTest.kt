package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class IndexingUiCopyTest {
    @Test
    fun sanitizesPlaceIndexAndIndexedMapsLabels() {
        assertEquals(
            "Preparing search: writing database… 55%",
            IndexingUiCopy.sanitizeProgress("Place index: writing database… 55%"),
        )
        assertEquals(
            "Region ready 100% (6 / 6)",
            IndexingUiCopy.sanitizeProgress("Place index ready 100% (6 / 6)"),
        )
        assertEquals(
            "Preparing maps (background): scanning",
            IndexingUiCopy.sanitizeProgress("Indexed maps (background): scanning"),
        )
        assertEquals(
            "Ostlandet: preparing…",
            IndexingUiCopy.sanitizeProgress("Ostlandet: indexing…"),
        )
        val cleaned = IndexingUiCopy.sanitizeProgress("Slow plan: indexed maps not ready")
        assertFalse(cleaned.contains("index", ignoreCase = true))
    }

    @Test
    fun longTripStateLabelsAvoidIndexVocabulary() {
        assertEquals("Installing", IndexingUiCopy.longTripStateLabel("Indexing"))
        assertEquals("Ready", IndexingUiCopy.longTripStateLabel("Indexed"))
        assertEquals("Downloading", IndexingUiCopy.longTripStateLabel("Downloading"))
    }
}
