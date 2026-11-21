package no.navi.app

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NonMotorTunnelAccessTest {
    private fun tunnel(
        highway: String? = "primary",
        foot: String? = null,
        bicycle: String? = null,
        sidewalk: String? = null,
        footway: String? = null,
        hasYesFoot: Boolean = false,
        hasYesBicycle: Boolean = false,
        hasSidewalkType: Boolean = false,
    ) = NonMotorTunnelTags(
        tunnel = "yes",
        highway = highway,
        foot = foot,
        bicycle = bicycle,
        sidewalk = sidewalk,
        footway = footway,
        hasYesFoot = hasYesFoot,
        hasYesBicycle = hasYesBicycle,
        hasSidewalkType = hasSidewalkType,
    )

    @Test
    fun hikingAllowsFootYesOrDesignated() {
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.HIKING, tunnel(foot = "yes")))
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.HIKING, tunnel(foot = "designated")))
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.HIKING, tunnel(hasYesFoot = true)))
        assertEquals(1.0, nonMotorTunnelCostMultiplier(NonMotorTunnelProfile.HIKING, tunnel(foot = "yes")), 0.0)
    }

    @Test
    fun hikingAllowsSidewalkAndPathHighways() {
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.HIKING, tunnel(sidewalk = "both")))
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.HIKING, tunnel(sidewalk = "left")))
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.HIKING, tunnel(footway = "sidewalk")))
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.HIKING, tunnel(hasSidewalkType = true)))
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.HIKING, tunnel(highway = "footway")))
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.HIKING, tunnel(highway = "cycleway")))
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.HIKING, tunnel(highway = "path")))
    }

    @Test
    fun hikingDeniesCarOnlyTunnel() {
        val tags = tunnel(highway = "trunk")
        assertFalse(nonMotorTunnelAllowed(NonMotorTunnelProfile.HIKING, tags))
        assertEquals(
            NON_MOTOR_TUNNEL_HARD_AVOID_MULT,
            nonMotorTunnelCostMultiplier(NonMotorTunnelProfile.HIKING, tags),
            0.0,
        )
        // bicycle=yes alone is not enough for hiking
        assertFalse(nonMotorTunnelAllowed(NonMotorTunnelProfile.HIKING, tunnel(bicycle = "yes")))
    }

    @Test
    fun bicycleAllowsBicycleYesOrDesignated() {
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.BICYCLE, tunnel(bicycle = "yes")))
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.BICYCLE, tunnel(bicycle = "designated")))
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.BICYCLE, tunnel(hasYesBicycle = true)))
        assertEquals(1.0, nonMotorTunnelCostMultiplier(NonMotorTunnelProfile.BICYCLE, tunnel(bicycle = "yes")), 0.0)
    }

    @Test
    fun bicycleAllowsSidewalkAndPathHighways() {
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.BICYCLE, tunnel(sidewalk = "right")))
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.BICYCLE, tunnel(hasSidewalkType = true)))
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.BICYCLE, tunnel(highway = "footway")))
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.BICYCLE, tunnel(highway = "cycleway")))
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.BICYCLE, tunnel(highway = "path")))
    }

    @Test
    fun bicycleDeniesFootOnlyOrCarOnlyTunnel() {
        assertFalse(nonMotorTunnelAllowed(NonMotorTunnelProfile.BICYCLE, tunnel(foot = "yes")))
        assertFalse(nonMotorTunnelAllowed(NonMotorTunnelProfile.BICYCLE, tunnel(hasYesFoot = true)))
        val tags = tunnel(highway = "secondary")
        assertFalse(nonMotorTunnelAllowed(NonMotorTunnelProfile.BICYCLE, tags))
        assertEquals(
            NON_MOTOR_TUNNEL_HARD_AVOID_MULT,
            nonMotorTunnelCostMultiplier(NonMotorTunnelProfile.BICYCLE, tags),
            0.0,
        )
    }

    @Test
    fun nonTunnelIsUnrestricted() {
        val surface =
            NonMotorTunnelTags(tunnel = "no", highway = "primary")
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.HIKING, surface))
        assertTrue(nonMotorTunnelAllowed(NonMotorTunnelProfile.BICYCLE, surface))
        assertEquals(1.0, nonMotorTunnelCostMultiplier(NonMotorTunnelProfile.HIKING, surface), 0.0)
        assertEquals(
            1.0,
            nonMotorTunnelCostMultiplier(
                NonMotorTunnelProfile.BICYCLE,
                NonMotorTunnelTags(highway = "motorway"),
            ),
            0.0,
        )
    }

    @Test
    fun forbiddenTunnelTurnRespectsTunnelOk() {
        val forbidden = JSONObject().put("is_tunnel", true)
        assertTrue(isForbiddenTunnelTurn(forbidden))
        val allowed = JSONObject().put("is_tunnel", true).put("tunnel_ok", true)
        assertFalse(isForbiddenTunnelTurn(allowed))
        val surface = JSONObject().put("is_tunnel", false)
        assertFalse(isForbiddenTunnelTurn(surface))
    }
}
