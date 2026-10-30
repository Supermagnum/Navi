package no.navi.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class BasemapStyleApplyQueueForceTest {
    @Before
    fun reset() {
        BasemapStyleApplyQueue.resetForTests()
    }

    @Test
    fun force_reload_wins_when_source_key_unchanged() {
        val first = BasemapStyleApplyQueue.enqueue("mount-a|3d=false|contours=false")
        assertTrue(BasemapStyleApplyQueue.shouldReloadBase(first))
        BasemapStyleApplyQueue.accept(
            BasemapStyleApplyQueue.Result(
                generation = first.generation,
                sourceKey = first.sourceKey,
                ok = true,
                reloadedBase = true,
            ),
        )
        val sameKey =
            BasemapStyleApplyQueue.enqueue(
                "mount-a|3d=false|contours=false",
                forceBaseReload = false,
            )
        assertFalse(BasemapStyleApplyQueue.shouldReloadBase(sameKey))
        val forced =
            BasemapStyleApplyQueue.enqueue(
                "mount-a|3d=false|contours=false",
                forceBaseReload = true,
            )
        assertTrue(BasemapStyleApplyQueue.shouldReloadBase(forced))
    }

    @Test
    fun source_key_change_reloads_without_force() {
        val first = BasemapStyleApplyQueue.enqueue("mount-a|3d=false|contours=false")
        BasemapStyleApplyQueue.accept(
            BasemapStyleApplyQueue.Result(
                generation = first.generation,
                sourceKey = first.sourceKey,
                ok = true,
                reloadedBase = true,
            ),
        )
        val toggled = BasemapStyleApplyQueue.enqueue("mount-a|3d=true|contours=true")
        assertTrue(BasemapStyleApplyQueue.shouldReloadBase(toggled))
    }
}
