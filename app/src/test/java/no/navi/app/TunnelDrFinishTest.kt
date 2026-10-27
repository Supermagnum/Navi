package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.PI
import kotlin.math.sin

/**
 * Idle margin, bias is covered by the simulated suite, tunnel-aware give-up,
 * and the accelerometer-minus-gravity fallback. Tunnel geometry here is
 * synthetic. No place names in the engine.
 */
class TunnelDrFinishTest {
    @Test
    fun idleAfterBrakingStop_staysLatchedThroughVibrationRevAndSlam() {
        val levels = listOf(0.20, 0.25, 0.30, 0.40)
        for (rms in levels) {
            val creep = runIdle(rms, rev = false, slam = false)
            println("idle rms=$rms creep=${"%.2f".format(creep.creepM)} latched=${creep.stayed}")
            assertTrue("rms=$rms stayed=${creep.stayed} creep=${creep.creepM}", creep.stayed && creep.creepM <= 1.0)
        }
        val rev = runIdle(0.15, rev = true, slam = false)
        println("idle rev creep=${"%.2f".format(rev.creepM)} latched=${rev.stayed}")
        assertTrue("rev stayed=${rev.stayed} creep=${rev.creepM}", rev.stayed && rev.creepM <= 1.0)
        val slam = runIdle(0.15, rev = false, slam = true)
        println("idle slam creep=${"%.2f".format(slam.creepM)} latched=${slam.stayed}")
        assertTrue("slam stayed=${slam.stayed} creep=${slam.creepM}", slam.stayed && slam.creepM <= 1.0)
    }

    @Test
    fun standstillFlag_noisyStationaryStaysLatched_andPullReleasesPromptly() {
        val stopped = runUntilLatched()
        val engine = stopped.first
        val road = stopped.second
        var t = stopped.third
        val settleEnd = t + 2_000L
        while (t < settleEnd) {
            engine.onGps(GpsSample(tMs = t, lat = 0.0, lon = 0.0, available = false), road)
            engine.onSensorTick(
                SensorFrame(tMs = t, headingDeg = 0.0, yawRateDegPerS = 0.0, forwardAccelMps2 = 0.0),
                road,
            )
            t += 100L
        }
        // One-sample gyro spike and one-sample accel slam. Neither holds for 200 ms.
        engine.onSensorTick(
            SensorFrame(tMs = t, headingDeg = 0.0, yawRateDegPerS = 20.0, forwardAccelMps2 = 0.0),
            road,
        )
        t += 100L
        engine.onSensorTick(
            SensorFrame(tMs = t, headingDeg = 0.0, yawRateDegPerS = 0.2, forwardAccelMps2 = 12.0),
            road,
        )
        t += 100L
        repeat(10) {
            engine.onSensorTick(
                SensorFrame(tMs = t, headingDeg = 0.0, yawRateDegPerS = 0.2, forwardAccelMps2 = 0.05),
                road,
            )
            t += 100L
        }
        assertTrue(engine.diagSnapshot().standstill == true)

        // A single GNSS speed above the latch, then zero again, must not clear it.
        val p = DeadReckoningEngine.pointAtAlong(road, engine.diagSnapshot().alongM ?: 0.0)
        engine.onGps(
            GpsSample(
                tMs = t,
                lat = p.lat,
                lon = p.lon,
                accuracyM = 8f,
                speedMps = 1.2,
                bearingDeg = p.bearingDeg,
            ),
            road,
        )
        t += 1_000L
        engine.onGps(
            GpsSample(
                tMs = t,
                lat = p.lat,
                lon = p.lon,
                accuracyM = 8f,
                speedMps = 0.0,
                bearingDeg = p.bearingDeg,
            ),
            road,
        )
        assertTrue(
            "one noisy GNSS speed cleared standstill",
            engine.diagSnapshot().standstill == true,
        )

        // Let the slam decay out of the 0.5 s filter before timing the pull.
        val calmEnd = t + 3_000L
        while (t < calmEnd) {
            engine.onGps(GpsSample(tMs = t, lat = 0.0, lon = 0.0, available = false), road)
            engine.onSensorTick(
                SensorFrame(tMs = t, headingDeg = 0.0, yawRateDegPerS = 0.0, forwardAccelMps2 = 0.0),
                road,
            )
            t += 100L
        }
        assertTrue(engine.diagSnapshot().standstill == true)

        val pullStart = t
        var released: Long? = null
        while (t < pullStart + 1_500L) {
            engine.onGps(GpsSample(tMs = t, lat = 0.0, lon = 0.0, available = false), road)
            engine.onSensorTick(
                SensorFrame(tMs = t, headingDeg = 0.0, yawRateDegPerS = 0.0, forwardAccelMps2 = 0.3),
                road,
            )
            if (released == null && engine.diagSnapshot().standstill != true) {
                released = t - pullStart
            }
            t += 100L
        }
        println("standstill release from first 0.3 m/s^2 sample: $released ms")
        assertTrue("release latency $released", released != null && released!! in 0..1_000L)
    }

    @Test
    fun gentlePull_stillReleasesWithinOneSecond() {
        for (pull in listOf(0.3, 0.5, 0.8)) {
            val release = releaseAfterPull(pull)
            println("gentle pull=$pull releaseMs=$release")
            assertTrue("pull=$pull release=$release", release in 0..1_000L)
        }
    }

    @Test
    fun longTunnel_defersGiveUpUntilPastTheSpan() {
        val length = 24_500.0
        for (speed in listOf(50.0, 80.0)) {
            val result = runLongTunnel(length, speed, queue = false)
            val limit = 0.05 * length
            println(
                "long steady $speed exit=${"%.1f".format(result.exitJumpM)} " +
                    "modeAtExit=${result.modeAtExit} phases=${result.phases}",
            )
            assertEquals(DrMode.Estimating, result.modeAtExit)
            assertTrue("exit ${result.exitJumpM} > $limit", result.exitJumpM <= limit)
            assertTrue(result.estimatingAtSevenMin)
        }
        val queue = runLongTunnel(length, 50.0, queue = true)
        println(
            "long queue latch=${queue.latchMs} creep=${"%.2f".format(queue.creepM)} " +
                "exit=${"%.1f".format(queue.exitJumpM)} phases=${queue.phases}",
        )
        assertEquals(DrMode.Estimating, queue.modeAtExit)
        assertTrue("latch=${queue.latchMs} phases=${queue.phases}", queue.latchMs != null && queue.latchMs!! <= 2_000L)
        assertTrue("creep=${queue.creepM}", queue.creepM <= 5.0)
        assertTrue(
            "exit ${queue.exitJumpM} phases=${queue.phases}",
            queue.exitJumpM <= 0.05 * length,
        )
    }

    @Test
    fun noRoute_usesTimeGiveUp() {
        val engine = DeadReckoningEngine()
        engine.onGps(
            GpsSample(
                tMs = 0L,
                lat = 60.0,
                lon = 10.0,
                accuracyM = 5f,
                speedMps = 20.0,
                bearingDeg = 0.0,
            ),
        )
        engine.onGnssSilence(2_500L)
        assertEquals(DrMode.Estimating, engine.mode())
        var t = 2_600L
        while (t < 362_400L) {
            engine.onSensorTick(SensorFrame(tMs = t, headingDeg = 0.0, forwardAccelMps2 = 0.0))
            t += 200L
        }
        assertEquals(DrMode.Estimating, engine.mode())
        engine.onSensorTick(SensorFrame(tMs = 362_600L, headingDeg = 0.0, forwardAccelMps2 = 0.0))
        assertEquals(DrMode.PositionLost, engine.mode())
    }

    @Test
    fun fallbackLinearAccel_passesSteadyAndQueue() {
        val engine = DeadReckoningEngine()
        trainDevice(engine)
        assertEquals("confident", engine.forwardAxisState())
        engine.onGps(gps(20_000L, 20.0), null)
        engine.onGps(gps(22_500L, available = false), null)
        assertEquals(DrMode.Estimating, engine.mode())
        var t = 22_600L
        val end = t + 15_000L
        while (t <= end) {
            val out = engine.onSensorTick(deviceFrame(t, 0.0), null)
            assertTrue("steady speed ${out.speedMps}", kotlin.math.abs(out.speedMps - 20.0) < 0.5)
            assertTrue(engine.diagSnapshot().standstill != true)
            t += 100L
        }

        val queue = DeadReckoningEngine()
        trainDevice(queue)
        queue.onGps(gps(20_000L, 16.0), null)
        queue.onGps(gps(22_500L, available = false), null)
        t = 22_600L
        var latchedAt: Long? = null
        while (t < 22_600L + 12_000L) {
            queue.onSensorTick(deviceFrame(t, -2.5), null)
            t += 100L
        }
        val stopAlongStart = t
        var creep = 0.0
        var origin: Double? = null
        while (t < stopAlongStart + 3_000L) {
            val out = queue.onSensorTick(deviceFrame(t, 0.0), null)
            if (engineLatched(queue)) {
                if (latchedAt == null) latchedAt = t
                val along = out.alongM ?: 0.0
                if (origin == null) origin = along
                creep = maxOf(creep, kotlin.math.abs(along - origin!!))
            }
            t += 100L
        }
        assertTrue("latch=$latchedAt", latchedAt != null && latchedAt!! - (22_600L + 12_000L) <= 2_000L)
        assertTrue("creep=$creep", creep <= 1.0)
        val pullStart = t
        var released: Long? = null
        while (t < pullStart + 1_500L) {
            queue.onSensorTick(deviceFrame(t, 0.5), null)
            if (released == null && queue.diagSnapshot().standstill != true) released = t
            t += 100L
        }
        assertTrue("release=$released", released != null && released!! - pullStart <= 1_000L)
        println("fallback latch=$latchedAt release=$released creep=$creep source=accel_minus_gravity")
    }

    private data class IdleResult(
        val stayed: Boolean,
        val creepM: Double,
    )

    private fun runIdle(
        rms: Double,
        rev: Boolean,
        slam: Boolean,
    ): IdleResult {
        val stopped = runUntilLatched()
        val engine = stopped.first
        val road = stopped.second
        var t = stopped.third
        val stopT = t
        var holdAlong: Double? = null
        var creep = 0.0
        var stayed = true
        var sawLatch = false
        while (t <= stopT + 8_000L) {
            var extra = 0.0
            val since = t - stopT
            // One sample is shorter than the 200 ms sustain, so a blip is not motion.
            if (rev && since == 3_000L) extra = 3.0
            if (slam && since == 5_000L) extra = 12.0
            val vib = idleSample(rms, t)
            engine.onGps(GpsSample(tMs = t, lat = 0.0, lon = 0.0, available = false), road)
            val out =
                engine.onSensorTick(
                    SensorFrame(
                        tMs = t,
                        headingDeg = 0.0,
                        yawRateDegPerS = 0.0,
                        forwardAccelMps2 = vib + extra,
                    ),
                    road,
                )
            val latched = engine.diagSnapshot().standstill == true
            if (latched) {
                sawLatch = true
                val a = out.alongM ?: 0.0
                if (holdAlong == null) holdAlong = a
                creep = maxOf(creep, kotlin.math.abs(a - holdAlong!!))
            } else if (sawLatch) {
                stayed = false
            }
            t += 100L
        }
        return IdleResult(sawLatch && stayed, creep)
    }

    private fun releaseAfterPull(pull: Double): Long {
        val idle = runUntilLatched()
        val engine = idle.first
        val road = idle.second
        var t = idle.third
        val settleEnd = t + 3_000L
        while (t < settleEnd) {
            engine.onGps(GpsSample(tMs = t, lat = 0.0, lon = 0.0, available = false), road)
            engine.onSensorTick(
                SensorFrame(tMs = t, headingDeg = 0.0, forwardAccelMps2 = 0.0),
                road,
            )
            t += 100L
        }
        val start = t
        var released: Long? = null
        while (t < start + 1_500L) {
            engine.onGps(GpsSample(tMs = t, lat = 0.0, lon = 0.0, available = false), road)
            engine.onSensorTick(
                SensorFrame(tMs = t, headingDeg = 0.0, yawRateDegPerS = 0.0, forwardAccelMps2 = pull),
                road,
            )
            if (released == null && engine.diagSnapshot().standstill != true) released = t - start
            t += 100L
        }
        return released ?: 9_999L
    }

    private fun runUntilLatched(): Triple<DeadReckoningEngine, DrConstraint, Long> {
        val engine = DeadReckoningEngine()
        val road = straight(2_000.0)
        var t = 0L
        var speed = 16.0
        var along = 0.0
        while (along < 300.0) {
            stepGnss(engine, road, t, along, speed)
            engine.onSensorTick(SensorFrame(tMs = t, headingDeg = 0.0, forwardAccelMps2 = 0.0), road)
            along += speed * 0.1
            t += 100L
        }
        val atBrake = DeadReckoningEngine.pointAtAlong(road, along.coerceAtMost(road.samples.last().cumM))
        engine.onGps(
            GpsSample(
                tMs = t,
                lat = atBrake.lat,
                lon = atBrake.lon,
                accuracyM = 5f,
                speedMps = speed,
                bearingDeg = atBrake.bearingDeg,
            ),
            road,
        )
        val brakeStart = t
        while (engine.diagSnapshot().standstill != true && t < brakeStart + 20_000L) {
            engine.onGps(GpsSample(tMs = t, lat = 0.0, lon = 0.0, available = false), road)
            val braking = engine.diagSnapshot().speedMps > 0.4
            engine.onSensorTick(
                SensorFrame(
                    tMs = t,
                    headingDeg = 0.0,
                    forwardAccelMps2 = if (braking) -2.5 else 0.0,
                ),
                road,
            )
            t += 100L
        }
        val snap = engine.diagSnapshot()
        assertTrue(
            "mode=${engine.mode()} speed=${snap.speedMps} standstill=${snap.standstill}",
            snap.standstill == true,
        )
        return Triple(engine, road, t)
    }

    private data class LongResult(
        val exitJumpM: Double,
        val modeAtExit: DrMode,
        val phases: String,
        val estimatingAtSevenMin: Boolean,
        val latchMs: Long?,
        val creepM: Double,
    )

    private fun runLongTunnel(
        lengthM: Double,
        speedKmh: Double,
        queue: Boolean,
    ): LongResult {
        val road = straight(lengthM)
        val engine = DeadReckoningEngine()
        val cruise = speedKmh / 3.6
        var t = 0L
        var along = 0.0
        var speed = cruise
        var phase = if (queue) 0 else 9
        var stopTicks = 0
        val tunnelEnd = 500.0 + lengthM
        var exitJump = 0.0
        var modeAtExit = DrMode.Gps
        var exitTaken = false
        var estimatingAtSeven = false
        var latchAt: Long? = null
        var stopStart: Long? = null
        var holdAlong: Double? = null
        var creep = 0.0
        val phases = linkedMapOf<String, Double>()
        var phaseName: String? = null
        val endAlong = tunnelEnd + 500.0
        while (along < endAlong && t < 2_400_000L) {
            val accel: Double
            when (phase) {
                0 -> {
                    accel = 0.0
                    speed = cruise
                    if (along >= tunnelEnd / 2.0) phase = 1
                }
                1 -> {
                    accel = -2.0
                    speed = (speed + accel * 0.1).coerceAtLeast(0.0)
                    if (speed <= 0.05) {
                        speed = 0.0
                        phase = 2
                    }
                }
                2 -> {
                    accel = 0.0
                    speed = 0.0
                    stopTicks += 1
                    if (stopTicks >= 100) phase = 3
                }
                3 -> {
                    accel = if (speed < cruise - 0.2) 0.5 else 0.0
                    speed = (speed + accel * 0.1).coerceAtMost(cruise)
                }
                else -> {
                    accel = 0.0
                    speed = cruise
                }
            }
            val inTunnel = along in 500.0..tunnelEnd
            if (!inTunnel && along < 500.0 && t % 1_000L < 100L) {
                val p = DeadReckoningEngine.pointAtAlong(road, along.coerceAtMost(road.samples.last().cumM))
                engine.onGps(
                    GpsSample(tMs = t, lat = p.lat, lon = p.lon, accuracyM = 5f, speedMps = speed, bearingDeg = p.bearingDeg),
                    road,
                )
            } else if (inTunnel || along >= tunnelEnd) {
                if (along < tunnelEnd + 40.0) {
                    engine.onGps(GpsSample(tMs = t, lat = 0.0, lon = 0.0, available = false), road)
                } else if (t % 1_000L < 100L) {
                    val p = DeadReckoningEngine.pointAtAlong(road, along.coerceAtMost(road.samples.last().cumM))
                    engine.onGps(
                        GpsSample(tMs = t, lat = p.lat, lon = p.lon, accuracyM = 5f, speedMps = speed, bearingDeg = p.bearingDeg),
                        road,
                    )
                }
            }
            val out =
                engine.onSensorTick(
                    SensorFrame(tMs = t, headingDeg = 0.0, yawRateDegPerS = 0.0, forwardAccelMps2 = accel),
                    road,
                )
            if (t - 2_500L in 420_000L..420_200L) {
                estimatingAtSeven = engine.mode() == DrMode.Estimating
            }
            val nowPhase =
                when {
                    along < 500.0 -> "approach"
                    speed < 0.3 && inTunnel -> "stop"
                    accel <= -0.5 -> "brake"
                    accel >= 0.25 && speed < cruise - 0.5 -> "pull-away"
                    along > tunnelEnd -> "exit"
                    else -> "cruise"
                }
            val estAlong = out.alongM ?: 0.0
            if (phaseName != null && phaseName != nowPhase) {
                phases[phaseName!!] = estAlong - along
            }
            phaseName = nowPhase
            if (speed < 0.3 && inTunnel) {
                if (stopStart == null) stopStart = t
                if (engine.diagSnapshot().standstill == true) {
                    if (latchAt == null) latchAt = t - (stopStart ?: t)
                    val a = out.alongM ?: 0.0
                    if (holdAlong == null) holdAlong = a
                    creep = maxOf(creep, kotlin.math.abs(a - holdAlong!!))
                }
            }
            if (!exitTaken && along >= tunnelEnd + 20.0) {
                val p = DeadReckoningEngine.pointAtAlong(road, along.coerceAtMost(road.samples.last().cumM))
                exitJump = RouteProgressTracker.haversineM(out.lat, out.lon, p.lat, p.lon)
                modeAtExit = engine.mode()
                exitTaken = true
            }
            along += speed * 0.1
            t += 100L
        }
        if (phaseName != null) phases[phaseName!!] = engine.diagSnapshot().alongM - along
        val phaseText = phases.entries.joinToString(",") { "${it.key}=${"%.0f".format(it.value)}" }
        return LongResult(exitJump, modeAtExit, phaseText, estimatingAtSeven, latchAt, creep)
    }

    private fun straight(tunnelLengthM: Double): DrConstraint {
        val samples = mutableListOf<RouteSimSample>()
        var cum = 0.0

        fun add(
            step: Double,
            tunnel: Boolean,
        ) {
            val p = DeadReckoningEngine.destinationPoint(60.0, 10.0, 0.0, cum + step)
            cum += step
            samples +=
                RouteSimSample(
                    lat = p.first,
                    lon = p.second,
                    cumM = cum,
                    speedKmh = 80.0,
                    highway = "trunk",
                    maxspeedPosted = true,
                    isTunnel = tunnel,
                )
        }
        samples += RouteSimSample(60.0, 10.0, 0.0, 80.0, "trunk", true, isTunnel = false)
        while (cum < 500.0) add(50.0, false)
        while (cum < 500.0 + tunnelLengthM) add(50.0, true)
        while (cum < 500.0 + tunnelLengthM + 800.0) add(50.0, false)
        return DrConstraint(samples)
    }

    private fun stepGnss(
        engine: DeadReckoningEngine,
        road: DrConstraint,
        t: Long,
        along: Double,
        speed: Double,
    ) {
        if (t % 1_000L >= 100L) return
        val p = DeadReckoningEngine.pointAtAlong(road, along.coerceAtMost(road.samples.last().cumM))
        engine.onGps(
            GpsSample(tMs = t, lat = p.lat, lon = p.lon, accuracyM = 5f, speedMps = speed, bearingDeg = p.bearingDeg),
            road,
        )
    }

    private fun idleSample(
        rms: Double,
        tMs: Long,
    ): Double {
        val sec = tMs / 1000.0
        val n = ((tMs * 1103515245L + 12345L) ushr 16) and 0x7fff
        val noise = (n / 16384.0) - 1.0
        // 12 Hz and 22 Hz engine order. At a 10 Hz tick both alias near 2 Hz.
        val diesel = 0.5 * sin(2.0 * PI * 12.0 * sec) + 0.5 * sin(2.0 * PI * 22.0 * sec)
        return rms * (0.65 * noise + 0.35 * diesel)
    }

    private fun trainDevice(engine: DeadReckoningEngine) {
        var speed = 10.0
        var t = 1_000L
        repeat(6) {
            speed += 1.2
            engine.onSensorTick(deviceFrame(t, 1.2), null)
            engine.onGps(gps(t, speed), null)
            t += 1_000L
        }
        repeat(6) {
            speed = (speed - 1.2).coerceAtLeast(2.0)
            engine.onSensorTick(deviceFrame(t, -1.2), null)
            engine.onGps(gps(t, speed), null)
            t += 1_000L
        }
    }

    private fun deviceFrame(
        tMs: Long,
        longAccel: Double,
    ): SensorFrame {
        val specific = floatArrayOf(0f, longAccel.toFloat(), 9.81f)
        val gravity = floatArrayOf(0f, 0f, 9.81f)
        val lin = LinearAccelFallback.minus(specific, gravity)
        return SensorFrame(
            tMs = tMs,
            headingDeg = 0.0,
            yawRateDegPerS = 0.0,
            linAccelX = lin[0].toDouble(),
            linAccelY = lin[1].toDouble(),
            linAccelZ = lin[2].toDouble(),
            gravityX = 0.0,
            gravityY = 0.0,
            gravityZ = 9.81,
        )
    }

    private fun gps(
        tMs: Long,
        speed: Double = 0.0,
        available: Boolean = true,
    ): GpsSample =
        if (!available) {
            GpsSample(tMs = tMs, lat = 0.0, lon = 0.0, available = false)
        } else {
            GpsSample(tMs = tMs, lat = 60.0, lon = 10.0, accuracyM = 5f, speedMps = speed, bearingDeg = 0.0)
        }

    private fun engineLatched(engine: DeadReckoningEngine): Boolean = engine.diagSnapshot().standstill == true
}
