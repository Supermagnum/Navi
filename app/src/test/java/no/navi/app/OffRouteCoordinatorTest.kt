package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class OffRouteCoordinatorTest {
    @Test
    fun briefBlip_doesNotConfirm() {
        val c = OffRouteCoordinator(confirmAfterMs = 5_000)
        assertEquals(
            OffRouteCoordinator.Action.ShowOffRouteUi,
            c.onFix(offRoute = true, hiking = false, busy = false, nowMs = 1_000, confirmMs = 5_000),
        )
        assertEquals(
            OffRouteCoordinator.Action.ShowOffRouteUi,
            c.onFix(offRoute = true, hiking = false, busy = false, nowMs = 3_000, confirmMs = 5_000),
        )
        assertEquals(
            OffRouteCoordinator.Action.None,
            c.onFix(offRoute = false, hiking = false, busy = false, nowMs = 3_500, confirmMs = 5_000),
        )
    }

    @Test
    fun sustainedMotor_autoReroutesOnce() {
        val c = OffRouteCoordinator(confirmAfterMs = 5_000)
        c.onFix(true, hiking = false, busy = false, nowMs = 0, confirmMs = 5_000)
        assertEquals(
            OffRouteCoordinator.Action.AutoReroute,
            c.onFix(true, hiking = false, busy = false, nowMs = 5_000, confirmMs = 5_000),
        )
        assertEquals(
            OffRouteCoordinator.Action.ShowOffRouteUi,
            c.onFix(true, hiking = false, busy = false, nowMs = 6_000, confirmMs = 5_000),
        )
    }

    @Test
    fun sustainedHiking_prompts() {
        val c = OffRouteCoordinator()
        c.onFix(true, hiking = true, busy = false, nowMs = 0, confirmMs = 1_000)
        assertEquals(
            OffRouteCoordinator.Action.PromptHikingReroute,
            c.onFix(true, hiking = true, busy = false, nowMs = 1_000, confirmMs = 1_000),
        )
    }

    @Test
    fun declineSuppressesUntilOnRoute() {
        val c = OffRouteCoordinator()
        c.onFix(true, hiking = false, busy = false, nowMs = 0, confirmMs = 100)
        assertEquals(
            OffRouteCoordinator.Action.AutoReroute,
            c.onFix(true, hiking = false, busy = false, nowMs = 100, confirmMs = 100),
        )
        c.suppressUntilOnRoute()
        assertTrue(c.suppressedUntilOnRoute)
        assertEquals(
            OffRouteCoordinator.Action.ShowOffRouteUi,
            c.onFix(true, hiking = false, busy = false, nowMs = 500, confirmMs = 100),
        )
        assertEquals(
            OffRouteCoordinator.Action.None,
            c.onFix(false, hiking = false, busy = false, nowMs = 600, confirmMs = 100),
        )
        assertEquals(false, c.suppressedUntilOnRoute)
    }
}
