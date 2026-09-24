package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PlaceSearchHintTest {
    @Test
    fun showsOnlineMessageWhenNetworkAvailableAndIndexEmpty() {
        val msg =
            placeSearchBuildingMessage(
                hitsEmpty = true,
                indexHasEntries = false,
                indexRunning = true,
                onlineAvailable = true,
            )
        assertTrue(msg!!.contains("online", ignoreCase = true))
        assertTrue(msg.contains("Nominatim"))
    }

    @Test
    fun showsBuildingMessageWhenOfflineAndIndexRunning() {
        assertEquals(
            "Place index is still building — try coordinates, map tap, or wait for Wi‑Fi search",
            placeSearchBuildingMessage(
                hitsEmpty = true,
                indexHasEntries = false,
                indexRunning = true,
                onlineAvailable = false,
            ),
        )
    }

    @Test
    fun showsConnectMessageWhenOfflineAndNoIndexJob() {
        assertTrue(
            placeSearchBuildingMessage(
                hitsEmpty = true,
                indexHasEntries = false,
                indexRunning = false,
                onlineAvailable = false,
            )!!.contains("connect", ignoreCase = true),
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
    fun showsOnlineMessageWhenPopulatedIndexMissesButNetworkOk() {
        val msg =
            placeSearchBuildingMessage(
                hitsEmpty = true,
                indexHasEntries = true,
                indexRunning = false,
                onlineAvailable = true,
            )
        assertTrue(msg!!.contains("online", ignoreCase = true))
        assertTrue(msg.contains("download region", ignoreCase = true))
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

class OnlinePlaceSearchParseTest {
    @Test
    fun parseNominatimMapsDisplayNameAndCoords() {
        val json =
            """
            [{"place_id":1,"osm_id":844080404,"lat":"61.514623","lon":"8.852972",
              "display_name":"Bessheim Fjellstue, Sjodalsvegen, Vågå, Innlandet, Norway",
              "class":"tourism","type":"alpine_hut",
              "address":{"municipality":"Vågå","suburb":"Sjodalen"}}]
            """.trimIndent()
        val hits = OnlinePlaceSearch.parseNominatimJson(json, 5)
        assertEquals(1, hits.size)
        assertEquals(844080404L, hits[0].osmId)
        assertEquals(61.514623, hits[0].lat, 1e-6)
        assertEquals(8.852972, hits[0].lon, 1e-6)
        assertTrue(hits[0].name.contains("Bessheim"))
        assertTrue(hits[0].kind.startsWith("online/"))
        assertEquals("Vågå", hits[0].municipality)
    }

    @Test
    fun parseOrsMapsLabelAndCoords() {
        val json =
            """
            {"features":[{"geometry":{"coordinates":[10.467007,61.114545],"type":"Point"},
              "properties":{"label":"Lillehammer, Innlandet, Norway","name":"Lillehammer",
              "locality":"Lillehammer","county":"Innlandet"}}]}
            """.trimIndent()
        val hits = OnlinePlaceSearch.parseOrsJson(json, 5)
        assertEquals(1, hits.size)
        assertEquals(61.114545, hits[0].lat, 1e-6)
        assertEquals(10.467007, hits[0].lon, 1e-6)
        assertTrue(hits[0].name.contains("Lillehammer"))
        assertEquals("online/ors/geocode", hits[0].kind)
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
