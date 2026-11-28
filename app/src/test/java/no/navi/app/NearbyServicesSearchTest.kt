package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.navi.FfiNearbyServiceCategory

class NearbyServicesSearchTest {
    @Test
    fun allCategoriesDefaultToSeven() {
        assertEquals(7, allNearbyServiceCategories().size)
        assertTrue(allNearbyServiceCategories().contains(FfiNearbyServiceCategory.GAS_STATION))
        assertTrue(allNearbyServiceCategories().contains(FfiNearbyServiceCategory.DOCTOR))
    }

    @Test
    fun refreshMoveThresholdIsOneKm() {
        assertEquals(1_000.0, NEARBY_SERVICES_REFRESH_MOVE_M, 0.0)
        assertEquals(150_000.0, NEARBY_SERVICES_RADIUS_M, 0.0)
    }
}
