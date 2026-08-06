package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Full turn-tier icon table for [RouteManeuver.iconKey].
 * Navit convention: `_1` slight, `_2` normal (~90°), `_3` sharp.
 */
class RouteManeuverIconKeyTest {
    private fun man(
        kind: String,
        exit: Int? = null,
    ) = RouteManeuver(
        lat = 0.0,
        lon = 0.0,
        cumM = 0.0,
        kind = kind,
        street = null,
        roundaboutExit = exit,
    )

    @Test
    fun turnTierIconKey_fullTable() {
        assertEquals("nav_left_1", man("slight_left").iconKey())
        assertEquals("nav_left_2", man("left").iconKey())
        assertEquals("nav_left_3", man("sharp_left").iconKey())
        assertEquals("nav_right_1", man("slight_right").iconKey())
        assertEquals("nav_right_2", man("right").iconKey())
        assertEquals("nav_right_3", man("sharp_right").iconKey())
    }

    @Test
    fun nonTierKinds_haveExplicitIcons() {
        assertEquals("nav_turnaround_left", man("u_turn").iconKey())
        assertEquals("nav_destination", man("destination").iconKey())
        assertEquals("nav_keep_left", man("keep_left").iconKey())
        assertEquals("nav_keep_right", man("keep_right").iconKey())
        assertEquals("nav_exit_left", man("exit_left").iconKey())
        assertEquals("nav_exit_right", man("exit_right").iconKey())
        assertEquals("nav_merge_left", man("merge_left").iconKey())
        assertEquals("nav_merge_right", man("merge_right").iconKey())
        assertEquals("nav_straight", man("straight").iconKey())
        assertEquals("nav_straight", man("unknown").iconKey())
        assertEquals("nav_roundabout_r1", man("roundabout", exit = null).iconKey())
        assertEquals("nav_roundabout_r2", man("roundabout", exit = 2).iconKey())
        assertEquals("nav_roundabout_r3", man("roundabout", exit = 3).iconKey())
        assertEquals("nav_roundabout_r5", man("roundabout", exit = 5).iconKey())
    }

    @Test
    fun explicitIcon_overridesKindMapping() {
        val m =
            RouteManeuver(
                lat = 0.0,
                lon = 0.0,
                cumM = 0.0,
                kind = "roundabout",
                street = null,
                roundaboutExit = 2,
                icon = "nav_roundabout_l3",
            )
        assertEquals("nav_roundabout_l3", m.iconKey())
    }

    @Test
    fun investigationRouteTurns_useNormalTier() {
        // Route 1 maneuver #2 (kind=right) and Route 2 #2/#3 (right, left).
        assertEquals("nav_right_2", man("right").iconKey())
        assertEquals("nav_left_2", man("left").iconKey())
    }
}
