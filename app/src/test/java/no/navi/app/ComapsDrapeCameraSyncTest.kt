package no.navi.app

import android.content.Context
import android.view.View
import androidx.lifecycle.LifecycleOwner
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ComapsDrapeCameraSyncTest {
    private class FakeBackend : ComapsDrape.Backend {
        val viewports = mutableListOf<Triple<Double, Double, Double>>()

        override fun initialize(context: Context): String? = null

        override fun createMapView(context: Context): View = error("not used")

        override fun attachLifecycle(owner: LifecycleOwner) = Unit

        override fun detachLifecycle(owner: LifecycleOwner) = Unit

        override fun setViewport(
            lat: Double,
            lon: Double,
            zoomLevel: Double,
        ) {
            viewports += Triple(lat, lon, zoomLevel)
        }

        override fun zoomOffset(): Double? = 1.0

        override fun setNight(night: Boolean) = Unit

        override fun isNight(): Boolean = false

        override fun setContours(enabled: Boolean) = Unit

        override fun contoursEnabled(): Boolean = false

        override fun set3d(
            allow3d: Boolean,
            allow3dBuildings: Boolean,
        ) = Unit

        override fun get3d(): Pair<Boolean, Boolean> = false to false

        override fun renderingActive(): Boolean = true

        override fun drawScale(): Int = 0
    }

    @After
    fun resetHooks() {
        NaviMapTestHooks.drapeZoomOffset = null
    }

    @Test
    fun drapeOnlyReproducesNorthUpFlatCameras() {
        val sync = DrapeCameraSync()
        assertTrue(sync.aligned(bearing = 0.0, tilt = 0.0))
        assertTrue(sync.aligned(bearing = 359.8, tilt = 0.2))
        assertTrue(sync.aligned(bearing = -0.3, tilt = 0.0))
        assertFalse(sync.aligned(bearing = 12.0, tilt = 0.0))
        assertFalse(sync.aligned(bearing = 180.0, tilt = 0.0))
        assertFalse(sync.aligned(bearing = 0.0, tilt = 45.0))
    }

    @Test
    fun pushAppliesZoomOffsetAndSkipsUnchangedCameras() {
        val sync = DrapeCameraSync()
        val backend = FakeBackend()
        sync.push(backend, 61.0, 10.0, 14.0, force = true)
        assertEquals(1, backend.viewports.size)
        assertEquals(15.0, backend.viewports.single().third, 1e-9)
        // Same camera, not forced: nothing to send.
        sync.push(backend, 61.0, 10.0, 14.0)
        assertEquals(1, backend.viewports.size)
        // Forced re-push (e.g. rendering just became active) always goes through.
        sync.push(backend, 61.0, 10.0, 14.0, force = true)
        assertEquals(2, backend.viewports.size)
    }

    @Test
    fun zoomOffsetMatchesMeasuredSamsungTabletCalibration() {
        // SM-P613: 240 dpi (density 1.5), 1200x2000 surface -> tile 1024, visual scale 1.5.
        // On-device overlay calibration put the best offset between 0.5 and 0.59.
        assertEquals(1024, DrapeCameraSync.tileSize(2000))
        assertEquals(0.585, DrapeCameraSync.zoomOffset(240, 1.5f, 2000), 0.001)
    }

    @Test
    fun tileSizeFollowsCoMapsRule() {
        assertEquals(256, DrapeCameraSync.tileSize(300))
        assertEquals(512, DrapeCameraSync.tileSize(900))
        assertEquals(512, DrapeCameraSync.tileSize(1000))
        assertEquals(512, DrapeCameraSync.tileSize(1100))
        assertEquals(1024, DrapeCameraSync.tileSize(1900))
        assertEquals(1024, DrapeCameraSync.tileSize(2400))
    }

    @Test
    fun zoomOffsetHookOverridesDefaultAndLevelIsClamped() {
        val sync = DrapeCameraSync()
        val backend = FakeBackend()
        NaviMapTestHooks.drapeZoomOffset = 0.5
        sync.push(backend, 61.0, 10.0, 10.0, force = true)
        assertEquals(10.5, backend.viewports.last().third, 1e-9)
        sync.push(backend, 61.0, 10.0, 30.0, force = true)
        assertEquals(19.0, backend.viewports.last().third, 1e-9)
    }
}
