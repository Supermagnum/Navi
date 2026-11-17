package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.navi.TravelProfile

class VehicleRestrictionUiTest {
    @Test
    fun truckMobileHomeAndElectricTruckAlwaysWarn() {
        for (p in listOf(TravelProfile.TRUCK, TravelProfile.TRUCK_ELECTRIC, TravelProfile.MOBILE_HOME)) {
            assertTrue("$p", VehicleRestrictionUi.alwaysWarnOnMwm(p))
            assertTrue("$p", VehicleRestrictionUi.warnOnPlannedRoute(p, null))
            assertTrue("$p", VehicleRestrictionUi.warnOnPlannedRoute(p, 4.0))
        }
    }

    @Test
    fun carWarnsOnlyWhenHeightIsStored() {
        assertFalse(VehicleRestrictionUi.warnOnPlannedRoute(TravelProfile.CAR, null))
        assertTrue(VehicleRestrictionUi.warnOnPlannedRoute(TravelProfile.CAR, 2.0))
        assertFalse(VehicleRestrictionUi.warnOnPlannedRoute(TravelProfile.CAR_ELECTRIC, null))
        assertTrue(VehicleRestrictionUi.warnOnPlannedRoute(TravelProfile.CAR_ELECTRIC, 1.9))
    }

    @Test
    fun motorcycleDoesNotWarnOnPlannedRoute() {
        assertFalse(VehicleRestrictionUi.warnOnPlannedRoute(TravelProfile.MOTORCYCLE, null))
        assertFalse(VehicleRestrictionUi.warnOnPlannedRoute(TravelProfile.MOTORCYCLE, 1.5))
        assertFalse(VehicleRestrictionUi.warnOnPlannedRoute(TravelProfile.BICYCLE, null))
        assertFalse(VehicleRestrictionUi.warnOnPlannedRoute(TravelProfile.HIKING, null))
    }

    @Test
    fun profilePanelStillCoversMotorcycleViaLimitsNotApplied() {
        assertTrue(
            VehicleRestrictionUi.showProfilePanelNote(TravelProfile.MOTORCYCLE, null),
        )
        assertTrue(
            VehicleRestrictionUi.showProfilePanelNote(TravelProfile.CAR, 2.5),
        )
        assertFalse(
            VehicleRestrictionUi.showProfilePanelNote(TravelProfile.CAR, null),
        )
        assertFalse(
            VehicleRestrictionUi.showProfilePanelNote(TravelProfile.HIKING, null),
        )
    }

    @Test
    fun appendPlanWarningIsIdempotentAndSkipsWhenNotNeeded() {
        val warn = "Caution: limits not yet applied"
        assertEquals("ok", VehicleRestrictionUi.appendPlanWarning("ok", warn, TravelProfile.CAR, null))
        assertEquals(warn, VehicleRestrictionUi.appendPlanWarning("", warn, TravelProfile.TRUCK, null))
        assertEquals(
            "12.3 km\n$warn",
            VehicleRestrictionUi.appendPlanWarning("12.3 km", warn, TravelProfile.MOBILE_HOME, null),
        )
        assertEquals(
            "12.3 km\n$warn",
            VehicleRestrictionUi.appendPlanWarning("12.3 km\n$warn", warn, TravelProfile.TRUCK, null),
        )
        assertEquals(
            "car route\n$warn",
            VehicleRestrictionUi.appendPlanWarning("car route", warn, TravelProfile.CAR, 2.8),
        )
    }

    @Test
    fun totalWeightUiProfilesMatchAlwaysWarnProfiles() {
        // Vehicle panel shows total weight for the same clearance profiles that
        // always get the temporary MWM warning (truck / electric truck / mobile home).
        for (p in TravelProfile.entries) {
            val showTotalWeight =
                p == TravelProfile.TRUCK ||
                    p == TravelProfile.TRUCK_ELECTRIC ||
                    p == TravelProfile.MOBILE_HOME
            assertEquals("$p", showTotalWeight, VehicleRestrictionUi.alwaysWarnOnMwm(p))
        }
    }
}
