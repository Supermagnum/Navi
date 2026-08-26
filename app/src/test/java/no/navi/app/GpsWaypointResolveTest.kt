package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.navi.PlaceHit

class GpsWaypointResolveTest {
    @Test
    fun prefersAddressWithinHits() {
        val hits =
            listOf(
                PlaceHit(1L, "Finstad", "highway:bus_stop", 60.0, 11.0, "", ""),
                PlaceHit(2L, "Ådalsbrukvegen 134", "addr:housenumber", 60.0, 11.0, "", ""),
            )
        assertEquals(
            "Ådalsbrukvegen 134",
            pickNearbyPlaceNameForGpsWaypoint(hits),
        )
    }

    @Test
    fun usesNearestNameWhenNoAddress() {
        val hits =
            listOf(
                PlaceHit(1L, "Finstad", "highway:bus_stop", 60.80573, 11.32984, "", ""),
            )
        assertEquals("Finstad", pickNearbyPlaceNameForGpsWaypoint(hits))
    }

    @Test
    fun nullWhenEmptyOrBlank() {
        assertNull(pickNearbyPlaceNameForGpsWaypoint(emptyList()))
        assertNull(
            pickNearbyPlaceNameForGpsWaypoint(
                listOf(PlaceHit(1L, "  ", "named", 60.0, 11.0, "", "")),
            ),
        )
    }

    @Test
    fun placeHitDisplayLabelJoinsContextAndSkipsDuplicates() {
        assertEquals(
            "Båberg, Brattberg, Gjøvik",
            placeHitDisplayLabel(
                PlaceHit(1L, "Båberg", "place:farm", 60.97, 10.55, "Brattberg", "Gjøvik"),
            ),
        )
        assertEquals(
            "Espa, Stange",
            placeHitDisplayLabel(
                PlaceHit(2L, "Espa", "place:village", 60.58, 11.27, "", "Stange"),
            ),
        )
        assertEquals(
            "Gjøvik",
            placeHitDisplayLabel(
                PlaceHit(3L, "Gjøvik", "place:town", 60.80, 10.69, "", "Gjøvik"),
            ),
        )
    }

    @Test
    fun fallbackFormat() {
        assertEquals(
            "GPS (60.80573, 11.32984)",
            formatGpsWaypointFallback(60.80573, 11.32984),
        )
    }

    @Test
    fun snapsPinOntoSegmentWithinTwelveMetres() {
        val pts =
            listOf(
                60.0 to 10.0,
                60.0 to 10.001,
            )
        val midLon = 10.0005
        val placeLat = 60.0 + (5.0 / 111_320.0)
        val snap = snapWaypointToRoutePolyline(pts, placeLat, midLon)
        assertNotNull(snap)
        assertTrue(snap!!.distM <= WAYPOINT_ROUTE_PIN_MAX_M)
        assertEquals(60.0, snap.lat, 1e-6)
    }

    @Test
    fun reportsDistanceWhenPlaceFarFromCorridor() {
        val pts = listOf(60.0 to 10.0, 60.0 to 10.001)
        val farLat = 60.0 + (40.0 / 111_320.0)
        val snap = snapWaypointToRoutePolyline(pts, farLat, 10.0005)
        assertNotNull(snap)
        assertTrue(snap!!.distM > WAYPOINT_ROUTE_PIN_MAX_M)
    }
}
