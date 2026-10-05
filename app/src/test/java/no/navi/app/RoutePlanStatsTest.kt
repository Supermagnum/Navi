package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class RoutePlanStatsTest {
    @Test
    fun parsesZeroCountsForEveryProfileReport() {
        val report =
            """
            TEST_KIND=PLAN_HIKING_ROUTE
            profile=Hiking
            graph_ferry_edges=0
            route_ferry_legs=0
            route_ferry_fp=
            route_tunnel_count=0
            route_tunnel_fp=
            rest_place_count=0
            route_uses_ferry=false
            """.trimIndent()
        val stats = routePlanStatsFromPlan(report, "[]")
        assertEquals(0, stats.tunnelCount)
        assertEquals(0, stats.ferryLegCount)
        assertEquals(0, stats.restPlaceCount)
        assertEquals(0, stats.attractionCount)
        assertEquals(0, stats.wildCampingSiteCount)
        assertTrue(stats.restPlaceNames.isEmpty())
    }

    @Test
    fun parsesCarFerryTunnelAndRestPlaces() {
        val report =
            """
            TEST_KIND=PLAN_CAR_ROUTE
            profile=Car
            route_ferry_legs=2
            route_ferry_fp=Horten-Moss@10.20|Sandefjord-Stromstad@32.00
            route_tunnel_count=3
            route_tunnel_fp=Laerdalstunnelen@24.50
            rest_place_count=4
            """.trimIndent()
        val breaks =
            """[{"name":"Services Minnesund","lat":60.8,"lon":11.2,"kind":"rest_area"},
            {"name":"Dombas rast","lat":62.0,"lon":9.1,"kind":"amenity"}]"""
        val stats = routePlanStatsFromPlan(report, breaks)
        assertEquals(3, stats.tunnelCount)
        assertEquals(2, stats.ferryLegCount)
        assertEquals("Horten-Moss, Sandefjord-Stromstad", formatNamedFpList(stats.ferryFp))
        assertEquals(4, stats.restPlaceCount)
        assertEquals(listOf("Services Minnesund", "Dombas rast"), stats.restPlaceNames)
    }

    @Test
    fun restPlaceCountFallsBackToJsonLength() {
        val stats =
            routePlanStatsFromPlan(
                "profile=Bicycle\nroute_tunnel_count=0\nroute_ferry_legs=0\n",
                """[{"name":"Hut A","lat":61.0,"lon":10.0,"kind":"hut"}]""",
            )
        assertEquals(1, stats.restPlaceCount)
        assertEquals(listOf("Hut A"), stats.restPlaceNames)
    }

    @Test
    fun attractionTallyDedupesOsmIds() {
        val q1 =
            """{"hits":[{"osm_id":1,"label":"Cafe","icon_key":"amenity-cafe","distance_m":40,"open_now":"unknown"}]}"""
        val q2 =
            """{"hits":[{"osm_id":1,"label":"Cafe","icon_key":"amenity-cafe","distance_m":80,"open_now":"unknown"},{"osm_id":2,"label":"Museum","icon_key":"tourism-museum","distance_m":120,"open_now":"unknown"}]}"""
        val (n, byType) = uniqueAttractionTally(listOf(q1, q2))
        assertEquals(2, n)
        assertEquals(1, byType["amenity-cafe"])
        assertEquals(1, byType["tourism-museum"])
    }

    @Test
    fun wildCampingCountIsZeroWithoutPluginResult() {
        assertEquals(0, wildCampingSiteCount(null))
    }

    @Test
    fun wildCampingStatsLineDistinguishesUnavailableFromPluginOff() {
        assertEquals("Wild camping: plugin off", formatWildCampingStatsLine(false, 0, null))
        assertEquals(
            "Wild camping: UNAVAILABLE (corridor graph segments produced no seeds)",
            formatWildCampingStatsLine(
                true,
                0,
                CampingSuggestStatus(
                    kind = "UNAVAILABLE",
                    message = "corridor graph segments produced no seeds",
                ),
            ),
        )
        assertEquals(
            "Wild camping sites: 3",
            formatWildCampingStatsLine(true, 3, CampingSuggestStatus("OK", "")),
        )
    }
}
