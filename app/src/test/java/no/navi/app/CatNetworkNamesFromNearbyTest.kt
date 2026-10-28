package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CatNetworkNamesFromNearbyTest {
    @Test
    fun skipsJsonNullAndLiteralNullNetworkId() {
        val json =
            """
            [
              {"network_id":null,"distance_km":10.0,"modulation":"FM","position_accurate":true},
              {"network_id":"null","distance_km":12.0,"modulation":"FM","position_accurate":true},
              {"network_id":"NULL","distance_km":13.0,"modulation":"FM","position_accurate":true},
              {"network_id":"","distance_km":14.0,"modulation":"FM","position_accurate":true},
              {"network_id":"LA5MR","distance_km":37.0,"modulation":"FM","position_accurate":true},
              {"network_id":"LA5MR","distance_km":40.0,"modulation":"FM","position_accurate":true},
              {"network_id":"LA5MR","distance_km":20.0,"modulation":"DMR","position_accurate":true}
            ]
            """.trimIndent()
        val chips = networkNamesFromNearby(json)
        assertEquals(listOf("LA5MR"), chips.map { it.name })
        assertEquals(2, chips.single().members)
        assertEquals(37.0, chips.single().nearestKm, 0.001)
    }

    @Test
    fun emptyOrInvalidJsonYieldsNoChips() {
        assertTrue(networkNamesFromNearby("").isEmpty())
        assertTrue(networkNamesFromNearby("[]").isEmpty())
        assertTrue(networkNamesFromNearby("not-json").isEmpty())
    }
}
