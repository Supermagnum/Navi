package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ImuSensorBackendTest {
    private class FakeSource(
        val present: Set<Int>,
    ) : SensorSource {
        val registered = mutableListOf<Pair<Int, Int>>()
        val unregistered = mutableListOf<Int>()
        var failRegistration = false

        override fun hasSensor(androidType: Int) = androidType in present

        override fun register(
            androidType: Int,
            samplingPeriodUs: Int,
            sink: SensorSink,
        ): Boolean {
            if (failRegistration) return false
            registered += androidType to samplingPeriodUs
            return true
        }

        override fun unregister(androidType: Int) {
            unregistered += androidType
        }
    }

    // Wire codes of navi_plugin_host::SensorKind.
    private val accelerometer = 1u
    private val ambientTemperature = 2u
    private val gravity = 3u

    private val table: (UInt) -> List<Int> = {
        when (it) {
            accelerometer -> listOf(1)
            ambientTemperature -> listOf(TYPE_AMBIENT_TEMPERATURE, TYPE_TEMPERATURE)
            gravity -> listOf(9)
            else -> emptyList()
        }
    }

    private fun backend(source: SensorSource) = ImuSensorBackend(source, table)

    @Test
    fun ambientTemperaturePrefersTypeAmbientTemperature() {
        val both = backend(FakeSource(setOf(TYPE_AMBIENT_TEMPERATURE, TYPE_TEMPERATURE)))
        assertEquals(TYPE_AMBIENT_TEMPERATURE, both.resolveAndroidType(ambientTemperature))
    }

    @Test
    fun ambientTemperatureFallsBackToDeprecatedTypeOnlyWhenNeeded() {
        val legacyOnly = backend(FakeSource(setOf(TYPE_TEMPERATURE)))
        assertEquals(TYPE_TEMPERATURE, legacyOnly.resolveAndroidType(ambientTemperature))
        assertTrue(legacyOnly.isSensorAvailable(ambientTemperature))
    }

    @Test
    fun availabilityIsFalseForMissingSensorsAndUnknownCodes() {
        val b = backend(FakeSource(setOf(9)))
        assertTrue(b.isSensorAvailable(gravity))
        assertFalse(b.isSensorAvailable(accelerometer))
        assertFalse(b.isSensorAvailable(ambientTemperature))
        assertFalse(b.isSensorAvailable(99u))
        assertFalse(b.registerSensor(accelerometer, 100_000u))
    }

    @Test
    fun registrationPassesThePeriodAndIsIdempotent() {
        val source = FakeSource(setOf(9))
        val b = backend(source)
        assertTrue(b.registerSensor(gravity, 100_000u))
        assertTrue(b.registerSensor(gravity, 100_000u))
        assertEquals(listOf(9 to 100_000), source.registered)
        assertEquals(setOf(9), b.registeredAndroidTypes())
    }

    @Test
    fun failedPlatformRegistrationIsReported() {
        val source = FakeSource(setOf(9)).also { it.failRegistration = true }
        val b = backend(source)
        assertFalse(b.registerSensor(gravity, 100_000u))
        assertTrue(b.registeredAndroidTypes().isEmpty())
    }

    @Test
    fun eventsAreRateLimitedToTheRequestedPeriod() {
        val b = backend(FakeSource(setOf(9)))
        b.registerSensor(gravity, 100_000u)
        b.onSensorChanged(9, floatArrayOf(0f, 0f, 9.8f), 1_000_000_000L, 3)
        // 10 ms later: dropped.
        b.onSensorChanged(9, floatArrayOf(1f, 1f, 1f), 1_010_000_000L, 3)
        assertEquals(1_000L, b.latestSensorSample(gravity)!!.timestampMs)
        // 100 ms later: accepted.
        b.onSensorChanged(9, floatArrayOf(0f, 0.5f, 9.7f), 1_100_000_000L, 2)
        val s = b.latestSensorSample(gravity)!!
        assertEquals(1_100L, s.timestampMs)
        assertEquals(2, s.accuracy)
        assertEquals(listOf(0.0, 0.5, 9.7f.toDouble()), s.values)
    }

    @Test
    fun unknownAccuracyIsReportedAsMinusOne() {
        val b = backend(FakeSource(setOf(9)))
        b.registerSensor(gravity, 100_000u)
        b.onSensorChanged(9, floatArrayOf(0f, 0f, 9.8f), 5_000_000_000L, 42)
        assertEquals(-1, b.latestSensorSample(gravity)!!.accuracy)
    }

    @Test
    fun unregisterReleasesThePlatformListenerAndDropsTheSample() {
        val source = FakeSource(setOf(9))
        val b = backend(source)
        b.registerSensor(gravity, 100_000u)
        b.onSensorChanged(9, floatArrayOf(0f, 0f, 9.8f), 1_000_000_000L, 3)
        b.unregisterSensor(gravity)
        assertEquals(listOf(9), source.unregistered)
        assertNull(b.latestSensorSample(gravity))
        assertTrue(b.registeredAndroidTypes().isEmpty())
        // Late events after unregistration are ignored.
        b.onSensorChanged(9, floatArrayOf(0f, 0f, 9.8f), 2_000_000_000L, 3)
        assertNull(b.latestSensorSample(gravity))
        // Unregistering something never registered does not touch the platform.
        b.unregisterSensor(gravity)
        assertEquals(1, source.unregistered.size)
    }

    private companion object {
        // android.hardware.Sensor constants (stubbed to 0 in host JVM tests, so spelled out).
        const val TYPE_AMBIENT_TEMPERATURE = 13
        const val TYPE_TEMPERATURE = 7
    }
}
