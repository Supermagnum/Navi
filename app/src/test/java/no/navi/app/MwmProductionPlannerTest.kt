package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.navi.FfiComapsLatLon
import uniffi.navi.FfiComapsRoute
import uniffi.navi.FfiComapsRouteCode
import uniffi.navi.TravelProfile

class MwmProductionPlannerTest {
    private fun route(
        lengthM: Double,
        timeS: Double,
        points: Int = 5,
    ) = FfiComapsRoute(
        code = FfiComapsRouteCode.OK,
        lengthM = lengthM,
        timeS = timeS,
        buildTimeS = 1.0,
        points = (0 until points).map { FfiComapsLatLon(60.0 + it * 0.001, 11.0 + it * 0.001) },
        errorMessage = null,
        turnsJson = "[]",
        instructionsJson = "[]",
        ascentM = 10.0,
        descentM = 5.0,
        elevationJson = "[]",
        routerMode = "Joints",
        speedLimitsJson = "[]",
    )

    @Test
    fun simSamplesParseAndCarryRouteAveragePace() {
        // 36 km in 30 min = 72 km/h. A null speed here used to make parseRouteSimSamples
        // drop the whole route, so route simulation and progress tracking never started.
        val result = MwmProductionPlanner.toCorridor(route(36_000.0, 1_800.0), 60.004, 11.004, TravelProfile.CAR)
        val samples = parseRouteSimSamples(result.simSamplesJson)
        assertEquals(5, samples.size)
        assertEquals(72.0, samples.first().speedKmh, 0.01)
        assertTrue(samples.all { it.maxspeedKmh == null })
    }

    @Test
    fun postedLimitsFromTheMwmReachTheSamples() {
        // 5 points = 4 segments: 110, 110, untagged, 30. A sample is the lowest limit of the segments
        // it covers; -1 (untagged) and -2 (unlimited) carry no posted limit.
        val r = route(36_000.0, 1_800.0).copy(speedLimitsJson = "[110,110,-1,30]")
        val samples = parseRouteSimSamples(MwmProductionPlanner.toCorridor(r, 60.004, 11.004, TravelProfile.CAR).simSamplesJson)
        assertEquals(listOf(110.0, 110.0, null, 30.0, 30.0), samples.map { it.maxspeedKmh })
        assertEquals(listOf(true, true, false, true, true), samples.map { it.maxspeedPosted })
        val noLimits = route(36_000.0, 1_800.0).copy(speedLimitsJson = "[-1,-2,-1,-1]")
        val none = parseRouteSimSamples(MwmProductionPlanner.toCorridor(noLimits, 60.004, 11.004, TravelProfile.HIKING).simSamplesJson)
        assertTrue(none.none { it.maxspeedPosted })
    }

    @Test
    fun turnArrowsFollowTheManeuverPolylineIndices() {
        val instr =
            """[{"dist_m":111.0,"kind":"left"},{"dist_m":111.0,"kind":"right"},{"dist_m":10.0,"kind":"destination"}]"""
        val r = route(300.0, 60.0, points = 5).copy(instructionsJson = instr)
        val result = MwmProductionPlanner.toCorridor(r, 60.004, 11.004, TravelProfile.CAR)
        val idx = MwmProductionPlanner.lastTurnPointIndices(result.routePolyline)
        assertTrue("turn indices for the same polyline", idx != null && idx.isNotEmpty())
        assertTrue("destination has no arrow", idx!!.all { it in 1..3 })
        assertEquals(null, MwmProductionPlanner.lastTurnPointIndices("1,2;3,4"))
    }

    @Test
    fun vehicleLimitsAreNotAppliedForTheCarLikeProfiles() {
        for (p in listOf(TravelProfile.TRUCK, TravelProfile.MOBILE_HOME, TravelProfile.MOTORCYCLE, TravelProfile.CAR_ELECTRIC)) {
            assertTrue("$p", MwmProductionPlanner.limitsNotApplied(p))
        }
        for (p in listOf(TravelProfile.CAR, TravelProfile.BICYCLE, TravelProfile.HIKING)) {
            assertTrue("$p", !MwmProductionPlanner.limitsNotApplied(p))
        }
    }

    @Test
    fun simPaceIsClampedAndHasFallback() {
        val slow = parseRouteSimSamples(MwmProductionPlanner.toCorridor(route(1_000.0, 3_600.0), 0.0, 0.0, TravelProfile.CAR).simSamplesJson)
        assertEquals(15.0, slow.first().speedKmh, 0.01)
        val noTime = route(1_000.0, 0.0).copy(timeS = 0.0)
        val fallback = parseRouteSimSamples(MwmProductionPlanner.toCorridor(noTime, 0.0, 0.0, TravelProfile.CAR).simSamplesJson)
        assertEquals(60.0, fallback.first().speedKmh, 0.01)
    }

    @Test
    fun failedRouteReportsComapsCode() {
        val failed = route(0.0, 0.0).copy(code = FfiComapsRouteCode.NEED_MORE_MAPS, errorMessage = "Missing or outdated maps")
        val result = MwmProductionPlanner.toCorridor(failed, 0.0, 0.0, TravelProfile.CAR)
        assertTrue(result.report.contains("comaps_route_code=NEED_MORE_MAPS"))
        assertTrue(result.report.contains("FAIL: Missing or outdated maps"))
        assertEquals("", result.routePolyline)
    }

    @Test
    fun planReportIncludesFerryAndTunnelTokensFromTurns() {
        val turns =
            """
            [
              {"index":1,"sequence":1,"dist_m":1000.0,"is_ferry":false,"is_tunnel":true,"street":"Follotunnelen","ref":"E 6","destination":""},
              {"index":2,"sequence":2,"dist_m":18900.0,"is_ferry":true,"is_tunnel":false,"street":"Rødby (DK) - Puttgarden (D)","ref":"","destination":"Puttgarden"},
              {"index":3,"sequence":3,"dist_m":500.0,"is_ferry":false,"is_tunnel":false,"street":"E47","ref":"E 47","destination":""}
            ]
            """.trimIndent()
        val instr =
            """
            [
              {"dist_m":1000.0,"kind":"straight","street":"Follotunnelen","is_ferry":false},
              {"dist_m":18900.0,"kind":"straight","street":"Rødby (DK) - Puttgarden (D)","is_ferry":true},
              {"dist_m":500.0,"kind":"destination","street":"E47","is_ferry":false}
            ]
            """.trimIndent()
        val result =
            MwmProductionPlanner.toCorridor(
                route(20_400.0, 1_200.0).copy(turnsJson = turns, instructionsJson = instr),
                60.004,
                11.004,
                TravelProfile.CAR,
            )
        assertTrue(result.report.contains("PASS"))
        assertTrue(result.report.contains("route_tunnel_count=1"))
        assertTrue(result.report.contains("route_tunnel_fp=Follotunnelen@1.00"))
        assertTrue(result.report.contains("route_ferry_legs=1"))
        assertTrue(result.report.contains("route_ferry_fp=Rødby (DK) - Puttgarden (D)@18.90"))
        assertTrue(result.report.contains("route_uses_ferry=true"))
        val stats = routePlanStatsFromPlan(result.report, "[]")
        assertEquals(1, stats.tunnelCount)
        assertEquals(1, stats.ferryLegCount)
        assertTrue(result.maneuversJson.contains("\"is_tunnel\":true"))
        assertTrue(result.maneuversJson.contains("\"is_ferry\":true"))
    }

    @Test
    fun planReportEmitsZeroFerryTunnelTokensWhenTurnsHaveNone() {
        val result = MwmProductionPlanner.toCorridor(route(1_000.0, 60.0), 60.004, 11.004, TravelProfile.CAR)
        assertTrue(result.report.contains("route_tunnel_count=0"))
        assertTrue(result.report.contains("route_ferry_legs=0"))
        assertTrue(result.report.contains("route_uses_ferry=false"))
        val stats = routePlanStatsFromPlan(result.report, "[]")
        assertEquals(0, stats.tunnelCount)
        assertEquals(0, stats.ferryLegCount)
    }
}
