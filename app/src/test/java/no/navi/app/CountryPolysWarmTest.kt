package no.navi.app

import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Build-breaking guard: Stay-in-Country waits on [CountryPolysWarm] (bounded),
 * never builds Natural Earth on the main looper.
 */
class CountryPolysWarmTest {
    @Test
    fun planWaitBudgetIsBounded() {
        assertTrue(
            "plan wait must be long enough for AVD warm but not unbounded",
            CountryPolysWarm.PLAN_WAIT_MS in 30_000L..120_000L,
        )
    }
}
