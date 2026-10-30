package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RoadSignWarningHoldTest {
    private fun plate(
        code: String,
        label: String,
        dist: Double = 100.0,
    ) = RoadSignWarningState(
        active = true,
        phase = "approach",
        distanceM = dist,
        iconKey = "icon_$code",
        code = code,
        label = label,
    )

    @Test
    fun holds_through_brief_inactive_gap() {
        var now = 1_000L
        val hold = RoadSignWarningHold(nowMs = { now })
        assertTrue(hold.update(plate("142", "Children ahead")).active)
        now += 500L
        assertTrue(hold.update(RoadSignWarningState()).active)
        now += 5_000L
        assertFalse(hold.update(RoadSignWarningState()).active)
    }

    @Test
    fun requires_confirm_before_switching_plates() {
        var now = 1_000L
        val hold = RoadSignWarningHold(nowMs = { now })
        assertEquals("Children ahead", hold.update(plate("142", "Children ahead")).label)
        now += 100L
        assertEquals(
            "Children ahead",
            hold.update(plate("school", "Children zone: X")).label,
        )
        now += 100L
        assertEquals(
            "Children zone: X",
            hold.update(plate("school", "Children zone: X")).label,
        )
    }
}
