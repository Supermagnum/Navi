package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.approachHideM
import uniffi.navi.approachPhaseForDistance

/**
 * Approach hide distance is metres from Rust `APPROACH_HIDE_M` via UniFFI
 * `approachHideM` (default 25). Tracker must use that constant — not a local
 * magic number — so hide timing stays locked with phase styling.
 */
@RunWith(AndroidJUnit4::class)
class ApproachHideDistanceInstrumentedTest {

    @Test
    fun approachHideM_defaultsTo25Metres() {
        assertEquals(25.0, approachHideM(), 0.0)
        assertEquals("hidden", approachPhaseForDistance(true, approachHideM()))
        assertEquals("urgency", approachPhaseForDistance(true, approachHideM() + 1.0))
    }

    @Test
    fun trackerAdvancesManeuverAtHideDistance() {
        val hide = approachHideM()
        assertEquals(25.0, hide, 0.0)

        val samples = listOf(
            RouteSimSample(60.0, 11.0, 0.0, 50.0, null, false),
            RouteSimSample(60.001, 11.0, 100.0, 50.0, null, false),
        )
        val maneuvers = listOf(
            RouteManeuver(
                lat = 60.001,
                lon = 11.0,
                cumM = 100.0,
                kind = "right",
                street = "Testvegen",
                roundaboutExit = null,
            ),
        )
        val end = Waypoint(name = "End", lat = 60.001, lon = 11.0)
        val tracker = RouteProgressTracker(
            samples = samples,
            maneuvers = maneuvers,
            viaPoints = emptyList(),
            endPoint = end,
            hideDistanceM = hide,
        )

        val before = tracker.update(60.0, 11.0)
        assertEquals(0, before.maneuverIndex)
        assertNotNull(before.maneuver)
        assertEquals(100.0, before.distanceToManeuverM, 0.01)

        tracker.reset()
        val after = tracker.update(60.001, 11.0)
        assertNull(after.maneuver)
        assertEquals(1, after.maneuverIndex)
    }
}
