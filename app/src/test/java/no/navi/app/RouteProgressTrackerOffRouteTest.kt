package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Confirms [RouteProgressTracker] treats multi-km lateral deviation as off-route
 * and suppresses corridor maneuver guidance (no confidently-wrong approach).
 */
class RouteProgressTrackerOffRouteTest {
    @Test
    fun offRouteFix_suppressesManeuver_freezesAlong() {
        val samples =
            listOf(
                RouteSimSample(60.80, 11.30, 0.0, 50.0, "primary", true, street = "E6"),
                RouteSimSample(60.81, 11.30, 1_000.0, 50.0, "primary", true, street = "E6"),
                RouteSimSample(60.82, 11.30, 2_000.0, 50.0, "primary", true, street = "E6"),
                RouteSimSample(60.83, 11.30, 3_000.0, 50.0, "primary", true, street = "E6"),
            )
        val maneuvers =
            listOf(
                RouteManeuver(
                    lat = 60.825,
                    lon = 11.30,
                    cumM = 2_500.0,
                    kind = "right",
                    street = "Avkjøring",
                    roundaboutExit = null,
                ),
                RouteManeuver(
                    lat = 60.83,
                    lon = 11.30,
                    cumM = 3_000.0,
                    kind = "destination",
                    street = "End",
                    roundaboutExit = null,
                ),
            )
        val tracker =
            RouteProgressTracker(
                samples = samples,
                maneuvers = maneuvers,
                viaPoints = emptyList(),
                endPoint = Waypoint("End", 60.83, 11.30),
                hideDistanceM = 25.0,
                offRouteThresholdM = RouteProgressTracker.OFF_ROUTE_CROSS_TRACK_MOTOR_M,
            )

        val onRoute = tracker.update(60.81, 11.30)
        assertFalse(onRoute.offRoute)
        assertEquals(1_000.0, onRoute.alongM, 1e-6)
        assertNotNull(onRoute.maneuver)
        assertEquals("right", onRoute.maneuver!!.kind)

        val offLat = 60.81
        val offLon = 11.40
        val crossTrackM = RouteProgressTracker.haversineM(offLat, offLon, 60.81, 11.30)
        assertTrue("expected multi-km lateral offset, got $crossTrackM", crossTrackM > 4_000.0)

        val off = tracker.update(offLat, offLon)
        assertTrue(off.offRoute)
        assertTrue(off.crossTrackM > RouteProgressTracker.OFF_ROUTE_CROSS_TRACK_MOTOR_M)
        assertEquals(
            "along_m must freeze at last on-route sample (no global jump)",
            1_000.0,
            off.alongM,
            1e-6,
        )
        assertNull("maneuver must be suppressed while off-route", off.maneuver)
        assertTrue(off.distanceToManeuverM.isInfinite())
    }
}
