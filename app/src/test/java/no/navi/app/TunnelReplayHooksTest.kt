package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/**
 * Release builds keep the replay fields and ignore them. Debug builds honour
 * the same gate, so this test passes in both variants.
 */
class TunnelReplayHooksTest {
    @Test
    fun replayControlsStayOffUnlessDebug() {
        val previous = NaviMapTestHooks.ignoreLiveGpsFixes
        NaviMapTestHooks.ignoreLiveGpsFixes = true
        try {
            assertEquals(BuildConfig.DEBUG, NaviMapTestHooks.replayHooksEnabled())
            assertEquals(BuildConfig.DEBUG, NaviMapTestHooks.shouldIgnoreLiveGpsFixes())
            if (!BuildConfig.DEBUG) {
                assertFalse(NaviMapTestHooks.shouldIgnoreLiveGpsFixes())
                assertFalse(NaviMapTestHooks.replayHooksEnabled())
            }
        } finally {
            NaviMapTestHooks.ignoreLiveGpsFixes = previous
        }
    }
}
