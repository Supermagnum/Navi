package no.navi.app

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.navi.TravelProfile

class MwmHikingSoftBreaksAndTunnelsTest {
    @Test
    fun hikingAndBicycleUseSoftBreaks() {
        assertTrue(MwmProductionPlanner.usesSoftBreaks(TravelProfile.HIKING))
        assertTrue(MwmProductionPlanner.usesSoftBreaks(TravelProfile.BICYCLE))
        assertTrue(MwmProductionPlanner.usesSoftBreaks(TravelProfile.CAR))
        assertTrue(!MwmProductionPlanner.usesSoftBreaks(TravelProfile.TRUCK))
    }

    @Test
    fun tunnelRunCountCountsContiguousRuns() {
        val turns =
            JSONArray()
                .put(turn(false, 100.0))
                .put(turn(true, 200.0))
                .put(turn(true, 150.0))
                .put(turn(false, 50.0))
                .put(turn(true, 80.0))
        assertEquals(2, MwmProductionPlanner.tunnelRunCount(turns.toString()))
    }

    @Test
    fun tunnelMidpointsProjectOntoPolyline() {
        // Straight south: 0.1° lat ≈ 11.1 km.
        val poly =
            buildString {
                for (i in 0..20) {
                    if (i > 0) append(';')
                    val lat = 60.0 - i * 0.01
                    append("10.0,$lat")
                }
            }
        val turns =
            JSONArray()
                .put(turn(false, 2_000.0))
                .put(turn(true, 1_000.0))
                .put(turn(true, 1_000.0))
                .put(turn(false, 5_000.0))
        val mids = MwmProductionPlanner.tunnelMidpoints(turns.toString(), poly)
        assertEquals(1, mids.size)
        // Mid of tunnel run sits ~3 km along (2 km surface + 1 km into 2 km tunnel).
        assertTrue("mid lat=${mids[0].first}", mids[0].first in 59.96..60.0)
        assertEquals(10.0, mids[0].second, 1e-6)
    }

    @Test
    fun springNameHintDetected() {
        assertTrue(MwmProductionPlanner.comapsHitLooksLikeSpring("natural-spring", "Cold spring"))
        assertTrue(MwmProductionPlanner.comapsHitLooksLikeSpring(null, "Fjellkilde"))
        assertTrue(!MwmProductionPlanner.comapsHitLooksLikeSpring("amenity-cafe", "Cafe"))
    }

    private fun turn(
        tunnel: Boolean,
        distM: Double,
    ): JSONObject =
        JSONObject()
            .put("is_tunnel", tunnel)
            .put("dist_m", distM)
            .put("street", if (tunnel) "Tunnelvegen" else "Surface")
}
