package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PlaceSearchHintTest {
    @Test
    fun showsMessageWhenIndexEmpty() {
        val msg =
            placeSearchBuildingMessage(
                hitsEmpty = true,
                indexHasEntries = false,
                indexRunning = true,
            )
        assertEquals(
            "Place index is still building — try coordinates or map tap for now",
            msg,
        )
    }

    @Test
    fun showsMessageForEmptyStubEvenIfJobNotRunning() {
        assertEquals(
            "Place index is still building — try coordinates or map tap for now",
            placeSearchBuildingMessage(true, indexHasEntries = false, indexRunning = false),
        )
    }

    @Test
    fun noMessageWhenQueryHasHits() {
        assertNull(
            placeSearchBuildingMessage(
                hitsEmpty = false,
                indexHasEntries = false,
                indexRunning = true,
            ),
        )
    }

    @Test
    fun noMessageWhenPopulatedIndexHasZeroHits() {
        assertNull(
            placeSearchBuildingMessage(
                hitsEmpty = true,
                indexHasEntries = true,
                indexRunning = false,
            ),
        )
    }

    @Test
    fun skipsLiveGraphWorkOnlyWhilePlanActive() {
        assertTrue(skipLiveGraphWorkDuringForegroundPlan(true))
        assertFalse(skipLiveGraphWorkDuringForegroundPlan(false))
    }

    @Test
    fun planPercentNeverMovesBackwards() {
        assertEquals(0, monotonicPlanPercent(-1, 0))
        assertEquals(50, monotonicPlanPercent(25, 50))
        assertEquals(50, monotonicPlanPercent(50, 25))
        assertEquals(75, monotonicPlanPercent(50, 75))
        assertEquals(50, monotonicPlanPercent(50, null))
        assertEquals(50, monotonicPlanPercent(50, -1))
    }
}

class GpsImmediateWaypointTest {
    @Test
    fun immediateHitMatchesTypedCoordinates() {
        val hit = gpsImmediateCoordHit(59.9139, 10.7522)
        assertEquals(formatCoordWaypointName(59.9139, 10.7522), hit.name)
        assertEquals("coordinate", hit.kind)
        assertEquals(59.9139, hit.lat, 1e-9)
        assertEquals(10.7522, hit.lon, 1e-9)
    }

    @Test
    fun upgradesWhenSameFixAndRealName() {
        assertTrue(
            gpsWaypointShouldUpgrade(
                59.91,
                10.75,
                formatCoordWaypointName(59.91, 10.75),
                59.91,
                10.75,
                "Welhavens gate",
                "map-resolved",
            ),
        )
    }

    @Test
    fun doesNotUpgradeWhenUserReplacedWaypoint() {
        assertFalse(
            gpsWaypointShouldUpgrade(
                60.0,
                11.0,
                "Somewhere else",
                59.91,
                10.75,
                "Welhavens gate",
                "map-resolved",
            ),
        )
    }

    @Test
    fun leavesCoordsWhenResolveFails() {
        assertFalse(
            gpsWaypointShouldUpgrade(
                59.91,
                10.75,
                formatCoordWaypointName(59.91, 10.75),
                59.91,
                10.75,
                formatMapMarkFallback(59.91, 10.75),
                "map-mark",
            ),
        )
    }
}
