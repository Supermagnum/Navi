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
}
