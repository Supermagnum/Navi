package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.navi.TravelProfile

class VehicleRestrictionUiTest {
    @Test
    fun appliesStoredLimitsForCarTruckMobileHome() {
        for (p in listOf(
            TravelProfile.CAR,
            TravelProfile.CAR_ELECTRIC,
            TravelProfile.TRUCK,
            TravelProfile.TRUCK_ELECTRIC,
            TravelProfile.MOBILE_HOME,
        )) {
            assertTrue("$p", VehicleRestrictionUi.appliesStoredLimits(p))
        }
        assertFalse(VehicleRestrictionUi.appliesStoredLimits(TravelProfile.MOTORCYCLE))
        assertFalse(VehicleRestrictionUi.appliesStoredLimits(TravelProfile.HIKING))
    }

    @Test
    fun noWarnWhenDataAvailable() {
        for (p in listOf(TravelProfile.TRUCK, TravelProfile.TRUCK_ELECTRIC, TravelProfile.MOBILE_HOME)) {
            assertFalse("$p", VehicleRestrictionUi.warnOnPlannedRoute(p, null, dataAvailable = true))
            assertFalse("$p", VehicleRestrictionUi.warnOnPlannedRoute(p, 4.0, dataAvailable = true))
        }
        assertFalse(VehicleRestrictionUi.warnOnPlannedRoute(TravelProfile.CAR, 2.0, dataAvailable = true))
    }

    @Test
    fun warnWhenDataMissingForTruckAndCarWithHeight() {
        for (p in listOf(TravelProfile.TRUCK, TravelProfile.TRUCK_ELECTRIC, TravelProfile.MOBILE_HOME)) {
            assertTrue("$p", VehicleRestrictionUi.warnOnPlannedRoute(p, null, dataAvailable = false))
        }
        assertFalse(VehicleRestrictionUi.warnOnPlannedRoute(TravelProfile.CAR, null, dataAvailable = false))
        assertTrue(VehicleRestrictionUi.warnOnPlannedRoute(TravelProfile.CAR, 2.0, dataAvailable = false))
        assertFalse(VehicleRestrictionUi.warnOnPlannedRoute(TravelProfile.MOTORCYCLE, 1.5, dataAvailable = false))
    }

    @Test
    fun profilePanelStillCoversMotorcycleViaLimitsNotApplied() {
        assertTrue(VehicleRestrictionUi.showProfilePanelNote(TravelProfile.MOTORCYCLE, null))
        assertFalse(VehicleRestrictionUi.showProfilePanelNote(TravelProfile.CAR, 2.5, dataAvailable = true))
        assertTrue(VehicleRestrictionUi.showProfilePanelNote(TravelProfile.CAR, 2.5, dataAvailable = false))
        assertFalse(VehicleRestrictionUi.showProfilePanelNote(TravelProfile.HIKING, null, dataAvailable = true))
    }

    @Test
    fun appendPlanWarningRespectsDataAvailable() {
        val warn = "Caution: limits not available"
        assertEquals(
            "ok",
            VehicleRestrictionUi.appendPlanWarning("ok", warn, TravelProfile.TRUCK, null, dataAvailable = true),
        )
        assertEquals(
            warn,
            VehicleRestrictionUi.appendPlanWarning("", warn, TravelProfile.TRUCK, null, dataAvailable = false),
        )
    }

    @Test
    fun warnDataUnavailableFormatsRegionList() {
        assertNull(VehicleRestrictionUi.warnDataUnavailable(emptyList(), "Missing: %s"))
        assertEquals(
            "Missing: Norway_Hedmark, Norway_Oppland",
            VehicleRestrictionUi.warnDataUnavailable(
                listOf("Norway_Hedmark", "Norway_Oppland"),
                "Missing: %s",
            ),
        )
    }

    @Test
    fun totalWeightUiProfilesMatchClearanceProfiles() {
        for (p in TravelProfile.entries) {
            val showTotalWeight =
                p == TravelProfile.TRUCK ||
                    p == TravelProfile.TRUCK_ELECTRIC ||
                    p == TravelProfile.MOBILE_HOME
            assertEquals("$p", showTotalWeight, VehicleRestrictionUi.alwaysWarnOnMwm(p))
        }
    }
}
