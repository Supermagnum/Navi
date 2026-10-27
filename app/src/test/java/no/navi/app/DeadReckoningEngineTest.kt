package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.abs

class DeadReckoningEngineTest {
    private fun sample(
        cumM: Double,
        lat: Double,
        lon: Double,
        speedKmh: Double = 80.0,
    ) = RouteSimSample(
        lat = lat,
        lon = lon,
        cumM = cumM,
        speedKmh = speedKmh,
        highway = "trunk",
        maxspeedPosted = true,
        maxspeedKmh = speedKmh,
    )

    /** Straight northbound tunnel corridor (~2 km). */
    private fun straightTunnel(): DrConstraint {
        val samples = mutableListOf<RouteSimSample>()
        val startLat = 60.0
        val startLon = 10.0
        for (i in 0..40) {
            val cum = i * 50.0
            val p = DeadReckoningEngine.destinationPoint(startLat, startLon, 0.0, cum)
            samples += sample(cum, p.first, p.second, 80.0)
        }
        return DrConstraint(samples)
    }

    /** Gentle curve (heading sweeps 0 → 90° over 2 km). */
    private fun curvedTunnel(): DrConstraint {
        val samples = mutableListOf<RouteSimSample>()
        var lat = 60.0
        var lon = 10.0
        var cum = 0.0
        samples += sample(0.0, lat, lon)
        for (i in 1..40) {
            val bearing = (i / 40.0) * 90.0
            val step = 50.0
            val p = DeadReckoningEngine.destinationPoint(lat, lon, bearing, step)
            lat = p.first
            lon = p.second
            cum += step
            samples += sample(cum, lat, lon)
        }
        return DrConstraint(samples)
    }

    private fun seedGps(
        engine: DeadReckoningEngine,
        constraint: DrConstraint,
        tMs: Long = 0L,
        speedMps: Double = 80.0 / 3.6,
        alongM: Double = 0.0,
    ): DrOutput {
        val p = DeadReckoningEngine.pointAtAlong(constraint, alongM)
        return engine.onGps(
            GpsSample(
                tMs = tMs,
                lat = p.lat,
                lon = p.lon,
                accuracyM = 8f,
                speedMps = speedMps,
                bearingDeg = p.bearingDeg,
            ),
            constraint,
        )
    }

    private fun loseGps(
        engine: DeadReckoningEngine,
        constraint: DrConstraint,
        tMs: Long,
    ): DrOutput =
        engine.onGps(
            GpsSample(
                tMs = tMs,
                lat = 0.0,
                lon = 0.0,
                accuracyM = null,
                available = false,
            ),
            constraint,
        )

    @Test
    fun straightTunnel_keepsMovingAlongRoad() {
        val engine = DeadReckoningEngine()
        val road = straightTunnel()
        seedGps(engine, road, tMs = 0L)
        loseGps(engine, road, tMs = 2_500L)
        assertEquals(DrMode.Estimating, engine.mode())

        var lastAlong = 0.0
        var out: DrOutput? = null
        for (i in 1..40) {
            val t = 2_500L + i * 200L
            out =
                engine.onSensorTick(
                    SensorFrame(tMs = t, headingDeg = 0.0, forwardAccelMps2 = 0.0),
                    road,
                )
            assertTrue(out.estimated)
            assertFalse(out.positionLost)
            assertNotNull(out.alongM)
            assertTrue(out.alongM!! > lastAlong)
            lastAlong = out.alongM!!
            // Stay on the constraint polyline (not merely near a discrete sample).
            val onRoad = DeadReckoningEngine.pointAtAlong(road, out.alongM!!)
            val xt =
                RouteProgressTracker.haversineM(out.lat, out.lon, onRoad.lat, onRoad.lon)
            assertTrue("cross-track $xt m", xt < 1.0)
        }
        assertTrue("along=$lastAlong", lastAlong > 150.0)
    }

    @Test
    fun curvedTunnel_followsPolylineHeading() {
        val engine = DeadReckoningEngine()
        val road = curvedTunnel()
        seedGps(engine, road, tMs = 0L)
        loseGps(engine, road, tMs = 2_500L)

        var out: DrOutput =
            engine.onSensorTick(
                SensorFrame(tMs = 2_700L, yawRateDegPerS = 0.0),
                road,
            )
        for (i in 1..120) {
            val t = 2_700L + i * 200L
            // Heading comes from the constraint as we advance; gyro optional.
            out = engine.onSensorTick(SensorFrame(tMs = t), road)
        }
        assertTrue(out.estimated)
        assertTrue("along=${out.alongM}", out.alongM!! > 400.0)
        val onRoad = DeadReckoningEngine.pointAtAlong(road, out.alongM!!)
        val xt = RouteProgressTracker.haversineM(out.lat, out.lon, onRoad.lat, onRoad.lon)
        assertTrue("curved cross-track $xt", xt < 1.0)
        // Bearing should have turned toward east on this curve (~90° over 2 km).
        assertTrue("bearing ${out.bearingDeg}", out.bearingDeg in 15.0..100.0)
    }

    @Test
    fun speedChangeInTunnel_accelUpdatesCarry() {
        val engine = DeadReckoningEngine()
        val road = straightTunnel()
        seedGps(engine, road, tMs = 0L, speedMps = 20.0)
        loseGps(engine, road, tMs = 2_500L)

        // Decelerate hard for 2 s.
        for (i in 1..10) {
            engine.onSensorTick(
                SensorFrame(
                    tMs = 2_500L + i * 200L,
                    headingDeg = 0.0,
                    forwardAccelMps2 = -2.0,
                ),
                road,
            )
        }
        val slow = engine.onSensorTick(SensorFrame(tMs = 4_700L, headingDeg = 0.0), road)
        assertTrue(slow.speedMps < 18.0)

        // Accelerate.
        for (i in 1..15) {
            engine.onSensorTick(
                SensorFrame(
                    tMs = 4_700L + i * 200L,
                    headingDeg = 0.0,
                    forwardAccelMps2 = 1.5,
                ),
                road,
            )
        }
        val fast = engine.onSensorTick(SensorFrame(tMs = 8_000L, headingDeg = 0.0), road)
        assertTrue(fast.speedMps > slow.speedMps)
    }

    @Test
    fun gpsReturnWithOffset_blendsAndReportsExitError() {
        val engine = DeadReckoningEngine()
        val road = straightTunnel()
        seedGps(engine, road, tMs = 0L, alongM = 100.0)
        loseGps(engine, road, tMs = 2_500L)

        // Drive estimate to ~600 m+ (80 km/h ≈ 22 m/s).
        for (i in 1..120) {
            engine.onSensorTick(
                SensorFrame(tMs = 2_500L + i * 200L, headingDeg = 0.0),
                road,
            )
        }
        val before = engine.onSensorTick(SensorFrame(tMs = 27_000L, headingDeg = 0.0), road)
        assertTrue("along=${before.alongM}", before.alongM!! > 400.0)

        // GPS returns near 500 m (behind estimate) with good accuracy.
        val returnAlong = 500.0
        val ret = DeadReckoningEngine.pointAtAlong(road, returnAlong)
        var out =
            engine.onGps(
                GpsSample(
                    tMs = 27_200L,
                    lat = ret.lat,
                    lon = ret.lon,
                    accuracyM = 10f,
                    speedMps = 22.0,
                    bearingDeg = ret.bearingDeg,
                ),
                road,
            )
        assertTrue("mode=${out.mode}", out.estimated || out.mode == DrMode.Gps)

        // Finish blend window (default 800 ms).
        for (i in 1..12) {
            out =
                engine.onSensorTick(
                    SensorFrame(tMs = 27_200L + i * 100L, headingDeg = ret.bearingDeg),
                    road,
                )
        }
        assertEquals(DrMode.Gps, engine.mode())
        val err = out.exitErrorM ?: engine.lastExitErrorM()
        assertNotNull("expected exit error after blend", err)
        assertTrue("exit error $err", err!! > 50.0) // estimate was ahead of return
        // Final position near return fix.
        val xt = RouteProgressTracker.haversineM(out.lat, out.lon, ret.lat, ret.lon)
        assertTrue("after blend xt=$xt", xt < 30.0)
    }

    @Test
    fun longTunnel_hitsGiveUpLimit() {
        val engine =
            DeadReckoningEngine(
                DrConfig(
                    maxDrDurationMs = 5_000L,
                    maxDrDistanceM = 50_000.0,
                    gpsLossTimeoutMs = 1_000L,
                ),
            )
        val road = straightTunnel()
        seedGps(engine, road, tMs = 0L)
        loseGps(engine, road, tMs = 1_500L)
        assertEquals(DrMode.Estimating, engine.mode())

        var out: DrOutput? = null
        for (i in 1..40) {
            out =
                engine.onSensorTick(
                    SensorFrame(tMs = 1_500L + i * 200L, headingDeg = 0.0),
                    road,
                )
        }
        assertEquals(DrMode.PositionLost, engine.mode())
        assertTrue(out!!.positionLost)
        assertFalse(out.estimated)
    }

    @Test
    fun longTunnel_hitsDistanceGiveUp() {
        val engine =
            DeadReckoningEngine(
                DrConfig(
                    maxDrDurationMs = 600_000L,
                    maxDrDistanceM = 200.0,
                    gpsLossTimeoutMs = 1_000L,
                ),
            )
        val road = straightTunnel()
        seedGps(engine, road, tMs = 0L, speedMps = 30.0)
        loseGps(engine, road, tMs = 1_500L)
        var out: DrOutput? = null
        for (i in 1..20) {
            out =
                engine.onSensorTick(
                    SensorFrame(tMs = 1_500L + i * 500L, headingDeg = 0.0),
                    road,
                )
        }
        assertEquals(DrMode.PositionLost, engine.mode())
        assertTrue(out!!.positionLost)
    }

    @Test
    fun noCalibration_relativeYawStillAdvancesOnConstraint() {
        // No absolute heading — only yaw rate; road constraint supplies bearing.
        val engine = DeadReckoningEngine()
        val road = curvedTunnel()
        seedGps(engine, road, tMs = 0L)
        loseGps(engine, road, tMs = 2_500L)

        var out: DrOutput? = null
        for (i in 1..40) {
            out =
                engine.onSensorTick(
                    SensorFrame(
                        tMs = 2_500L + i * 200L,
                        headingDeg = null,
                        yawRateDegPerS = 2.0,
                        forwardAccelMps2 = null,
                    ),
                    road,
                )
        }
        assertTrue(out!!.estimated)
        assertTrue("along=${out.alongM}", out.alongM!! > 150.0)
        val onRoad = DeadReckoningEngine.pointAtAlong(road, out.alongM!!)
        val xt = RouteProgressTracker.haversineM(out.lat, out.lon, onRoad.lat, onRoad.lon)
        assertTrue("uncalibrated xt=$xt", xt < 1.0)
    }

    @Test
    fun degradedAccuracy_entersEstimatingAfterHold() {
        val engine =
            DeadReckoningEngine(
                DrConfig(gpsDegradeAccuracyM = 40f, gpsDegradeHoldMs = 1_000L, goodAccuracyM = 25f),
            )
        val road = straightTunnel()
        seedGps(engine, road, tMs = 0L)
        val p = DeadReckoningEngine.pointAtAlong(road, 50.0)
        engine.onGps(
            GpsSample(tMs = 500L, lat = p.lat, lon = p.lon, accuracyM = 80f, speedMps = 20.0),
            road,
        )
        assertEquals(DrMode.Gps, engine.mode())
        engine.onGps(
            GpsSample(tMs = 1_600L, lat = p.lat, lon = p.lon, accuracyM = 90f, speedMps = 20.0),
            road,
        )
        assertEquals(DrMode.Estimating, engine.mode())
    }

    @Test
    fun reportExitError_atTunnelExit() {
        val engine = DeadReckoningEngine()
        val road = straightTunnel()
        seedGps(engine, road, tMs = 0L, alongM = 0.0, speedMps = 25.0)
        loseGps(engine, road, tMs = 2_500L)
        // Simulate ~800 m of tunnel.
        for (i in 1..20) {
            engine.onSensorTick(
                SensorFrame(tMs = 2_500L + i * 400L, headingDeg = 0.0),
                road,
            )
        }
        val est = engine.onSensorTick(SensorFrame(tMs = 11_000L, headingDeg = 0.0), road)
        // Truth at exit is est.alongM; GPS returns with 15 m lateral offset.
        val along = est.alongM!!
        val onRoad = DeadReckoningEngine.pointAtAlong(road, along)
        val offset =
            DeadReckoningEngine.destinationPoint(onRoad.lat, onRoad.lon, 90.0, 15.0)
        engine.onGps(
            GpsSample(
                tMs = 11_200L,
                lat = offset.first,
                lon = offset.second,
                accuracyM = 8f,
                speedMps = 25.0,
                bearingDeg = onRoad.bearingDeg,
            ),
            road,
        )
        for (i in 1..10) {
            engine.onSensorTick(SensorFrame(tMs = 11_200L + i * 100L), road)
        }
        val err = engine.lastExitErrorM()
        assertNotNull(err)
        // Lateral 15 m plus any along mismatch from nearest-sample snap.
        assertTrue("exit error $err should be near 15 m", abs(err!! - 15.0) < 25.0)
        println("position error at tunnel exit: ${"%.1f".format(err)} m")
    }

    @Test
    fun gnssSilence_entersEstimatingAfterLossTimeout() {
        val engine = DeadReckoningEngine()
        val road = straightTunnel()
        val p = DeadReckoningEngine.pointAtAlong(road, 100.0)
        engine.onGps(
            GpsSample(
                tMs = 1_000L,
                lat = p.lat,
                lon = p.lon,
                accuracyM = 5f,
                speedMps = 20.0,
                bearingDeg = p.bearingDeg,
            ),
            road,
        )
        engine.onGnssSilence(2_000L)
        assertEquals(DrMode.Gps, engine.mode())
        engine.onGnssSilence(1_000L + 2_000L)
        assertEquals(DrMode.Estimating, engine.mode())
        val out = engine.onSensorTick(SensorFrame(tMs = 3_200L, headingDeg = p.bearingDeg), road)
        assertTrue(out.estimated)
        assertEquals(20.0, out.speedMps, 0.05)
    }
}
