package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MwmRouteLegStatsTest {
    private val sampleTurns =
        """
        [
          {"index":1,"sequence":1,"dist_m":500.0,"is_ferry":false,"is_tunnel":false,"street":"E6","ref":"E 6","destination":""},
          {"index":2,"sequence":2,"dist_m":24500.0,"is_ferry":false,"is_tunnel":true,"street":"Laerdalstunnelen","ref":"E 16","destination":""},
          {"index":3,"sequence":3,"dist_m":200.0,"is_ferry":false,"is_tunnel":true,"street":"Laerdalstunnelen","ref":"E 16","destination":""},
          {"index":4,"sequence":4,"dist_m":1000.0,"is_ferry":false,"is_tunnel":false,"street":"E16","ref":"E 16","destination":""},
          {"index":5,"sequence":5,"dist_m":18900.0,"is_ferry":true,"is_tunnel":false,"street":"Rødby (DK) - Puttgarden (D)","ref":"","destination":"Puttgarden"},
          {"index":6,"sequence":6,"dist_m":300.0,"is_ferry":false,"is_tunnel":false,"street":"E47","ref":"E 47","destination":""},
          {"index":7,"sequence":7,"dist_m":800.0,"is_ferry":false,"is_tunnel":true,"street":"","ref":"E 6","destination":""}
        ]
        """.trimIndent()

    @Test
    fun mergesContiguousTunnelRunsAndSplitsOnGap() {
        val tunnels = MwmRouteLegStats.tunnelLegsFromTurns(sampleTurns)
        assertEquals(2, tunnels.size)
        assertEquals("Laerdalstunnelen", tunnels[0].name)
        assertEquals(24_700.0, tunnels[0].lengthM, 0.01)
        assertEquals("E 6", tunnels[1].name)
        assertEquals(800.0, tunnels[1].lengthM, 0.01)
    }

    @Test
    fun ferryLegsUseStreetOrDestination() {
        val ferries = MwmRouteLegStats.ferryLegsFromTurns(sampleTurns)
        assertEquals(1, ferries.size)
        assertEquals("Rødby (DK) - Puttgarden (D)", ferries[0].name)
        assertEquals(18_900.0, ferries[0].lengthM, 0.01)
    }

    @Test
    fun fingerprintMatchesPbfStyleTokens() {
        val fp =
            MwmRouteLegStats.formatNamedFp(
                listOf(
                    MwmRouteLegStats.NamedLeg("Laerdalstunnelen", 24_500.0),
                    MwmRouteLegStats.NamedLeg("unnamed", 800.0),
                ),
            )
        assertEquals("Laerdalstunnelen@24.50|unnamed@0.80", fp)
    }

    @Test
    fun emptyTurnsYieldZeroTokens() {
        val sb = StringBuilder()
        MwmRouteLegStats.appendReportTokens(sb, "[]")
        val report = sb.toString()
        assertTrue(report.contains("route_ferry_legs=0"))
        assertTrue(report.contains("route_ferry_fp=\n") || report.contains("route_ferry_fp=\r\n"))
        assertTrue(report.contains("route_uses_ferry=false"))
        assertTrue(report.contains("route_tunnel_count=0"))
        assertTrue(report.contains("route_tunnel_fp=\n") || report.contains("route_tunnel_fp=\r\n"))
    }

    @Test
    fun appendReportTokensPopulateRoutePlanStats() {
        val sb = StringBuilder()
        MwmRouteLegStats.appendReportTokens(sb, sampleTurns)
        val stats = routePlanStatsFromPlan(sb.toString(), "[]")
        assertEquals(2, stats.tunnelCount)
        assertTrue(stats.tunnelFp.contains("Laerdalstunnelen@24.70"))
        assertEquals(1, stats.ferryLegCount)
        assertTrue(stats.ferryFp.contains("Rødby (DK) - Puttgarden (D)@18.90"))
        assertFalse(stats.ferryFp.isEmpty())
    }
}
