package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.cos
import kotlin.math.sin

/**
 * Device-frame forward axis from GNSS speeding-up and braking.
 * The fit is kept when dead reckoning starts.
 */
class ForwardAxisFitTest {
    @Test
    fun fit_coversYawPitchAndOrientation_andSurvivesDrEntry() {
        val yaws = listOf(0.0, 45.0, 90.0, 180.0)
        val pitches = listOf(0.0, 30.0)
        val landscapes = listOf(false, true)
        for (yaw in yaws) {
            for (pitch in pitches) {
                for (landscape in landscapes) {
                    val engine = DeadReckoningEngine()
                    val mount = mountAxes(yaw, pitch, landscape)
                    train(engine, mount, longAccel = 1.2)
                    train(engine, mount, longAccel = -1.2)
                    val label = "yaw=$yaw pitch=$pitch landscape=$landscape"
                    assertEquals(label, "confident", engine.forwardAxisState())
                    val fitted = engine.forwardAxisDevice()
                    assertNotNull(label, fitted)
                    val expected = horizontalForward(mount)
                    val dot =
                        fitted!![0] * expected[0] +
                            fitted[1] * expected[1] +
                            fitted[2] * expected[2]
                    assertTrue("$label dot=$dot", dot > 0.9)

                    engine.onGps(
                        GpsSample(tMs = 40_000L, lat = 0.0, lon = 0.0, available = false),
                        null,
                    )
                    engine.onGps(
                        GpsSample(tMs = 43_000L, lat = 0.0, lon = 0.0, available = false),
                        null,
                    )
                    assertEquals("DR entry keeps axis $label", "confident", engine.forwardAxisState())
                    assertTrue(engine.forwardAxisConfident())
                }
            }
        }
    }

    @Test
    fun learning_holdsSpeedAndDoesNotDetectBraking() {
        val engine = DeadReckoningEngine()
        val mount = mountAxes(yawDeg = 90.0, pitchDeg = 30.0, landscape = true)
        // One speeding-up sample is not enough (needs accel and brake).
        train(engine, mount, longAccel = 1.5, samples = 2)
        assertEquals("learning", engine.forwardAxisState())
        assertFalse(engine.forwardAxisConfident())

        val seedT = 20_000L
        engine.onGps(
            GpsSample(
                tMs = seedT,
                lat = 59.9,
                lon = 10.7,
                accuracyM = 5f,
                speedMps = 20.0,
                bearingDeg = 0.0,
            ),
            null,
        )
        engine.onGps(GpsSample(tMs = seedT + 2_500L, lat = 0.0, lon = 0.0, available = false), null)
        val before = engine.diagSnapshot().speedMps
        val frame = deviceFrame(seedT + 3_000L, mount, longAccel = -3.0)
        engine.onSensorTick(frame, null)
        assertEquals("learning", engine.forwardAxisState())
        assertFalse(engine.brakeSeenSinceLoss())
        assertEquals(before, engine.diagSnapshot().speedMps, 0.05)
        assertTrue(engine.diagSnapshot().speedUncertaintyMps > 0.0)
    }

    private fun train(
        engine: DeadReckoningEngine,
        mount: Mount,
        longAccel: Double,
        samples: Int = 6,
    ) {
        var speed = 15.0
        var t = if (longAccel > 0) 1_000L else 12_000L
        engine.onGps(
            GpsSample(
                tMs = t,
                lat = 59.9,
                lon = 10.7,
                accuracyM = 5f,
                speedMps = speed,
                bearingDeg = 0.0,
            ),
            null,
        )
        engine.onSensorTick(deviceFrame(t, mount, longAccel), null)
        repeat(samples) {
            t += 1_000L
            speed = (speed + longAccel).coerceAtLeast(1.0)
            engine.onSensorTick(deviceFrame(t, mount, longAccel), null)
            engine.onGps(
                GpsSample(
                    tMs = t,
                    lat = 59.9,
                    lon = 10.7,
                    accuracyM = 5f,
                    speedMps = speed,
                    bearingDeg = 0.0,
                ),
                null,
            )
        }
    }

    private fun deviceFrame(
        tMs: Long,
        mount: Mount,
        longAccel: Double,
    ): SensorFrame {
        val lin = DoubleArray(3) { i -> longAccel * mount.forward[i] }
        return SensorFrame(
            tMs = tMs,
            linAccelX = lin[0],
            linAccelY = lin[1],
            linAccelZ = lin[2],
            gravityX = mount.gravity[0],
            gravityY = mount.gravity[1],
            gravityZ = mount.gravity[2],
        )
    }

    private data class Mount(
        val forward: DoubleArray,
        val gravity: DoubleArray,
    )

    /** Device axes in world (x right, y forward, z up). Portrait +Y is forward at yaw 0. */
    private fun mountAxes(
        yawDeg: Double,
        pitchDeg: Double,
        landscape: Boolean,
    ): Mount {
        val yaw = Math.toRadians(if (landscape) yawDeg - 90.0 else yawDeg)
        val pitch = Math.toRadians(pitchDeg)
        val cy = cos(yaw)
        val sy = sin(yaw)
        val cp = cos(pitch)
        val sp = sin(pitch)
        // device X, Y, Z in world, then pitch about device X.
        val xw = doubleArrayOf(cy, -sy, 0.0)
        val ywFlat = doubleArrayOf(sy, cy, 0.0)
        val zwFlat = doubleArrayOf(0.0, 0.0, 1.0)
        val yw = doubleArrayOf(ywFlat[0] * cp + zwFlat[0] * sp, ywFlat[1] * cp + zwFlat[1] * sp, sp)
        val zw =
            doubleArrayOf(
                -ywFlat[0] * sp + zwFlat[0] * cp,
                -ywFlat[1] * sp + zwFlat[1] * cp,
                cp,
            )
        val gWorld = doubleArrayOf(0.0, 0.0, 9.81)
        val gravity =
            doubleArrayOf(
                dot(gWorld, xw),
                dot(gWorld, yw),
                dot(gWorld, zw),
            )
        val forwardWorld = doubleArrayOf(0.0, 1.0, 0.0)
        val forward =
            doubleArrayOf(
                dot(forwardWorld, xw),
                dot(forwardWorld, yw),
                dot(forwardWorld, zw),
            )
        return Mount(forward, gravity)
    }

    private fun horizontalForward(mount: Mount): DoubleArray {
        val g2 =
            mount.gravity[0] * mount.gravity[0] +
                mount.gravity[1] * mount.gravity[1] +
                mount.gravity[2] * mount.gravity[2]
        val inv = 1.0 / kotlin.math.sqrt(g2)
        val nx = mount.gravity[0] * inv
        val ny = mount.gravity[1] * inv
        val nz = mount.gravity[2] * inv
        val d = mount.forward[0] * nx + mount.forward[1] * ny + mount.forward[2] * nz
        val hx = mount.forward[0] - d * nx
        val hy = mount.forward[1] - d * ny
        val hz = mount.forward[2] - d * nz
        val m = kotlin.math.sqrt(hx * hx + hy * hy + hz * hz)
        return doubleArrayOf(hx / m, hy / m, hz / m)
    }

    private fun dot(
        a: DoubleArray,
        b: DoubleArray,
    ): Double = a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
