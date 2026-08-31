package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ContourGeneratorTest {
    @Test
    fun intervalsForZoom_lowZoomSkipped() {
        assertNotNull(ContourGenerator.intervalsForZoom(9))
        assertNotNull(ContourGenerator.intervalsForZoom(15))
    }

    @Test
    fun intervalsForZoom_kartverketLadder() {
        assertEquals(30.0 to 150.0, ContourGenerator.intervalsForZoom(9))
        assertEquals(30.0 to 150.0, ContourGenerator.intervalsForZoom(10))
        assertEquals(20.0 to 100.0, ContourGenerator.intervalsForZoom(11))
        assertEquals(20.0 to 100.0, ContourGenerator.intervalsForZoom(12))
        assertEquals(10.0 to 50.0, ContourGenerator.intervalsForZoom(13))
        assertEquals(10.0 to 50.0, ContourGenerator.intervalsForZoom(14))
        assertEquals(5.0 to 25.0, ContourGenerator.intervalsForZoom(15))
    }

    @Test
    fun intervalsForZoom_indexIsFiveTimesMinor() {
        for (zoom in 9..15) {
            val (minor, major) = ContourGenerator.intervalsForZoom(zoom) ?: continue
            assertEquals(minor * 5.0, major, 1e-9)
        }
    }

    @Test
    fun generateFeatures_onRampProducesLines() {
        val w = 32
        val h = 32
        val elev = DoubleArray(w * h)
        for (y in 0 until h) {
            for (x in 0 until w) {
                elev[y * w + x] = x.toDouble() * 10.0 + y.toDouble() * 5.0
            }
        }
        val grid =
            DemElevationGrid(
                width = w,
                height = h,
                elev = elev,
                west = 10.0,
                south = 60.0,
                east = 10.5,
                north = 60.5,
            )
        val features = ContourGenerator.generateFeatures(grid, zoom = 12)
        assertTrue(features.size > 10)
    }

    @Test
    fun generateFeatures_belowMinZoomEmpty() {
        val grid =
            DemElevationGrid(
                width = 4,
                height = 4,
                elev = doubleArrayOf(100.0, 110.0, 120.0, 130.0, 100.0, 110.0, 120.0, 130.0, 100.0, 110.0, 120.0, 130.0, 100.0, 110.0, 120.0, 130.0),
                west = 0.0,
                south = 0.0,
                east = 1.0,
                north = 1.0,
            )
        assertTrue(ContourGenerator.generateFeatures(grid, zoom = 8).isEmpty())
    }

    @Test
    fun stitch_twoAdjacentTiles_doublesWidth() {
        val w = 4
        val h = 4

        fun tile(
            west: Double,
            east: Double,
            base: Double,
        ) = DemElevationGrid(
            w,
            h,
            DoubleArray(w * h) { i -> base + i },
            west,
            60.0,
            east,
            60.1,
        )
        val left = tile(10.0, 10.05, 100.0)
        val right = tile(10.05, 10.1, 200.0)
        val stitched = DemElevationGridDecoder.stitch(listOf(left, right))
        assertNotNull(stitched)
        assertEquals(w * 2, stitched!!.width)
        assertEquals(h, stitched.height)
        assertEquals(10.0, stitched.west, 1e-9)
        assertEquals(10.1, stitched.east, 1e-9)
    }

    @Test
    fun sampleDimForZoom_sameOnlineOffline() {
        assertEquals(48, DemTileFetcher.sampleDimForZoom(9))
        assertEquals(160, DemTileFetcher.sampleDimForZoom(15))
    }
}
