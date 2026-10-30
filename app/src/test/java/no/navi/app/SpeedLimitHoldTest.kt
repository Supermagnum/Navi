package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SpeedLimitHoldTest {
    @Test
    fun holds_through_brief_null_gap() {
        var now = 1_000L
        val hold = SpeedLimitHold(nowMs = { now })
        assertEquals(80.0, hold.update(80.0)!!, 0.01)
        now += 500L
        assertEquals(80.0, hold.update(null)!!, 0.01)
        now += 20_000L
        assertEquals(80.0, hold.update(null)!!, 0.01)
        now += 15_000L
        assertNull(hold.update(null))
    }

    @Test
    fun requires_confirm_before_adjacent_toggle() {
        var now = 1_000L
        val hold = SpeedLimitHold(nowMs = { now })
        assertEquals(80.0, hold.update(80.0)!!, 0.01)
        now += 100L
        // First sight of 60 — still show 80.
        assertEquals(80.0, hold.update(60.0)!!, 0.01)
        now += 100L
        // Second consecutive 60 — switch.
        assertEquals(60.0, hold.update(60.0)!!, 0.01)
    }

    @Test
    fun same_limit_resets_pending_toggle() {
        var now = 1_000L
        val hold = SpeedLimitHold(nowMs = { now })
        hold.update(80.0)
        now += 100L
        hold.update(60.0)
        now += 100L
        assertEquals(80.0, hold.update(80.0)!!, 0.01)
        now += 100L
        // Need two 60s again.
        assertEquals(80.0, hold.update(60.0)!!, 0.01)
    }
}
