package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test
import uniffi.navi.FfiComapsRoute
import uniffi.navi.FfiComapsRouteCode

/** JVM tests for MWM-vs-report climb selection (A4). */
class RouteClimbTest {
    private val report =
        "profile=Car; use_eco=true\n" +
            "eco_climb_m=2011.0; eco_descent_m=1476.0; eco_uphill_j=1; eco_downhill_j=2; " +
            "eco_regen_credit_j=0; eco_net_j=3\n"

    private val elevJson =
        """{"comaps_ascent_m":10,"comaps_descent_m":5,"ascent_m":12.0,"descent_m":4.0,""" +
            """"sidecar_hits":2,"points":[{"ele_m":100,"source":"sidecar"},null,""" +
            """{"ele_m":112,"source":"comaps"},{"ele_m":108,"source":"comaps"}]}"""

    @Test
    fun fromComaps_prefersShimTotals() {
        val c = RouteClimbSource.fromComaps(12.0, 4.0, elevJson)
        assertNotNull(c)
        assertEquals(12.0, c!!.climbM, 0.0)
        assertEquals(4.0, c.descentM, 0.0)
        assertEquals(RouteClimb.Source.COMAPS_MWM, c.source)
        assertEquals(listOf(100.0, null, 112.0, 108.0), c.profileM)
    }

    @Test
    fun fromComaps_fallsBackToProfileWhenTotalsInvalid() {
        val c = RouteClimbSource.fromComaps(Double.NaN, -1.0, elevJson)
        assertNotNull(c)
        // Only the 112 -> 108 pair has two known neighbours.
        assertEquals(0.0, c!!.climbM, 0.0)
        assertEquals(4.0, c.descentM, 0.0)
    }

    @Test
    fun fromComaps_legacyBareArrayProfile() {
        val c = RouteClimbSource.fromComaps(Double.NaN, Double.NaN, "[10, 20.5, null, 15]")
        assertNotNull(c)
        assertEquals(10.5, c!!.climbM, 1e-9)
        assertEquals(0.0, c.descentM, 0.0)
    }

    @Test
    fun fromComaps_nothingUsable() {
        assertNull(RouteClimbSource.fromComaps(Double.NaN, Double.NaN, "[]"))
        assertNull(RouteClimbSource.fromComaps(Double.NaN, Double.NaN, "not json"))
    }

    @Test
    fun fromReport_readsEcoLines() {
        val c = RouteClimbSource.fromReport(report)
        assertNotNull(c)
        assertEquals(2011.0, c!!.climbM, 0.0)
        assertEquals(1476.0, c.descentM, 0.0)
        assertEquals(RouteClimb.Source.PLAN_REPORT, c.source)
        assertNull(RouteClimbSource.fromReport("FAIL: x"))
    }

    @Test
    fun resolve_mwmOnUsesComapsOtherwiseReport() {
        val comaps = RouteClimb(12.0, 4.0, RouteClimb.Source.COMAPS_MWM)
        assertSame(comaps, RouteClimbSource.resolve(true, comaps, report))

        // Flag OFF: the .rkyv report wins even if a CoMaps climb was published.
        val off = RouteClimbSource.resolve(false, comaps, report)
        assertEquals(RouteClimb.Source.PLAN_REPORT, off!!.source)
        assertEquals(2011.0, off.climbM, 0.0)

        // Flag ON without a CoMaps route: report fallback.
        val noRoute = RouteClimbSource.resolve(true, null, report)
        assertEquals(RouteClimb.Source.PLAN_REPORT, noRoute!!.source)
    }

    @Test
    fun publishedClimbIsTakenOnce() {
        val c = RouteClimb(1.0, 2.0, RouteClimb.Source.COMAPS_MWM)
        RouteClimbSource.publishComaps(c)
        assertSame(c, RouteClimbSource.takeComaps())
        assertNull(RouteClimbSource.takeComaps())
    }

    private fun route(
        code: FfiComapsRouteCode,
        ascentM: Double,
        descentM: Double,
    ) = FfiComapsRoute(
        code = code,
        lengthM = 1000.0,
        timeS = 60.0,
        buildTimeS = 0.1,
        points = emptyList(),
        errorMessage = null,
        turnsJson = "[]",
        instructionsJson = "[]",
        ascentM = ascentM,
        descentM = descentM,
        elevationJson = elevJson,
        routerMode = "Joints",
        speedLimitsJson = "[]",
    )

    @Test
    fun publishIfOk_publishesOkRouteAndClearsOnFailure() {
        val ok = RouteClimbSource.publishIfOk(route(FfiComapsRouteCode.OK, 703.0, 302.0))
        assertNotNull(ok)
        assertEquals(703.0, ok!!.climbM, 0.0)
        val taken = RouteClimbSource.takeComaps()
        assertEquals(RouteClimb.Source.COMAPS_MWM, taken!!.source)
        assertEquals(302.0, taken.descentM, 0.0)

        // A failed route must drop a stale climb from an earlier route.
        RouteClimbSource.publishIfOk(route(FfiComapsRouteCode.OK, 1.0, 1.0))
        assertNull(RouteClimbSource.publishIfOk(route(FfiComapsRouteCode.END_NOT_ON_ROAD, 0.0, 0.0)))
        assertNull(RouteClimbSource.takeComaps())
    }
}
