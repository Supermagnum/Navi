package no.navi.app

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Stationary dead reckoning. Synthetic routes only, except the replay test,
 * which feeds the pulled tablet log through [DeadReckoningEngine].
 */
class StationaryDeadReckoningTest {
    private data class Trace(
        var maxAlongM: Double = 0.0,
        var maxSpeedMps: Double = 0.0,
        var minAlongAfterPeakM: Double = 0.0,
    ) {
        fun note(out: DrOutput) {
            val along = out.alongM ?: 0.0
            if (along > maxAlongM) {
                maxAlongM = along
                minAlongAfterPeakM = along
            } else if (along < minAlongAfterPeakM) {
                minAlongAfterPeakM = along
            }
            if (out.speedMps > maxSpeedMps) maxSpeedMps = out.speedMps
        }

        fun dropM(): Double = maxAlongM - minAlongAfterPeakM
    }

    private fun routeAt(speedKmh: Double): DrConstraint {
        val samples = mutableListOf<RouteSimSample>()
        val startLat = 60.0
        val startLon = 10.0
        for (i in 0..40) {
            val cum = i * 50.0
            val p = DeadReckoningEngine.destinationPoint(startLat, startLon, 0.0, cum)
            samples +=
                RouteSimSample(
                    lat = p.first,
                    lon = p.second,
                    cumM = cum,
                    speedKmh = speedKmh,
                    highway = "residential",
                    maxspeedPosted = true,
                    maxspeedKmh = speedKmh,
                )
        }
        return DrConstraint(samples)
    }

    private fun rest(tMs: Long): SensorFrame =
        SensorFrame(
            tMs = tMs,
            headingDeg = 0.0,
            yawRateDegPerS = 0.01,
            forwardAccelMps2 = 0.001,
        )

    private fun moving(tMs: Long): SensorFrame =
        SensorFrame(
            tMs = tMs,
            headingDeg = 0.0,
            yawRateDegPerS = 15.0,
            // Real longitudinal accel so DR integrates speed (planned speed is
            // only an upper bound / no-IMU fallback after the braking-aware fix).
            forwardAccelMps2 = 1.2,
        )

    private fun fix(
        tMs: Long,
        road: DrConstraint,
        accuracyM: Float,
        speedMps: Double?,
        alongM: Double = 0.0,
    ): GpsSample {
        val p = DeadReckoningEngine.pointAtAlong(road, alongM)
        return GpsSample(
            tMs = tMs,
            lat = p.lat,
            lon = p.lon,
            accuracyM = accuracyM,
            speedMps = speedMps,
            bearingDeg = p.bearingDeg,
        )
    }

    private fun tickRange(
        engine: DeadReckoningEngine,
        road: DrConstraint,
        fromExclusive: Long,
        toInclusive: Long,
        frame: (Long) -> SensorFrame,
        trace: Trace,
    ) {
        var t = fromExclusive + 100L
        while (t <= toInclusive) {
            trace.note(engine.onSensorTick(frame(t), road))
            t += 100L
        }
    }

    @Test
    fun burst_networkThenGps_doesNotAdvanceOrShowPlannedSpeed() {
        val engine = DeadReckoningEngine()
        val road = routeAt(30.0)
        val trace = Trace()
        trace.note(engine.onGps(fix(0L, road, 11f, 0.0), road))
        repeat(3) { cycle ->
            val networkAt = 20_000L + cycle * 20_000L
            tickRange(engine, road, networkAt - 20_000L, networkAt - 100L, ::rest, trace)
            trace.note(engine.onGps(fix(networkAt, road, 100f, null), road))
            trace.note(engine.onSensorTick(rest(networkAt + 100L), road))
            trace.note(engine.onGps(fix(networkAt + 100L, road, 11f, 0.0), road))
            tickRange(engine, road, networkAt + 100L, networkAt + 1_000L, ::rest, trace)
        }
        assertTrue(
            "burst maxAlong=${trace.maxAlongM} maxSpeed=${trace.maxSpeedMps}",
            trace.maxAlongM <= 1.0 && trace.maxSpeedMps == 0.0,
        )
    }

    @Test
    fun lateGps_doesNotCreepThenBlendBack() {
        val engine = DeadReckoningEngine()
        val road = routeAt(30.0)
        val trace = Trace()
        trace.note(engine.onGps(fix(0L, road, 11f, 0.0), road))
        val networkAt = 20_000L
        val gpsAt = 40_000L
        trace.note(engine.onGps(fix(networkAt, road, 100f, null), road))
        tickRange(engine, road, networkAt, gpsAt - 100L, ::rest, trace)
        trace.note(engine.onGps(fix(gpsAt, road, 11f, 0.0), road))
        tickRange(engine, road, gpsAt, gpsAt + 1_000L, ::rest, trace)
        assertTrue(
            "late maxAlong=${trace.maxAlongM} drop=${trace.dropM()} maxSpeed=${trace.maxSpeedMps}",
            trace.maxAlongM <= 1.0 && trace.dropM() <= 1.0 && trace.maxSpeedMps == 0.0,
        )
    }

    @Test
    fun tunnelStandstill_missingSpeedAndRestImu_doesNotMove() {
        val engine = DeadReckoningEngine()
        val road = routeAt(30.0)
        val trace = Trace()
        trace.note(engine.onGps(fix(0L, road, 100f, null), road))
        trace.note(engine.onGps(fix(2_500L, road, 100f, null), road))
        assertEquals(DrMode.Estimating, engine.mode())
        tickRange(engine, road, 2_500L, 12_500L, ::rest, trace)
        assertTrue(
            "tunnel-still maxAlong=${trace.maxAlongM} maxSpeed=${trace.maxSpeedMps}",
            trace.maxAlongM <= 1.0 && trace.maxSpeedMps == 0.0,
        )
    }

    @Test
    fun tunnelMotion_missingSpeedAndMovingImu_usesPlannedSpeed() {
        val engine = DeadReckoningEngine()
        val road = routeAt(30.0)
        engine.onGps(fix(0L, road, 100f, null), road)
        engine.onGps(fix(2_500L, road, 100f, null), road)
        assertEquals(DrMode.Estimating, engine.mode())
        var along = 0.0
        var speed = 0.0
        var t = 2_600L
        while (t <= 12_500L) {
            val out = engine.onSensorTick(moving(t), road)
            along = out.alongM ?: along
            speed = out.speedMps
            t += 100L
        }
        assertTrue("along=$along speed=$speed", along > 50.0 && speed > 5.0)
        // Planned 30 km/h is an upper bound, not a driver.
        assertTrue("speed capped by planned: $speed", speed <= 30.0 / 3.6 + 0.5)
    }

    @Test
    fun stopAfterDriving_speedDropsToZeroWithoutCreep() {
        val engine = DeadReckoningEngine()
        val road = routeAt(30.0)
        engine.onGps(fix(0L, road, 8f, 8.0), road)
        val stopped = engine.onGps(fix(5_000L, road, 8f, 0.0), road)
        val trace = Trace()
        trace.note(stopped)
        tickRange(engine, road, 5_000L, 7_900L, ::rest, trace)
        trace.note(engine.onGps(fix(8_000L, road, 100f, null), road))
        tickRange(engine, road, 8_000L, 15_000L, ::rest, trace)
        assertTrue(
            "stop maxAlong=${trace.maxAlongM} maxSpeed=${trace.maxSpeedMps} mode=${engine.mode()}",
            trace.maxAlongM <= 1.0 && trace.maxSpeedMps == 0.0,
        )
        assertTrue(stopped.speedMps == 0.0)
    }

    @Test
    fun replayPulledTabletLog_staysAtRouteStartWithZeroSpeed() {
        val stats = replayTabletLog()
        assertTrue(
            "replay maxAlong=${stats.maxAlongM} maxSpeed=${stats.maxSpeedMps}",
            stats.maxAlongM <= 1.0 && stats.maxSpeedMps == 0.0,
        )
    }

    private fun replayTabletLog(): Trace {
        val loader = javaClass.classLoader ?: error("test class loader")
        val stream =
            loader.getResourceAsStream("location-diag-replay.jsonl")
                ?: error("missing location-diag-replay.jsonl")
        val events = mutableListOf<JSONObject>()
        stream.bufferedReader().useLines { lines ->
            lines.forEach { line ->
                if (line.isNotBlank()) events += JSONObject(line)
            }
        }
        var road: DrConstraint? = null
        var accel = 0.001
        var yawDeg = 0.01
        var haveImu = false
        val engine = DeadReckoningEngine()
        val trace = Trace()
        val timed =
            events.mapNotNull { row ->
                val wall = row.optLong("wall_ms", -1L)
                if (wall < 0L) null else wall to row
            }
        if (timed.isEmpty()) error("replay log has no timestamps")
        var index = 0
        var t = timed.first().first
        val end = timed.last().first
        while (t <= end) {
            while (index < timed.size && timed[index].first <= t) {
                val row = timed[index].second
                when (row.optString("kind")) {
                    "route" -> road = constraintFromRoute(row)
                    "fix" -> trace.note(engine.onGps(sampleFromFix(row), road))
                    "imu" -> {
                        accel = row.optDouble("mean_ax", 0.0)
                        yawDeg = Math.toDegrees(row.optDouble("mean_gz", 0.0))
                        haveImu = true
                    }
                }
                index += 1
            }
            val frame =
                if (haveImu) {
                    SensorFrame(
                        tMs = t,
                        headingDeg = 0.0,
                        yawRateDegPerS = yawDeg,
                        forwardAccelMps2 = accel,
                    )
                } else {
                    SensorFrame(tMs = t)
                }
            trace.note(engine.onSensorTick(frame, road))
            t += 100L
        }
        return trace
    }

    private fun sampleFromFix(row: JSONObject): GpsSample {
        val speed = if (row.isNull("speed_mps")) null else row.optDouble("speed_mps")
        val accuracy = if (row.isNull("accuracy_m")) null else row.optDouble("accuracy_m").toFloat()
        return GpsSample(
            tMs = row.optLong("wall_ms"),
            lat = row.optDouble("lat"),
            lon = row.optDouble("lon"),
            accuracyM = accuracy,
            speedMps = speed,
            bearingDeg = if (row.isNull("bearing_deg")) null else row.optDouble("bearing_deg"),
        )
    }

    private fun constraintFromRoute(row: JSONObject): DrConstraint {
        val samplesJson = row.getJSONArray("samples")
        val samples = mutableListOf<RouteSimSample>()
        for (i in 0 until samplesJson.length()) {
            val sample = samplesJson.getJSONObject(i)
            samples +=
                RouteSimSample(
                    lat = sample.getDouble("lat"),
                    lon = sample.getDouble("lon"),
                    cumM = sample.getDouble("cum_m"),
                    speedKmh = sample.optDouble("speed_kmh", 30.0),
                    highway = "residential",
                    maxspeedPosted = true,
                    maxspeedKmh = sample.optDouble("speed_kmh", 30.0),
                )
        }
        return DrConstraint(samples)
    }
}
