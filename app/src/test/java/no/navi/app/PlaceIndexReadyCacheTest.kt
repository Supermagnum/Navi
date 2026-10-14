package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.io.path.createTempDirectory

class PlaceIndexReadyCacheTest {
    @Test
    fun searchAllowedRegions_cacheSurvivesRepeatedCalls() {
        val dir = createTempDirectory("place-ready-cache").toFile()
        FakePlaceIndexRows.install()
        try {
            FakePlaceIndexRows.putRows(dir, "europe/norway/ostlandet", 1_483_135L)
            val first = PlaceIndexReady.searchAllowedRegions(dir)
            assertTrue(first.any { it.contains("ostlandet") })
            // Rows unchanged: second call must hit TTL cache (same set).
            val second = PlaceIndexReady.searchAllowedRegions(dir)
            assertEquals(first, second)
            PlaceIndexReady.clearReady(dir, "europe/norway/ostlandet")
            val afterClear = PlaceIndexReady.searchAllowedRegions(dir)
            assertTrue(afterClear.none { it.contains("ostlandet") })
        } finally {
            dir.deleteRecursively()
            FakePlaceIndexRows.reset()
        }
    }
}
