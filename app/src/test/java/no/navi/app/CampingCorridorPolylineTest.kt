package no.navi.app

import org.json.JSONArray
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CampingCorridorPolylineTest {
    @Test
    fun naviOverlayLonLatBecomesNativeLatLon() {
        // Elsa origin: overlay `"lon,lat"` must not be treated as lat=29.63
        // (that lands camping corridor graph loads in asia/pakistan).
        val poly = "29.6337571,69.9741435;29.35743,69.79367"
        val sampled = sampleCampingCorridorWaypoints(poly, maxPoints = 12, targetSpacingKm = 2.5)
        assertEquals(2, sampled.size)
        assertEquals(69.9741435, sampled[0][0], 1e-9)
        assertEquals(29.6337571, sampled[0][1], 1e-9)
        assertEquals(69.79367, sampled[1][0], 1e-9)
        assertEquals(29.35743, sampled[1][1], 1e-9)

        val json = JSONArray(campingWaypointsJson(sampled))
        val first = json.getJSONArray(0)
        assertEquals(69.9741435, first.getDouble(0), 1e-9)
        assertEquals(29.6337571, first.getDouble(1), 1e-9)
        assertTrue(
            "native lat must stay in Norway, not Pakistan",
            first.getDouble(0) > 60.0,
        )
    }

    @Test
    fun longCorridorSamplesReachDestination() {
        // ~2500 km at 2.5 km overlay steps would exceed maxPoints=240 if spacing
        // stayed 2.5 km; adaptive spacing must still keep destination Norway.
        val sb = StringBuilder()
        var lat = 53.08
        var lon = 10.59
        sb.append(String.format(java.util.Locale.US, "%.5f,%.5f", lon, lat))
        repeat(900) {
            lat += 0.01
            lon -= 0.003
            sb.append(String.format(java.util.Locale.US, ";%.5f,%.5f", lon, lat))
        }
        val sampled = sampleCampingCorridorWaypoints(sb.toString(), maxPoints = 240, targetSpacingKm = 2.5)
        assertTrue(sampled.size in 2..240)
        assertEquals(53.08, sampled.first()[0], 0.02)
        assertTrue(
            "last sample must stay near the north end, got ${sampled.last()[0]}",
            sampled.last()[0] > 60.0,
        )
    }
}
