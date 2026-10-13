package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ImuReadoutTest {
    @Test
    fun calibrationInstructionIsVerbatim() {
        assertEquals(
            "Find a level concrete surface, like the floor of your garage, then tap Calibrate. " +
                "The unit will then use the current values as level.",
            IMU_CALIBRATION_INSTRUCTION,
        )
    }

    @Test
    fun parsesCalibratedStateFromTheGuest() {
        val json =
            """
            {"status":"calibrated","status_text":"Calibrated on 2026-10-12.",
             "notice":{"kind":"calibration_saved","text":"Calibration saved.","at_ms":5},
             "instruction":"x","calibrated":true,"calibrated_date":"2026-10-12","calibrated_unix":1,
             "pitch_deg":1.25,"roll_deg":-0.4,"tilt_deg":1.3,"heading_deg":94.0,
             "alarm":"warning","alarm_enabled":true,
             "sensors":{"gravity":true,"accelerometer":false,"gyroscope":true,
                        "linear_acceleration":false,"magnetic_field":true},
             "updated_ms":1000}
            """.trimIndent()
        val r = ImuReadoutParser.parse(json)!!
        assertEquals(ImuStatus.CALIBRATED, r.status)
        assertEquals("Calibrated on 2026-10-12.", r.statusText)
        assertEquals("Calibration saved.", r.noticeText)
        assertTrue(r.calibrated)
        assertEquals("2026-10-12", r.calibratedDate)
        assertEquals(1.25, r.pitchDeg!!, 1e-9)
        assertEquals(-0.4, r.rollDeg!!, 1e-9)
        assertEquals(94.0, r.headingDeg!!, 1e-9)
        assertEquals(ImuAlarmLevel.WARNING, r.alarm)
        assertEquals(listOf("gravity", "gyroscope", "magnetometer"), r.sensors.labels())
    }

    @Test
    fun parsesStateWithoutSensorsAsNulls() {
        val json =
            """
            {"status":"no_sensor","status_text":"No accelerometer or gravity sensor available on this device.",
             "notice":null,"instruction":"x","calibrated":false,"calibrated_date":null,"calibrated_unix":null,
             "pitch_deg":null,"roll_deg":null,"tilt_deg":null,"heading_deg":null,
             "alarm":"none","alarm_enabled":false,"sensors":{},"updated_ms":0}
            """.trimIndent()
        val r = ImuReadoutParser.parse(json)!!
        assertEquals(ImuStatus.NO_SENSOR, r.status)
        assertNull(r.noticeText)
        assertNull(r.calibratedDate)
        assertNull(r.pitchDeg)
        assertNull(r.headingDeg)
        assertEquals(ImuAlarmLevel.NONE, r.alarm)
        assertFalse(r.alarmEnabled)
        assertTrue(r.sensors.labels().isEmpty())
    }

    @Test
    fun rejectsGarbageAndUnknownStatus() {
        assertNull(ImuReadoutParser.parse(null))
        assertNull(ImuReadoutParser.parse(""))
        assertNull(ImuReadoutParser.parse("not json"))
        assertNull(ImuReadoutParser.parse("""{"status":"mystery"}"""))
        assertNull(ImuReadoutParser.parse("""{"status_text":"missing status"}"""))
    }

    @Test
    fun formatsDegreesAndHeading() {
        assertEquals("--", formatImuDegrees(null))
        assertEquals("+1.5\u00B0", formatImuDegrees(1.5))
        assertEquals("-12.0\u00B0", formatImuDegrees(-12.0))
        assertEquals("--", formatImuHeading(null))
        assertEquals("094\u00B0", formatImuHeading(94.0))
        assertEquals("350\u00B0", formatImuHeading(-10.0))
    }

    @Test
    fun thresholdFieldsAcceptCommaAndRejectOutOfRange() {
        assertEquals(25.0, parseImuThresholdDegrees("25")!!, 0.0)
        assertEquals(12.5, parseImuThresholdDegrees(" 12,5 ")!!, 0.0)
        assertNull(parseImuThresholdDegrees(""))
        assertNull(parseImuThresholdDegrees("abc"))
        assertNull(parseImuThresholdDegrees("0"))
        assertNull(parseImuThresholdDegrees("90"))
        assertNull(parseImuThresholdDegrees("NaN"))
    }
}
