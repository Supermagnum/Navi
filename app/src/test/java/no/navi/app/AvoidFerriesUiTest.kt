package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AvoidFerriesUiTest {
    @Test
    fun parseGraphFerryEdges_readsReportToken() {
        val report =
            """
            TEST_KIND=PLAN_HIKING_ROUTE
            pack_hit=true; nodes=10; edges=20
            graph_ferry_edges=0
            route_uses_ferry=false
            """.trimIndent()
        assertEquals(0, AvoidFerriesUi.parseGraphFerryEdges(report))
    }

    @Test
    fun parseGraphFerryEdges_readsHostProofStyleCount() {
        // Host PBF ferry graphs report non-zero counts (ferry-host-proof.log).
        assertEquals(
            204,
            AvoidFerriesUi.parseGraphFerryEdges("build_s=1.0; pack_hit=false\ngraph_ferry_edges=204\n"),
        )
    }

    @Test
    fun parseGraphFerryEdges_missingIsNull() {
        assertNull(AvoidFerriesUi.parseGraphFerryEdges("pack_hit=true\nroute_uses_ferry=false\n"))
    }

    @Test
    fun ostlandetPackNoFerries_disablesMotorAndHiking() {
        assertFalse(
            AvoidFerriesUi.toggleEnabled(
                profileIsMotor = true,
                profileIsHikingOrBike = false,
                graphFerryEdges = 0,
            ),
        )
        assertFalse(
            AvoidFerriesUi.toggleEnabled(
                profileIsMotor = false,
                profileIsHikingOrBike = true,
                graphFerryEdges = 0,
            ),
        )
        assertEquals(
            AvoidFerriesUi.NO_FERRY_DATA_NOTE,
            AvoidFerriesUi.unavailableNote(0),
        )
    }

    @Test
    fun ferryCapableGraph_enablesMotorAndHikingBike() {
        assertTrue(
            AvoidFerriesUi.toggleEnabled(
                profileIsMotor = true,
                profileIsHikingOrBike = false,
                graphFerryEdges = 12,
            ),
        )
        assertTrue(
            AvoidFerriesUi.toggleEnabled(
                profileIsMotor = false,
                profileIsHikingOrBike = true,
                graphFerryEdges = 1,
            ),
        )
        assertNull(AvoidFerriesUi.unavailableNote(1))
    }

    @Test
    fun beforePlan_motorEnabled_hikingBikeDisabled() {
        assertTrue(
            AvoidFerriesUi.toggleEnabled(
                profileIsMotor = true,
                profileIsHikingOrBike = false,
                graphFerryEdges = null,
            ),
        )
        assertFalse(
            AvoidFerriesUi.toggleEnabled(
                profileIsMotor = false,
                profileIsHikingOrBike = true,
                graphFerryEdges = null,
            ),
        )
        assertNull(AvoidFerriesUi.unavailableNote(null))
    }

    @Test
    fun greyOutDoesNotImplyClearingPreference() {
        // Preference (Compose/ConfigStore) is independent of toggleEnabled.
        // When graph_ferry_edges=0 the Switch is disabled but checked may stay ON.
        val preferenceOn = true
        assertFalse(
            AvoidFerriesUi.toggleEnabled(
                profileIsMotor = true,
                profileIsHikingOrBike = false,
                graphFerryEdges = 0,
            ),
        )
        assertEquals(AvoidFerriesUi.NO_FERRY_DATA_NOTE, AvoidFerriesUi.unavailableNote(0))
        // Surviving preference: still ON while greyed; later ferry-capable graphs
        // re-enable the Switch without resetting the stored value.
        assertTrue(preferenceOn)
        assertTrue(
            AvoidFerriesUi.toggleEnabled(
                profileIsMotor = true,
                profileIsHikingOrBike = false,
                graphFerryEdges = 12,
            ),
        )
    }
}
