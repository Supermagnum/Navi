package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class DisplayUnitsTest {
    @Test
    fun distanceMetricAndUsImperial() {
        assertEquals("250 m", DisplayUnits.formatDistanceM(250.0, UnitSystem.METRIC))
        assertEquals("3.2 km", DisplayUnits.formatDistanceM(3200.0, UnitSystem.METRIC))
        assertEquals("328 ft", DisplayUnits.formatDistanceM(100.0, UnitSystem.IMPERIAL_US))
        assertEquals("2.0 mi", DisplayUnits.formatDistanceM(3200.0, UnitSystem.IMPERIAL_US))
    }

    @Test
    fun distanceUkUsesYardsThenMiles() {
        assertEquals("109 yd", DisplayUnits.formatDistanceM(100.0, UnitSystem.IMPERIAL_UK))
        assertEquals("2.0 mi", DisplayUnits.formatDistanceM(3200.0, UnitSystem.IMPERIAL_UK))
        assertEquals("0.2 mi", DisplayUnits.formatDistanceM(250.0, UnitSystem.IMPERIAL_UK))
    }

    @Test
    fun speedMetricAndImperial() {
        assertEquals("80 km/h", DisplayUnits.formatSpeedKmh(80.0, UnitSystem.METRIC))
        assertEquals("50 mph", DisplayUnits.formatSpeedKmh(80.0, UnitSystem.IMPERIAL_US))
        assertEquals("50 mph", DisplayUnits.formatSpeedKmh(80.0, UnitSystem.IMPERIAL_UK))
        assertEquals("km/h", DisplayUnits.speedUnit(UnitSystem.METRIC))
        assertEquals("mph", DisplayUnits.speedUnit(UnitSystem.IMPERIAL_UK))
    }

    @Test
    fun altitudeUsFeetUkMetres() {
        // UK altitude is metres on purpose (not US feet). See UnitSystem.IMPERIAL_UK.
        assertEquals("412 m", DisplayUnits.formatAltitudeM(412.0, UnitSystem.METRIC))
        assertEquals("1352 ft", DisplayUnits.formatAltitudeM(412.0, UnitSystem.IMPERIAL_US))
        assertEquals("412 m", DisplayUnits.formatAltitudeM(412.0, UnitSystem.IMPERIAL_UK))
    }

    @Test
    fun routeAndDayCardDistances() {
        assertEquals("12.3 km", DisplayUnits.formatDistanceKm(12.3, UnitSystem.METRIC))
        assertEquals("7.6 mi", DisplayUnits.formatDistanceKm(12.3, UnitSystem.IMPERIAL_US))
        assertEquals("7.6 mi", DisplayUnits.formatDistanceKm(12.3, UnitSystem.IMPERIAL_UK))
        assertEquals("10–20 km", DisplayUnits.formatDistanceKmRange(10.0, 20.0, UnitSystem.METRIC))
        assertEquals("6–12 mi", DisplayUnits.formatDistanceKmRange(10.0, 20.0, UnitSystem.IMPERIAL_UK))
        assertEquals(
            "Route planned · 12.3 km",
            DisplayUnits.formatRoutePlanned(12.3, UnitSystem.METRIC),
        )
        assertEquals(
            "Route planned · 7.6 mi",
            DisplayUnits.formatRoutePlanned(12.3, UnitSystem.IMPERIAL_UK),
        )
    }

    @Test
    fun hudSpeedLineFollowsUnitSystem() {
        assertNull(formatHudSpeedLine(DriveHudState()))
        assertEquals(
            "72 km/h",
            formatHudSpeedLine(DriveHudState(currentSpeedKmh = 72.0)),
        )
        assertEquals(
            "45 mph",
            formatHudSpeedLine(
                DriveHudState(currentSpeedKmh = 72.0, unitSystem = UnitSystem.IMPERIAL_US),
            ),
        )
        assertEquals(
            "45 / 50 mph",
            formatHudSpeedLine(
                DriveHudState(
                    currentSpeedKmh = 72.4,
                    currentSpeedLimitKmh = 80.0,
                    unitSystem = UnitSystem.IMPERIAL_UK,
                ),
            ),
        )
        assertEquals(
            "Limit 50 mph",
            formatHudSpeedLine(
                DriveHudState(
                    currentSpeedLimitKmh = 80.0,
                    unitSystem = UnitSystem.IMPERIAL_US,
                ),
            ),
        )
    }

    @Test
    fun breakHudLineImperialDistance() {
        assertEquals(
            "Break in 199 mi",
            formatBreakHudLine(
                routePlanned = true,
                breakRemindersEnabled = true,
                minutesToBreak = 240.0,
                breakAsDistance = true,
                unitSystem = UnitSystem.IMPERIAL_UK,
            ),
        )
    }

    @Test
    fun speedCameraTitleUsesDisplayUnits() {
        val metric =
            SpeedCameraWarningState(
                active = true,
                kind = "point",
                label = "Speed camera 80 km/h",
                limitKmh = 80.0,
                unitSystem = UnitSystem.METRIC,
            )
        assertEquals("Speed camera 80 km/h", speedCameraTitle(metric))
        assertEquals(
            "Speed camera 50 mph",
            speedCameraTitle(metric.copy(unitSystem = UnitSystem.IMPERIAL_US)),
        )
        val entering =
            SpeedCameraWarningState(
                kind = "average_speed",
                label = "Entering average-speed zone 80 km/h",
                limitKmh = 80.0,
                unitSystem = UnitSystem.IMPERIAL_UK,
            )
        assertEquals("Entering average-speed zone 50 mph", speedCameraTitle(entering))
    }

    @Test
    fun firstInstallCountryIso() {
        assertEquals(UnitSystem.METRIC, UnitSystem.defaultForCountryIso(null))
        assertEquals(UnitSystem.METRIC, UnitSystem.defaultForCountryIso("  "))
        assertEquals(UnitSystem.METRIC, UnitSystem.defaultForCountryIso("NO"))
        assertEquals(UnitSystem.IMPERIAL_US, UnitSystem.defaultForCountryIso("us"))
        assertEquals(UnitSystem.IMPERIAL_US, UnitSystem.defaultForCountryIso("LR"))
        assertEquals(UnitSystem.IMPERIAL_US, UnitSystem.defaultForCountryIso("MM"))
        assertEquals(UnitSystem.IMPERIAL_UK, UnitSystem.defaultForCountryIso("GB"))
        assertEquals(UnitSystem.IMPERIAL_UK, UnitSystem.defaultForCountryIso("UK"))
    }

    @Test
    fun emulatorFingerprintSkipsInference() {
        assertTrue(
            UnitSystem.looksLikeEmulator(
                fingerprint = "generic/sdk_gphone64_x86_64/emulator:16",
                product = "sdk_gphone64_x86_64",
                model = "sdk_gphone64_x86_64",
            ),
        )
        assertFalse(
            UnitSystem.looksLikeEmulator(
                fingerprint = "google/husky/husky:16/BP2A",
                product = "husky",
                model = "Pixel 8 Pro",
            ),
        )
    }
}
