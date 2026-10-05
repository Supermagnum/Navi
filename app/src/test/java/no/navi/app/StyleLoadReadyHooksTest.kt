package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** Unit coverage for MapLibre style-ready generation on [NaviMapTestHooks]. */
class StyleLoadReadyHooksTest {
    @Before
    fun setUp() {
        NaviMapTestHooks.resetStyleLoadState()
    }

    @Test
    fun beginThenComplete_marksReadyForCurrentGeneration() {
        assertFalse(NaviMapTestHooks.styleReady)
        NaviMapTestHooks.beginStyleApply(1)
        assertFalse(NaviMapTestHooks.styleReady)
        NaviMapTestHooks.completeStyleApply(1)
        assertTrue(NaviMapTestHooks.styleReady)
        assertEquals(1, NaviMapTestHooks.styleReadyGeneration())
    }

    @Test
    fun lateCompleteForOlderGeneration_doesNotMarkReady() {
        NaviMapTestHooks.beginStyleApply(1)
        NaviMapTestHooks.beginStyleApply(2)
        NaviMapTestHooks.completeStyleApply(1)
        assertFalse(NaviMapTestHooks.styleReady)
        NaviMapTestHooks.completeStyleApply(2)
        assertTrue(NaviMapTestHooks.styleReady)
        assertEquals(2, NaviMapTestHooks.styleReadyGeneration())
    }

    @Test
    fun midWaitReload_clearsThenReadyAgainOnNewerGeneration() {
        NaviMapTestHooks.beginStyleApply(1)
        NaviMapTestHooks.completeStyleApply(1)
        assertTrue(NaviMapTestHooks.styleReady)
        val gate = NaviMapTestHooks.styleReadyGeneration()

        NaviMapTestHooks.beginStyleApply(2)
        assertFalse(NaviMapTestHooks.styleReady)
        NaviMapTestHooks.completeStyleApply(2)
        assertTrue(NaviMapTestHooks.styleReady)
        assertTrue(NaviMapTestHooks.styleReadyGeneration() > gate)
    }

    @Test
    fun assigningStyleReadyFalse_doesNotDropCompletedSignal() {
        NaviMapTestHooks.beginStyleApply(3)
        NaviMapTestHooks.completeStyleApply(3)
        assertTrue(NaviMapTestHooks.styleReady)

        NaviMapTestHooks.styleReady = false
        assertTrue(
            "soft clear must not drop a completed styleReady signal",
            NaviMapTestHooks.styleReady,
        )
    }

    @Test
    fun resetStyleLoadState_hardClearsReady() {
        NaviMapTestHooks.beginStyleApply(4)
        NaviMapTestHooks.completeStyleApply(4)
        NaviMapTestHooks.resetStyleLoadState()
        assertFalse(NaviMapTestHooks.styleReady)
        assertEquals(0, NaviMapTestHooks.styleLoadGeneration())
        assertEquals(0, NaviMapTestHooks.styleReadyGeneration())
    }
}
