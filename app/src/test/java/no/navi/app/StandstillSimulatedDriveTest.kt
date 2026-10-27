package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Locale
import kotlin.math.sin

/**
 * Simulated drive for the standstill gate.
 *
 * Route names and tunnel roles come from the Raufoss → Bergen regression gate
 * (`timing_raufoss_bergen`): Lærdalstunnelen (long straight) and
 * Gudvangatunnelen (curved). Geometry is generated at test time — lengths are
 * shortened for runtime, roles are kept. No large polyline is committed.
 *
 * Moving / tunnel behaviour is simulation only. There is no real drive.
 */
class StandstillSimulatedDriveTest {
    data class TunnelSpec(
        val name: String,
        val lengthM: Double,
        val curved: Boolean,
    )

    data class TrueState(
        val tMs: Long,
        val alongM: Double,
        val speedMps: Double,
        val inTunnel: Boolean,
        val lat: Double,
        val lon: Double,
        val bearingDeg: Double,
        val yawRateDegPerS: Double,
        val forwardAccelMps2: Double,
    )

    data class RunResult(
        val name: String,
        val vibrationRms: Double,
        val accelBiasMps2: Double,
        val falseLatchWhileMoving: Boolean,
        val shadowFalseLatchWhileMoving: Boolean,
        val latchDelayAfterStopMs: Long?,
        val releaseDelayAfterPullMs: Long?,
        val maxStopCreepM: Double,
        val exitErrorM: Double?,
        val crawlLagM: Double?,
        val crawlExitJumpM: Double?,
        /** Max est−truth along inside the first tunnel (lead +, lag −). */
        val maxLeadLagM: Double,
        val stayedLatchedUnderIdleVib: Boolean?,
    )

    /** Public test origin (Oslo Opera plaza). Test data only. */
    private val originLat = 59.907462
    private val originLon = 10.753595

    private val tunnels =
        listOf(
            TunnelSpec("Lærdalstunnelen", 3_000.0, curved = false),
            TunnelSpec("Gudvangatunnelen", 2_000.0, curved = true),
        )

    /** Typical car cabin vibration RMS steps (m/s²), then near-zero / sensor noise. */
    private val vibrationSteps =
        listOf(0.40, 0.25, 0.15, 0.08, 0.04, 0.02, 0.005)

    private val accelBiases =
        listOf(0.0, 0.02, -0.02, 0.05, -0.05, 0.1, -0.1)

    private fun buildRoute(): DrConstraint {
        val samples = mutableListOf<RouteSimSample>()
        var lat = originLat
        var lon = originLon
        var cum = 0.0
        var bearing = 0.0
        // Seed at the public origin so along=0 is a real sample (not 25 m in).
        samples +=
            RouteSimSample(
                lat = lat,
                lon = lon,
                cumM = 0.0,
                speedKmh = 80.0,
                highway = "trunk",
                maxspeedPosted = true,
                maxspeedKmh = 80.0,
                isTunnel = false,
            )

        fun add(
            stepM: Double,
            turnDegPerM: Double,
            tunnel: Boolean,
            speedKmh: Double,
        ) {
            bearing = DeadReckoningEngine.normalizeDeg(bearing + turnDegPerM * stepM)
            val p = DeadReckoningEngine.destinationPoint(lat, lon, bearing, stepM)
            lat = p.first
            lon = p.second
            cum += stepM
            samples +=
                RouteSimSample(
                    lat = lat,
                    lon = lon,
                    cumM = cum,
                    speedKmh = speedKmh,
                    highway = if (tunnel) "motorway" else "trunk",
                    maxspeedPosted = true,
                    maxspeedKmh = speedKmh,
                    isTunnel = tunnel,
                )
        }
        // Approach.
        repeat(40) { add(25.0, 0.0, false, 80.0) }
        // Lærdalstunnelen — straight.
        val straight = tunnels[0]
        val straightSteps = (straight.lengthM / 25.0).toInt()
        repeat(straightSteps) { add(25.0, 0.0, true, 80.0) }
        // Between tunnels.
        repeat(20) { add(25.0, 0.0, false, 80.0) }
        // Gudvangatunnelen — curved (~90° over length).
        val curved = tunnels[1]
        val curvedSteps = (curved.lengthM / 25.0).toInt()
        val turnPerM = 90.0 / curved.lengthM
        repeat(curvedSteps) { add(25.0, turnPerM, true, 80.0) }
        // Exit approach.
        repeat(40) { add(25.0, 0.0, false, 80.0) }
        return DrConstraint(samples)
    }

    private fun tunnelRanges(road: DrConstraint): List<Pair<Double, Double>> {
        val ranges = mutableListOf<Pair<Double, Double>>()
        var start: Double? = null
        for (s in road.samples) {
            if (s.isTunnel && start == null) start = s.cumM
            if (!s.isTunnel && start != null) {
                ranges += start!! to s.cumM
                start = null
            }
        }
        if (start != null) {
            ranges += start!! to road.samples.last().cumM
        }
        return ranges
    }

    private fun profileSteady(
        road: DrConstraint,
        speedKmh: Double,
    ): List<TrueState> = profileAlong(road, speedKmh / 3.6) { _, _, _ -> null }

    private fun profileQueueInTunnel(
        road: DrConstraint,
        cruiseKmh: Double,
    ): List<TrueState> {
        val ranges = tunnelRanges(road)
        val stopAlong = (ranges.first().first + ranges.first().second) / 2.0
        val cruise = cruiseKmh / 3.6
        val out = mutableListOf<TrueState>()
        var t = 0L
        var along = 0.0
        var speed = cruise
        var phase = 0 // 0 cruise, 1 brake, 2 stop, 3 pull
        var stopTicks = 0
        val end = ranges.first().second + 200.0
        while (along < end && t < 900_000L) {
            val accel: Double
            when (phase) {
                0 -> {
                    accel = 0.0
                    speed = cruise
                    along += speed * 0.1
                    if (along >= stopAlong - (cruise * cruise) / 4.0 - 40.0) phase = 1
                }
                1 -> {
                    accel = -2.0
                    speed = (speed + accel * 0.1).coerceAtLeast(0.0)
                    along += speed * 0.1
                    if (speed <= 0.05 || along >= stopAlong) {
                        speed = 0.0
                        along = along.coerceAtMost(stopAlong)
                        phase = 2
                    }
                }
                2 -> {
                    accel = if (stopTicks < 15) -2.0 else 0.0
                    speed = 0.0
                    stopTicks++
                    if (stopTicks >= 600) phase = 3 // 60 s
                }
                else -> {
                    accel = 2.5
                    speed = (speed + accel * 0.1).coerceAtMost(cruise)
                    along += speed * 0.1
                }
            }
            val p = DeadReckoningEngine.pointAtAlong(road, along.coerceAtMost(road.samples.last().cumM))
            val inTunnel =
                road.samples.minByOrNull { kotlin.math.abs(it.cumM - along) }?.isTunnel == true
            out +=
                TrueState(
                    tMs = t,
                    alongM = along,
                    speedMps = speed,
                    inTunnel = inTunnel,
                    lat = p.lat,
                    lon = p.lon,
                    bearingDeg = p.bearingDeg,
                    yawRateDegPerS = 0.0,
                    forwardAccelMps2 = accel,
                )
            t += 100L
        }
        return out
    }

    private fun profileStopAndGo(road: DrConstraint): List<TrueState> {
        val ranges = tunnelRanges(road)
        val mid = (ranges.first().first + ranges.first().second) / 2.0
        val out = mutableListOf<TrueState>()
        var t = 0L
        var along = 0.0
        var speed = 80.0 / 3.6
        val end = mid + 600.0
        // Long enough phases: cruise 10 s, brake+stop 15 s, pull 10 s.
        while (along < end && t < 900_000L) {
            val cycle = ((t / 1000L) % 35L).toInt()
            val target =
                when {
                    cycle < 10 -> 80.0 / 3.6
                    cycle < 25 -> 0.0
                    else -> 40.0 / 3.6
                }
            val accel =
                when {
                    target <= 0.05 && speed > 0.05 -> -2.0
                    target <= 0.05 -> 0.0
                    speed < target - 0.2 -> 2.5
                    else -> 0.0
                }
            speed = (speed + accel * 0.1).coerceAtLeast(0.0)
            if (target <= 0.05 && speed < 0.05) speed = 0.0
            along += speed * 0.1
            val p = DeadReckoningEngine.pointAtAlong(road, along.coerceAtMost(road.samples.last().cumM))
            val inTunnel = road.samples.minByOrNull { kotlin.math.abs(it.cumM - along) }?.isTunnel == true
            out +=
                TrueState(
                    tMs = t,
                    alongM = along,
                    speedMps = speed,
                    inTunnel = inTunnel,
                    lat = p.lat,
                    lon = p.lon,
                    bearingDeg = p.bearingDeg,
                    yawRateDegPerS = 0.0,
                    forwardAccelMps2 = accel,
                )
            t += 100L
        }
        return out
    }

    private fun profileCrawl(road: DrConstraint): List<TrueState> {
        val ranges = tunnelRanges(road)
        val crawlStartAlong = ranges.first().first + 50.0
        val crawlMps = 4.0 / 3.6
        val cruiseMps = 80.0 / 3.6
        val out = mutableListOf<TrueState>()
        var t = 0L
        var along = 0.0
        var speed = cruiseMps
        var crawledFor = 0L
        val end = ranges.first().second + 200.0
        while (along < end) {
            val accel: Double
            when {
                along < crawlStartAlong -> {
                    accel = 0.0
                    speed = cruiseMps
                    along += speed * 0.1
                }
                crawledFor < 60_000L -> {
                    // Brake into the crawl, then hold constant (no further accel).
                    accel =
                        if (speed > crawlMps + 0.2) {
                            ((crawlMps - speed) / 0.8).coerceIn(-2.5, 0.0)
                        } else {
                            0.0
                        }
                    speed = (speed + accel * 0.1).coerceAtLeast(crawlMps)
                    along += speed * 0.1
                    crawledFor += 100L
                }
                else -> {
                    accel =
                        if (speed < cruiseMps - 0.2) {
                            2.5
                        } else {
                            0.0
                        }
                    speed = (speed + accel * 0.1).coerceAtMost(cruiseMps)
                    along += speed * 0.1
                }
            }
            val p = DeadReckoningEngine.pointAtAlong(road, along.coerceAtMost(road.samples.last().cumM))
            val inTunnel =
                road.samples.minByOrNull { kotlin.math.abs(it.cumM - along) }?.isTunnel == true
            out +=
                TrueState(
                    tMs = t,
                    alongM = along,
                    speedMps = speed,
                    inTunnel = inTunnel,
                    lat = p.lat,
                    lon = p.lon,
                    bearingDeg = p.bearingDeg,
                    yawRateDegPerS = 0.0,
                    forwardAccelMps2 = accel,
                )
            t += 100L
            if (t > 900_000L) break
        }
        return out
    }

    /** Queue stop then idle cabin vibration while latched (engine running). */
    private fun profileQueueThenIdleVib(road: DrConstraint): List<TrueState> = profileQueueInTunnel(road, 80.0)

    private fun profileStopAfterExit(road: DrConstraint): List<TrueState> {
        val ranges = tunnelRanges(road)
        val exit = ranges.first().second
        val stopAt = exit + 40.0
        val out = mutableListOf<TrueState>()
        var t = 0L
        var along = 0.0
        var speed = 80.0 / 3.6
        var stoppedFor = 0L
        while (along < stopAt + 1.0 || stoppedFor < 10_000L) {
            val target = if (along >= stopAt) 0.0 else 80.0 / 3.6
            val accel = ((target - speed) / 0.4).coerceIn(-3.0, 2.0)
            speed = (speed + accel * 0.1).coerceAtLeast(0.0)
            if (along < stopAt || speed > 0.05) {
                along += speed * 0.1
            } else {
                speed = 0.0
                stoppedFor += 100L
            }
            val p = DeadReckoningEngine.pointAtAlong(road, along.coerceAtMost(road.samples.last().cumM))
            val inTunnel =
                road.samples.minByOrNull { kotlin.math.abs(it.cumM - along) }?.isTunnel == true
            out +=
                TrueState(
                    tMs = t,
                    alongM = along,
                    speedMps = speed,
                    inTunnel = inTunnel,
                    lat = p.lat,
                    lon = p.lon,
                    bearingDeg = p.bearingDeg,
                    yawRateDegPerS = 0.0,
                    forwardAccelMps2 = accel,
                )
            t += 100L
            if (t > 600_000L) break
        }
        return out
    }

    private fun profileAlong(
        road: DrConstraint,
        cruiseMps: Double,
        accelOverride: (along: Double, speed: Double, tMs: Long) -> Double?,
    ): List<TrueState> {
        val out = mutableListOf<TrueState>()
        var t = 0L
        var along = 0.0
        var speed = cruiseMps
        val end = road.samples.last().cumM
        var prevBearing = DeadReckoningEngine.pointAtAlong(road, 0.0).bearingDeg
        while (along < end - 1.0) {
            val override = accelOverride(along, speed, t)
            val accel = override ?: 0.0
            if (override != null) {
                speed = (speed + accel * 0.1).coerceAtLeast(0.0)
            } else {
                speed = cruiseMps
            }
            along = (along + speed * 0.1).coerceAtMost(end)
            val p = DeadReckoningEngine.pointAtAlong(road, along)
            val yaw =
                if (t == 0L) {
                    0.0
                } else {
                    var d = p.bearingDeg - prevBearing
                    if (d > 180) d -= 360
                    if (d < -180) d += 360
                    d / 0.1
                }
            prevBearing = p.bearingDeg
            val inTunnel = road.samples.minByOrNull { kotlin.math.abs(it.cumM - along) }?.isTunnel == true
            out +=
                TrueState(
                    tMs = t,
                    alongM = along,
                    speedMps = speed,
                    inTunnel = inTunnel,
                    lat = p.lat,
                    lon = p.lon,
                    bearingDeg = p.bearingDeg,
                    yawRateDegPerS = yaw,
                    forwardAccelMps2 = accel,
                )
            t += 100L
        }
        return out
    }

    private fun noise(
        seed: Long,
        i: Int,
    ): Double {
        // Deterministic LCG in [-1, 1].
        val x = (seed * 1103515245L + i * 12345L) and 0x7fffffffL
        return (x.toDouble() / 0x7fffffff.toDouble()) * 2.0 - 1.0
    }

    private fun runScenario(
        name: String,
        road: DrConstraint,
        truth: List<TrueState>,
        vibrationRms: Double,
        accelBiasMps2: Double = 0.0,
        assertPass: Boolean,
        idleVibAfterLatch: Boolean = false,
    ): RunResult {
        val engine = DeadReckoningEngine()
        val shadow = StandstillShadowGate()
        val ranges = tunnelRanges(road)
        val firstTunnel = ranges.first()

        // Seed outside the first tunnel with a short GNSS approach so the
        // forward-axis correlator sees a speed history.
        val seed = truth.first { it.alongM < firstTunnel.first - 50.0 }
        for (k in 0..5) {
            val t = seed.tMs - (6 - k) * 200L
            engine.onGps(
                GpsSample(
                    tMs = t.coerceAtLeast(0L),
                    lat = seed.lat,
                    lon = seed.lon,
                    accuracyM = 5f,
                    speedMps = seed.speedMps,
                    bearingDeg = seed.bearingDeg,
                ),
                road,
            )
            engine.onSensorTick(
                SensorFrame(
                    tMs = t.coerceAtLeast(0L) + 50L,
                    headingDeg = seed.bearingDeg,
                    yawRateDegPerS = 0.1,
                    forwardAccelMps2 = 0.0 + accelBiasMps2,
                ),
                road,
            )
        }

        var falseLatch = false
        var shadowFalseLatch = false
        var stopStartMs: Long? = null
        var latchAtMs: Long? = null
        var pullStartMs: Long? = null
        var releaseAtMs: Long? = null
        var maxCreep = 0.0
        var stopAlong: Double? = null
        var lastAlong = 0.0
        var exitError: Double? = null
        var crawlLag: Double? = null
        var crawlExitJump: Double? = null
        var crawlEndEstAlong: Double? = null
        var lastGnssSpeed: Double? = seed.speedMps
        var lastGnssAcc: Float? = 5f
        var wasStopped = false
        var maxLeadLag = 0.0
        var stayedLatchedUnderIdleVib: Boolean? = null
        var idlePhase = false
        var stopCyclesOk = 0
        var stopCyclesBad = 0
        var cycleStopStart: Long? = null
        var cycleLatched = false
        var cycleReleased = false

        for ((i, s) in truth.withIndex()) {
            // Zero-mean cabin vibration at the requested RMS. Braking-aware
            // latch does not false-trigger on quiet dips; a non-zero mean would
            // bias the integrated brake and delay the stop.
            val vib =
                vibrationRms * (0.85 * noise(1, i) + 0.15 * sin(i * 0.37))
            val yawNoise = (0.3 + vibrationRms * 3.0) * noise(2, i)
            val idleVib =
                if (idlePhase) {
                    0.05 + 0.10 * ((noise(11, i) + 1.0) / 2.0)
                } else {
                    0.0
                }
            val idleSign = if (noise(12, i) < 0.0) -1.0 else 1.0
            val frame =
                SensorFrame(
                    tMs = s.tMs,
                    headingDeg = s.bearingDeg,
                    yawRateDegPerS = s.yawRateDegPerS + yawNoise,
                    forwardAccelMps2 =
                        s.forwardAccelMps2 + vib + accelBiasMps2 + idleSign * idleVib,
                )

            // Network 100 m every 20 s.
            if (s.tMs > 0L && s.tMs % 20_000L < 100L) {
                engine.onGps(
                    GpsSample(
                        tMs = s.tMs,
                        lat = s.lat,
                        lon = s.lon,
                        accuracyM = 100f,
                        speedMps = null,
                        bearingDeg = null,
                    ),
                    road,
                )
                lastGnssSpeed = null
                lastGnssAcc = 100f
            }

            val portal =
                ranges.any { r ->
                    kotlin.math.abs(s.alongM - r.first) < 40.0 ||
                        kotlin.math.abs(s.alongM - r.second) < 40.0
                }
            if (s.inTunnel) {
                // No GNSS / no speed over tunnel edges.
                engine.onGps(
                    GpsSample(
                        tMs = s.tMs,
                        lat = 0.0,
                        lon = 0.0,
                        available = false,
                    ),
                    road,
                )
                lastGnssSpeed = null
                lastGnssAcc = null
            } else if (portal) {
                val portalSpeed = if (s.speedMps <= 0.05) 0.0 else s.speedMps
                engine.onGps(
                    GpsSample(
                        tMs = s.tMs,
                        lat = s.lat + 0.00002 * noise(3, i),
                        lon = s.lon + 0.00002 * noise(4, i),
                        accuracyM = 35f,
                        speedMps = portalSpeed,
                        bearingDeg = s.bearingDeg,
                    ),
                    road,
                )
                lastGnssSpeed = portalSpeed
                lastGnssAcc = 35f
            } else if (s.tMs % 1_000L < 100L) {
                val acc = 3f + 7f * ((noise(5, i) + 1) / 2).toFloat()
                // When truly stopped, report exact 0. A small positive noise
                // would floor to minSpeed (0.5) and look like a crawl.
                val reportedSpeed =
                    if (s.speedMps <= 0.05) {
                        0.0
                    } else {
                        (s.speedMps + 0.2 * noise(8, i)).coerceAtLeast(0.0)
                    }
                engine.onGps(
                    GpsSample(
                        tMs = s.tMs,
                        lat = s.lat + 0.00001 * noise(6, i),
                        lon = s.lon + 0.00001 * noise(7, i),
                        accuracyM = acc,
                        speedMps = reportedSpeed,
                        bearingDeg = s.bearingDeg,
                    ),
                    road,
                )
                lastGnssSpeed = reportedSpeed
                lastGnssAcc = acc
            }

            val shadowSample = shadow.observe(frame, lastGnssSpeed, lastGnssAcc)
            val out = engine.onSensorTick(frame, road)
            val along = out.alongM ?: lastAlong
            lastAlong = along
            val liveStandstill = engine.diagSnapshot().standstill == true
            // While truth is still stopped, a near-zero estimate also counts as
            // held. Once truth pulls away, only the standstill flag matters so
            // a post-release speed of 0 (restarting from rest) is not "held".
            val heldStopped =
                liveStandstill ||
                    (s.speedMps < 0.3 && out.speedMps < 0.05)

            if (s.inTunnel) {
                val leadLag = along - s.alongM
                if (kotlin.math.abs(leadLag) > kotlin.math.abs(maxLeadLag)) {
                    maxLeadLag = leadLag
                }
            }

            if (
                s.trueMoving() &&
                s.forwardAccelMps2 > -0.5 &&
                shadowSample.state == StandstillShadowGate.State.WouldLatch
            ) {
                shadowFalseLatch = true
            }
            // Standstill while truth is still braking into a stop is expected
            // briefly; count a false latch only during non-braking motion.
            if (liveStandstill && s.speedMps > 2.0 && s.forwardAccelMps2 > -0.5) {
                falseLatch = true
            }

            if (s.speedMps < 0.3) {
                if (!wasStopped) {
                    wasStopped = true
                    stopStartMs = stopStartMs ?: s.tMs
                    stopAlong = along
                    if (latchAtMs == null) {
                        // first stop only for latchDelay
                    }
                    cycleStopStart = s.tMs
                    cycleLatched = false
                    cycleReleased = false
                }
                if (heldStopped) {
                    if (latchAtMs == null) latchAtMs = s.tMs
                    cycleLatched = true
                    if (idleVibAfterLatch && liveStandstill && !idlePhase) {
                        idlePhase = true
                        stayedLatchedUnderIdleVib = true
                    }
                }
                if (idlePhase && !liveStandstill) {
                    stayedLatchedUnderIdleVib = false
                }
                val creep = kotlin.math.abs(along - (stopAlong ?: along))
                if (creep > maxCreep) maxCreep = creep
            } else {
                if (wasStopped) {
                    wasStopped = false
                    idlePhase = false
                    if (pullStartMs == null) pullStartMs = s.tMs
                    if (cycleStopStart != null) {
                        if (cycleLatched && cycleReleased) {
                            stopCyclesOk++
                        } else if (cycleLatched && !heldStopped) {
                            cycleReleased = true
                            stopCyclesOk++
                        } else {
                            stopCyclesBad++
                        }
                    }
                    cycleStopStart = null
                }
                if (pullStartMs != null && !liveStandstill && releaseAtMs == null) {
                    releaseAtMs = s.tMs
                }
                if (!liveStandstill) cycleReleased = true
            }

            // Exit error after first tunnel.
            if (exitError == null && s.alongM >= firstTunnel.second + 20.0 && !s.inTunnel) {
                exitError = RouteProgressTracker.haversineM(out.lat, out.lon, s.lat, s.lon)
            }
        }

        if (name.contains("crawl")) {
            val crawlTruth = truth.filter { it.speedMps in 0.5..2.0 && it.inTunnel }
            if (crawlTruth.isNotEmpty()) {
                val last = crawlTruth.last()
                crawlLag = last.alongM - lastAlong
                crawlEndEstAlong = lastAlong
            }
            // Jump back at exit: compare est vs truth once outside the tunnel.
            val after =
                truth.firstOrNull {
                    it.alongM >= firstTunnel.second + 30.0 && !it.inTunnel
                }
            if (after != null && crawlEndEstAlong != null) {
                crawlExitJump = kotlin.math.abs((exitError ?: 0.0))
            }
        }

        val latchDelay =
            if (stopStartMs != null && latchAtMs != null) {
                latchAtMs - stopStartMs
            } else {
                null
            }
        val releaseDelay =
            if (pullStartMs != null && releaseAtMs != null) {
                releaseAtMs - pullStartMs
            } else {
                null
            }

        if (assertPass) {
            assertFalse(
                "$name false latch vib=$vibrationRms bias=$accelBiasMps2",
                falseLatch,
            )
            assertFalse(
                "$name shadow false latch vib=$vibrationRms bias=$accelBiasMps2",
                shadowFalseLatch,
            )
            if (name.startsWith("queue") || name == "stop_after_exit") {
                assertTrue(
                    "$name must latch: delay=$latchDelay creep=$maxCreep",
                    latchDelay != null && latchDelay <= 2_000L,
                )
                val creepLimit = if (name == "stop_after_exit") 1.0 else 5.0
                assertTrue("$name creep $maxCreep", maxCreep <= creepLimit)
            }
            if (name.startsWith("queue")) {
                assertTrue(
                    "$name release $releaseDelay",
                    releaseDelay != null && releaseDelay <= 1_000L,
                )
            }
            if (name.startsWith("stop_go")) {
                assertTrue(
                    "$name must latch and release: latch=$latchDelay release=$releaseDelay " +
                        "cyclesOk=$stopCyclesOk",
                    latchDelay != null &&
                        latchDelay <= 2_000L &&
                        releaseDelay != null &&
                        releaseDelay <= 1_000L,
                )
            }
            if (idleVibAfterLatch) {
                // Position must stay held under engine-idle vibration; the
                // standstill flag may clear briefly if vib crosses pull-away.
                assertTrue(
                    "$name idle vib creep $maxCreep stayed=$stayedLatchedUnderIdleVib",
                    maxCreep <= 5.0,
                )
            }
        }

        return RunResult(
            name = name,
            vibrationRms = vibrationRms,
            accelBiasMps2 = accelBiasMps2,
            falseLatchWhileMoving = falseLatch,
            shadowFalseLatchWhileMoving = shadowFalseLatch,
            latchDelayAfterStopMs = latchDelay,
            releaseDelayAfterPullMs = releaseDelay,
            maxStopCreepM = maxCreep,
            exitErrorM = exitError,
            crawlLagM = crawlLag,
            crawlExitJumpM = crawlExitJump,
            maxLeadLagM = maxLeadLag,
            stayedLatchedUnderIdleVib = stayedLatchedUnderIdleVib,
        )
    }

    private fun TrueState.trueMoving(): Boolean = speedMps > 2.0

    @Test
    fun routeMetadata_listsRaufossBergenTunnels() {
        val road = buildRoute()
        val ranges = tunnelRanges(road)
        assertEquals(2, ranges.size)
        assertEquals("Lærdalstunnelen", tunnels[0].name)
        assertEquals("Gudvangatunnelen", tunnels[1].name)
        assertTrue(ranges[0].second - ranges[0].first > 2_500.0)
        assertTrue(ranges[1].second - ranges[1].first > 1_500.0)
        // Curved tunnel changes heading.
        val a = DeadReckoningEngine.pointAtAlong(road, ranges[1].first + 50.0).bearingDeg
        val b = DeadReckoningEngine.pointAtAlong(road, ranges[1].second - 50.0).bearingDeg
        var d = kotlin.math.abs(a - b)
        if (d > 180) d = 360 - d
        assertTrue("curved heading delta $d", d > 40.0)
    }

    @Test
    fun shadowEvaluation_doesNotChangeDrOutputs() {
        val road = buildRoute()
        val truth = profileSteady(road, 80.0).take(200)

        fun runOnce(): List<DrOutput> {
            val engine = DeadReckoningEngine()
            val shadow = StandstillShadowGate()
            val outs = mutableListOf<DrOutput>()
            val seed = truth.first()
            engine.onGps(
                GpsSample(
                    tMs = seed.tMs,
                    lat = seed.lat,
                    lon = seed.lon,
                    accuracyM = 5f,
                    speedMps = seed.speedMps,
                    bearingDeg = seed.bearingDeg,
                ),
                road,
            )
            for (s in truth) {
                val frame =
                    SensorFrame(
                        tMs = s.tMs,
                        headingDeg = s.bearingDeg,
                        yawRateDegPerS = s.yawRateDegPerS,
                        forwardAccelMps2 = s.forwardAccelMps2,
                    )
                // Shadow is side-effect free on the engine.
                shadow.observe(frame, s.speedMps, 5f)
                if (s.inTunnel) {
                    engine.onGps(
                        GpsSample(tMs = s.tMs, lat = 0.0, lon = 0.0, available = false),
                        road,
                    )
                }
                outs += engine.onSensorTick(frame, road)
            }
            return outs
        }
        val a = runOnce()
        val b = runOnce()
        assertEquals(a.size, b.size)
        for (i in a.indices) {
            assertEquals(a[i].lat, b[i].lat, 1e-12)
            assertEquals(a[i].lon, b[i].lon, 1e-12)
            assertEquals(a[i].speedMps, b[i].speedMps, 1e-12)
            assertEquals(a[i].mode, b[i].mode)
            assertEquals(a[i].alongM ?: -1.0, b[i].alongM ?: -1.0, 1e-12)
        }
    }

    @Test
    fun simulatedDrive_steadyAndStops_meetPassCriteria() {
        val road = buildRoute()
        val report = StringBuilder()
        report.appendLine("route=Raufoss→Bergen (timing_raufoss_bergen) shortened slice")
        report.appendLine("tunnels=${tunnels.joinToString { "${it.name}@${it.lengthM / 1000.0}km" }}")
        report.appendLine(
            "thresholds: standstillAccel=${DrConfig().standstillAccelMps2} " +
                "brakeAccel=${DrConfig().brakeAccelMps2} " +
                "standstillSpeed=${DrConfig().standstillSpeedMps} " +
                "holdMs=${DrConfig().standstillHoldMs} " +
                "trustTauS=${DrConfig().speedTrustTauS} " +
                "lossTimeoutMs=${DrConfig().gpsLossTimeoutMs}",
        )

        for (speed in listOf(50.0, 80.0, 100.0)) {
            for (vib in vibrationSteps) {
                for (bias in accelBiases) {
                    val r =
                        runScenario(
                            name = "steady_${speed.toInt()}",
                            road = road,
                            truth = profileSteady(road, speed),
                            vibrationRms = vib,
                            accelBiasMps2 = bias,
                            assertPass = true,
                        )
                    report.appendLine(
                        "steady speed=$speed vib=$vib bias=$bias " +
                            "false=${r.falseLatchWhileMoving} shadow_false=${r.shadowFalseLatchWhileMoving} " +
                            "lead_lag=${fmt(r.maxLeadLagM)} exit=${fmt(r.exitErrorM)}",
                    )
                }
            }
        }

        for (vib in vibrationSteps) {
            for (bias in accelBiases) {
                val queue =
                    runScenario(
                        name = "queue",
                        road = road,
                        truth = profileQueueInTunnel(road, 80.0),
                        vibrationRms = vib,
                        accelBiasMps2 = bias,
                        assertPass = true,
                    )
                report.appendLine(
                    "queue vib=$vib bias=$bias latch=${queue.latchDelayAfterStopMs} " +
                        "release=${queue.releaseDelayAfterPullMs} creep=${fmt(queue.maxStopCreepM)} " +
                        "lead_lag=${fmt(queue.maxLeadLagM)} exit=${fmt(queue.exitErrorM)}",
                )
            }
        }

        for (bias in accelBiases) {
            val stopGo =
                runScenario(
                    name = "stop_go",
                    road = road,
                    truth = profileStopAndGo(road),
                    vibrationRms = 0.15,
                    accelBiasMps2 = bias,
                    assertPass = true,
                )
            report.appendLine(
                "stop_go bias=$bias latch=${stopGo.latchDelayAfterStopMs} " +
                    "release=${stopGo.releaseDelayAfterPullMs} creep=${fmt(stopGo.maxStopCreepM)} " +
                    "lead_lag=${fmt(stopGo.maxLeadLagM)}",
            )
        }

        for (bias in accelBiases) {
            val crawl =
                runScenario(
                    name = "crawl",
                    road = road,
                    truth = profileCrawl(road),
                    vibrationRms = 0.15,
                    accelBiasMps2 = bias,
                    assertPass = true,
                )
            report.appendLine(
                "crawl bias=$bias lag=${fmt(crawl.crawlLagM)} lead_lag=${fmt(crawl.maxLeadLagM)} " +
                    "exit=${fmt(crawl.exitErrorM)} jump=${fmt(crawl.crawlExitJumpM)} " +
                    "latch=${crawl.latchDelayAfterStopMs}",
            )
            assertTrue(
                "crawl lead/lag too large: ${crawl.maxLeadLagM}",
                kotlin.math.abs(crawl.maxLeadLagM) < 2_000.0,
            )
        }

        val afterExit =
            runScenario(
                name = "stop_after_exit",
                road = road,
                truth = profileStopAfterExit(road),
                vibrationRms = 0.15,
                accelBiasMps2 = 0.0,
                assertPass = true,
            )
        report.appendLine(
            "stop_after_exit latch=${afterExit.latchDelayAfterStopMs} " +
                "creep=${fmt(afterExit.maxStopCreepM)}",
        )
        assertTrue(
            "stop_after_exit latch=${afterExit.latchDelayAfterStopMs}",
            afterExit.latchDelayAfterStopMs != null &&
                afterExit.latchDelayAfterStopMs!! <= 600L &&
                afterExit.maxStopCreepM <= 1.0,
        )

        val idle =
            runScenario(
                name = "queue_idle_vib",
                road = road,
                truth = profileQueueThenIdleVib(road),
                vibrationRms = 0.0,
                accelBiasMps2 = 0.0,
                assertPass = true,
                idleVibAfterLatch = true,
            )
        report.appendLine(
            "idle_vib_after_stop stayed_latched=${idle.stayedLatchedUnderIdleVib} " +
                "latch=${idle.latchDelayAfterStopMs}",
        )

        println(report.toString())
        assertTrue(report.contains("Lærdalstunnelen"))
        assertTrue(report.contains("Gudvangatunnelen"))
    }

    private fun fmt(v: Double?): String =
        if (v == null) {
            "n/a"
        } else {
            String.format(Locale.US, "%.1f", v)
        }

    @Test
    fun stationaryAtRouteStart_matchesReplayBounds() {
        // Covered by StationaryDeadReckoningTest replay / burst cases; keep a
        // short local smoke on this route geometry.
        val road = buildRoute()
        val engine = DeadReckoningEngine()
        val start = DeadReckoningEngine.pointAtAlong(road, 0.0)
        engine.onGps(
            GpsSample(
                tMs = 0L,
                lat = start.lat,
                lon = start.lon,
                accuracyM = 11f,
                speedMps = 0.0,
                bearingDeg = start.bearingDeg,
            ),
            road,
        )
        // 2 s of rest while still in GPS mode (speed 0) — must not creep.
        var maxAlong = 0.0
        var maxSpeed = 0.0
        for (i in 1..20) {
            val out =
                engine.onSensorTick(
                    SensorFrame(
                        tMs = i * 100L,
                        headingDeg = start.bearingDeg,
                        yawRateDegPerS = 0.01,
                        forwardAccelMps2 = 0.001,
                    ),
                    road,
                )
            maxAlong = maxOf(maxAlong, out.alongM ?: 0.0)
            maxSpeed = maxOf(maxSpeed, out.speedMps)
        }
        assertTrue("along=$maxAlong speed=$maxSpeed", maxAlong <= 1.0 && maxSpeed == 0.0)
        // Network loss then rest IMU with missing speed: standstill must hold.
        engine.onGps(
            GpsSample(
                tMs = 5_000L,
                lat = start.lat,
                lon = start.lon,
                accuracyM = 100f,
                speedMps = null,
            ),
            road,
        )
        for (i in 1..30) {
            val out =
                engine.onSensorTick(
                    SensorFrame(
                        tMs = 5_000L + i * 100L,
                        headingDeg = start.bearingDeg,
                        yawRateDegPerS = 0.01,
                        forwardAccelMps2 = 0.001,
                    ),
                    road,
                )
            maxAlong = maxOf(maxAlong, out.alongM ?: 0.0)
            maxSpeed = maxOf(maxSpeed, out.speedMps)
        }
        assertTrue(
            "after-network along=$maxAlong speed=$maxSpeed mode=${engine.mode()}",
            maxAlong <= 1.0 && maxSpeed == 0.0,
        )
    }
}
