package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.navi.PlaceHit

class PlaceSearchCountryTest {
    @Test
    fun splitDetectsCommaAndSpaceCountryForms() {
        val deComma = splitCountryQualifiedQuery("Bergen, Germany")
        assertEquals("Bergen", deComma.placeQuery)
        assertEquals("de", deComma.countryIso)

        val deSpace = splitCountryQualifiedQuery("Bergen Germany")
        assertEquals("Bergen", deSpace.placeQuery)
        assertEquals("de", deSpace.countryIso)

        val noComma = splitCountryQualifiedQuery("Bergen, Norway")
        assertEquals("Bergen", noComma.placeQuery)
        assertEquals("no", noComma.countryIso)

        val bare = splitCountryQualifiedQuery("Bergen")
        assertEquals("Bergen", bare.placeQuery)
        assertEquals(null, bare.countryIso)
    }

    @Test
    fun filterKeepsOnlyMatchingCountryIso() {
        val hits =
            listOf(
                PlaceHit(
                    1L,
                    "Bergen",
                    kindWithCountryIso("online/place/city", "no"),
                    60.39,
                    5.32,
                    "Vestland",
                    "Bergen",
                    "europe/norway/vestlandet",
                ),
                PlaceHit(
                    2L,
                    "Bergen",
                    kindWithCountryIso("online/place/city", "de"),
                    52.80,
                    9.96,
                    "Niedersachsen",
                    "Bergen",
                    "europe/germany/niedersachsen",
                ),
                PlaceHit(
                    3L,
                    "Bergen",
                    kindWithCountryIso("online/place/city", "nl"),
                    52.66,
                    4.68,
                    "",
                    "Bergen",
                    "europe/netherlands",
                ),
            )
        val deOnly = filterHitsByCountryIso(hits, "de")
        assertEquals(1, deOnly.size)
        assertEquals("de", placeHitCountryIso(deOnly[0]))
        val noOnly = filterHitsByCountryIso(hits, "no")
        assertEquals(1, noOnly.size)
        assertEquals(60.39, noOnly[0].lat, 0.01)
    }

    @Test
    fun multiCandidateLabelsIncludeCountry() {
        val no =
            PlaceHit(
                1L,
                "Bergen",
                kindWithCountryIso("online/place/city", "no"),
                60.39,
                5.32,
                "Vestland",
                "Bergen",
                "europe/norway/vestlandet",
            )
        val de =
            PlaceHit(
                2L,
                "Bergen",
                kindWithCountryIso("online/place/city", "de"),
                52.80,
                9.96,
                "Niedersachsen",
                "Bergen",
                "europe/germany/niedersachsen",
            )
        val labelNo = placeHitSearchLabel(no, disambiguate = true)
        val labelDe = placeHitSearchLabel(de, disambiguate = true)
        assertTrue(labelNo.contains("Norway", ignoreCase = true))
        assertTrue(labelDe.contains("Germany", ignoreCase = true))
        assertTrue(labelNo.contains("Bergen", ignoreCase = true))
        // Single-candidate mode stays compact.
        assertEquals(placeHitDisplayLabel(no), placeHitSearchLabel(no, disambiguate = false))
    }

    @Test
    fun kalmarWholeTokenMergeStillDropsBergenPrefix() {
        val online =
            listOf(
                PlaceHit(
                    1L,
                    "Kalmar",
                    kindWithCountryIso("online/place/city", "se"),
                    56.66,
                    16.36,
                    "",
                    "Kalmar kommun",
                    "europe/sweden/kalmar",
                ),
            )
        val offline =
            listOf(
                PlaceHit(
                    2L,
                    "Kalmargaten barnehage",
                    "amenity:kindergarten",
                    60.39,
                    5.32,
                    "Engen",
                    "Bergen",
                    "europe/norway/vestlandet",
                ),
            )
        val merged = mergeOnlineAndOfflinePlaceHits("Kalmar", online, offline)
        assertEquals(1, merged.size)
        assertEquals("Kalmar", merged[0].name)
    }
}
