package no.navi.app

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorManager
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.ImuCallKind
import uniffi.navi.imuPluginConfigure
import uniffi.navi.imuPluginInstallGuest
import uniffi.navi.imuPluginIsEnabled
import uniffi.navi.imuPluginReleaseSensors
import uniffi.navi.imuPluginRequestCalibrate
import uniffi.navi.imuPluginSetEnabled
import uniffi.navi.imuPluginTick
import java.io.File

/**
 * Real `SensorManager` + staged wasm guest + native session on a device or emulator
 * (Extended controls > Virtual sensors on an emulator). Uses a private files dir so
 * the app's own plugin state is untouched.
 */
@RunWith(AndroidJUnit4::class)
class ImuPluginInstrumentedTest {
    private val ctx = InstrumentationRegistry.getInstrumentation().targetContext

    private fun sessionDir(): File = File(ctx.cacheDir, "imu-instrumented").also { it.deleteRecursively() }

    private fun backend() = ImuSensorBackend(AndroidSensorSource(ctx))

    private fun tickReadout(previous: ImuReadout?): ImuReadout? {
        Thread.sleep(TICK_MS)
        val r = imuPluginTick()
        assertEquals(r.message, ImuCallKind.OK, r.kind)
        return ImuReadoutParser.parse(r.stateJson) ?: previous
    }

    @Test
    fun ambientTemperatureResolvesToThePreferredPlatformType() {
        val sm = ctx.getSystemService(Context.SENSOR_SERVICE) as SensorManager
        val resolved = backend().resolveAndroidType(AMBIENT_TEMPERATURE_CODE)
        if (sm.getDefaultSensor(Sensor.TYPE_AMBIENT_TEMPERATURE) != null) {
            assertEquals(Sensor.TYPE_AMBIENT_TEMPERATURE, resolved)
        } else {
            // Falls back to the deprecated type only when the device has it; otherwise unavailable.
            @Suppress("DEPRECATION")
            val legacy = sm.getDefaultSensor(Sensor.TYPE_TEMPERATURE) != null
            assertEquals(legacy, resolved != null)
        }
    }

    @Test
    fun guestRunsCalibratesAndReleasesSensorsOnRealHardware() {
        val backend = backend()
        assumeTrue(
            "needs gravity or accelerometer",
            backend.isSensorAvailable(GRAVITY_CODE) || backend.isSensorAvailable(ACCELEROMETER_CODE),
        )
        val dir = sessionDir()
        assertEquals("OK", imuPluginConfigure(dir.absolutePath, backend))
        val manifest = ctx.assets.open("plugins/imu/plugin.json").use { it.readBytes().decodeToString() }
        val wasm = ctx.assets.open("plugins/imu/plugin.wasm").use { it.readBytes() }
        assertEquals("OK", imuPluginInstallGuest(manifest, wasm))
        try {
            assertFalse("plugin is off by default", imuPluginIsEnabled())
            assertTrue(imuPluginSetEnabled(true).startsWith("OK"))

            var readout: ImuReadout? = null
            repeat(WARMUP_TICKS) { readout = tickReadout(readout) }
            val warm = readout
            assertNotNull("guest produced a readout", warm)
            assertTrue(warm!!.sensors.gravity || warm.sensors.accelerometer)
            assertNotNull(warm.pitchDeg)

            // Calibrate: accepted on a still device, rejected with a notice when moving; never silent.
            assertEquals("OK", imuPluginRequestCalibrate())
            var outcome: ImuReadout? = null
            for (i in 0 until CALIBRATE_TICKS) {
                outcome = tickReadout(outcome)
                if (outcome?.calibrated == true || outcome?.noticeText != null) break
            }
            assertTrue(
                "calibrate produced a result or notice: $outcome",
                outcome?.calibrated == true || outcome?.noticeText != null,
            )

            imuPluginReleaseSensors()
            assertTrue("sensors unregistered", backend.registeredAndroidTypes().isEmpty())
        } finally {
            imuPluginSetEnabled(false)
            imuPluginReleaseSensors()
        }
    }

    private companion object {
        // navi_plugin_host::SensorKind wire codes.
        const val ACCELEROMETER_CODE = 1u
        const val AMBIENT_TEMPERATURE_CODE = 2u
        const val GRAVITY_CODE = 3u
        const val TICK_MS = 125L
        const val WARMUP_TICKS = 20
        const val CALIBRATE_TICKS = 60
    }
}
