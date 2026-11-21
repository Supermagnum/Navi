package no.navi.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.navi.TravelProfile

class AvoidTunnelsUiTest {
    @Test
    fun hikingBikeEbikeHideUserToggle() {
        for (p in listOf(
            TravelProfile.HIKING,
            TravelProfile.BICYCLE,
            TravelProfile.BICYCLE_ELECTRIC,
        )) {
            assertTrue("$p", AvoidTunnelsUi.isAutoSafeProfile(p))
            assertFalse("$p", AvoidTunnelsUi.showsUserToggle(p))
            assertFalse("$p", AvoidTunnelsUi.acceptsPersistedPreference(p))
        }
    }

    @Test
    fun motorProfilesKeepUserToggle() {
        for (p in listOf(
            TravelProfile.CAR,
            TravelProfile.TRUCK,
            TravelProfile.MOBILE_HOME,
            TravelProfile.MOTORCYCLE,
            TravelProfile.CAR_ELECTRIC,
            TravelProfile.TRUCK_ELECTRIC,
            TravelProfile.MOTORCYCLE_ELECTRIC,
        )) {
            assertFalse("$p", AvoidTunnelsUi.isAutoSafeProfile(p))
            assertTrue("$p", AvoidTunnelsUi.showsUserToggle(p))
            assertTrue("$p", AvoidTunnelsUi.acceptsPersistedPreference(p))
        }
    }

    @Test
    fun autoSafeProfilesCannotPersistUserOff() {
        for (p in listOf(
            TravelProfile.HIKING,
            TravelProfile.BICYCLE,
            TravelProfile.BICYCLE_ELECTRIC,
        )) {
            // Soft-avoid backup stays on even when session state is false.
            assertTrue("$p", AvoidTunnelsUi.effectiveAvoidTunnels(p, userPreference = false))
            assertTrue("$p", AvoidTunnelsUi.effectiveAvoidTunnels(p, userPreference = true))
            // A restored false must not be accepted as a user setting.
            assertFalse("$p", AvoidTunnelsUi.acceptsPersistedPreference(p))
        }
    }

    @Test
    fun motorProfilesHonorUserPreference() {
        assertFalse(
            AvoidTunnelsUi.effectiveAvoidTunnels(TravelProfile.CAR, userPreference = false),
        )
        assertTrue(
            AvoidTunnelsUi.effectiveAvoidTunnels(TravelProfile.MOBILE_HOME, userPreference = true),
        )
    }
}
