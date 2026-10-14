package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PlaceSearchHintTest {
    @Test
    fun saysOfflineSearchUnavailableWhenIndexFileUnusable() {
        val offline =
            placeSearchBuildingMessage(
                hitsEmpty = true,
                indexHasEntries = false,
                indexRunning = false,
                indexProblem = "file missing",
            )
        assertEquals(
            "Offline place search unavailable (place index file missing) — enter coordinates or tap the map",
            offline,
        )
        val online =
            placeSearchBuildingMessage(
                hitsEmpty = false,
                indexHasEntries = false,
                indexRunning = false,
                onlineAvailable = true,
                indexProblem = "pack volume unavailable",
            )
        assertTrue(online!!.startsWith("Offline place search unavailable (place index pack volume unavailable)"))
    }

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
        val msg =
            placeSearchBuildingMessage(
                hitsEmpty = true,
                indexHasEntries = false,
                indexRunning = false,
                onlineAvailable = false,
            )!!
        assertTrue(msg.contains("Online place search", ignoreCase = true))
        assertTrue(msg.contains("coordinates", ignoreCase = true))
    }

    @Test
    fun offlineMoreHintWhenTopHitIsMinorPlace() {
        val msg =
            placeSearchOfflineMoreHint(
                onlineEnabled = false,
                topKind = "place:farm",
            )
        assertTrue(msg!!.contains("Online place search", ignoreCase = true))
        assertTrue(msg.contains("download", ignoreCase = true))
        assertNull(placeSearchOfflineMoreHint(onlineEnabled = true, topKind = "place:farm"))
        assertNull(placeSearchOfflineMoreHint(onlineEnabled = false, topKind = "place:city"))
        assertNull(placeSearchOfflineMoreHint(onlineEnabled = false, topKind = "place:village"))
    }

    @Test
    fun noOfflineMatchShownAbovePrefixOnlyHits() {
        val islet =
            uniffi.navi.PlaceHit(1L, "Kolnesholmane", "place:islet", 63.5, 10.0, "", "Frøya", "europe/norway/trondelag")
        val msg = placeSearchNoWholeMatchHint("Köln", listOf(islet), onlineEnabled = false, usedOnline = false)
        assertTrue(msg!!.startsWith("No offline match."))
        assertTrue(msg.contains("Online place search"))
        assertEquals(
            "No offline match.",
            placeSearchNoWholeMatchHint("Köln", listOf(islet), onlineEnabled = true, usedOnline = false),
        )
        assertNull(placeSearchNoWholeMatchHint("Köln", listOf(islet), onlineEnabled = true, usedOnline = true))
        assertNull(placeSearchNoWholeMatchHint("Köln", emptyList(), onlineEnabled = false, usedOnline = false))
    }

    @Test
    fun noOfflineMatchHiddenWhenAHitMatchesTheWholeQuery() {
        val farm =
            uniffi.navi.PlaceHit(2L, "København", "place:farm", 60.96, 10.55, "", "Gjøvik", "europe/norway/ostlandet")
        val islet =
            uniffi.navi.PlaceHit(3L, "Kobenhavnholmen", "place:islet", 63.5, 10.0, "", "", "europe/norway/trondelag")
        for (q in listOf("København", "Kobenhavn", "københavn")) {
            assertNull(q, placeSearchNoWholeMatchHint(q, listOf(islet, farm), onlineEnabled = false, usedOnline = false))
        }
        val dombas =
            uniffi.navi.PlaceHit(4L, "Dombås", "place:village", 62.07, 9.12, "", "Dovre", "europe/norway/ostlandet")
        assertNull(placeSearchNoWholeMatchHint("Dombas", listOf(dombas), onlineEnabled = false, usedOnline = false))
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
    fun noMatchLineWhenPopulatedIndexHasZeroHits() {
        val msg =
            placeSearchBuildingMessage(
                hitsEmpty = true,
                indexHasEntries = true,
                indexRunning = false,
            )
        assertTrue(msg!!.contains("No offline match"))
        assertTrue(msg.contains("Online place search"))
        assertFalse(msg.contains("Sets the Tools", ignoreCase = true))
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
        assertFalse(msg.contains("download region", ignoreCase = true))
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
