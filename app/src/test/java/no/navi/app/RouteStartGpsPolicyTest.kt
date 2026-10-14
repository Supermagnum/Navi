package no.navi.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RouteStartGpsPolicyTest {
    private val thresholdM = 1_000.0

    @Test
    fun typedStart_1km_away_neverAutoReplansFromGps() {
        assertFalse(RouteStartGpsPolicy.shouldAutoReplanFromGps(fromFollowsGps = false))
        assertTrue(
            RouteStartGpsPolicy.routeNotJoinedForAutoReroute(
                fromFollowsGps = false,
                hasGpsFix = true,
                startToGpsM = 1_000.0,
                awayThresholdM = thresholdM,
            ),
        )
    }

    @Test
    fun typedStart_50m_kept_sticky() {
        assertFalse(RouteStartGpsPolicy.shouldAutoReplanFromGps(fromFollowsGps = false))
        assertTrue(
            RouteStartGpsPolicy.routeNotJoinedForAutoReroute(
                fromFollowsGps = false,
                hasGpsFix = true,
                startToGpsM = 50.0,
                awayThresholdM = thresholdM,
            ),
        )
    }

    @Test
    fun myPosition_followsGps_whenWithinThreshold() {
        assertTrue(RouteStartGpsPolicy.shouldAutoReplanFromGps(fromFollowsGps = true))
        assertFalse(
            RouteStartGpsPolicy.routeNotJoinedForAutoReroute(
                fromFollowsGps = true,
                hasGpsFix = true,
                startToGpsM = 50.0,
                awayThresholdM = thresholdM,
            ),
        )
    }

    @Test
    fun myPosition_suppressesUntilJoined_whenFar() {
        assertTrue(
            RouteStartGpsPolicy.routeNotJoinedForAutoReroute(
                fromFollowsGps = true,
                hasGpsFix = true,
                startToGpsM = 1_001.0,
                awayThresholdM = thresholdM,
            ),
        )
    }

    @Test
    fun emptyFrom_followsGps() {
        // Empty / unset From is treated as GPS-follow origin by the UI flag.
        assertTrue(RouteStartGpsPolicy.shouldAutoReplanFromGps(fromFollowsGps = true))
        assertFalse(
            RouteStartGpsPolicy.routeNotJoinedForAutoReroute(
                fromFollowsGps = true,
                hasGpsFix = true,
                startToGpsM = 0.0,
                awayThresholdM = thresholdM,
            ),
        )
    }
}
