package no.navi.app

import org.json.JSONException
import org.json.JSONObject
import java.util.Locale

/** Calibration copy shown next to the Calibrate button. Do not reword. */
const val IMU_CALIBRATION_INSTRUCTION =
    "Find a level concrete surface, like the floor of your garage, then tap Calibrate. " +
        "The unit will then use the current values as level."

/** What the plugin is for, shown in settings. */
const val IMU_PLUGIN_PURPOSE =
    "For a phone or tablet mounted in a vehicle: shows pitch, tilt and compass heading."

enum class ImuStatus { NO_SENSOR, UNCALIBRATED, CALIBRATED, COLLECTING }

data class ImuSensorsPresent(
    val gravity: Boolean = false,
    val accelerometer: Boolean = false,
    val gyroscope: Boolean = false,
    val linearAcceleration: Boolean = false,
    val magneticField: Boolean = false,
) {
    fun labels(): List<String> =
        buildList {
            if (gravity) add("gravity")
            if (accelerometer) add("accelerometer")
            if (gyroscope) add("gyroscope")
            if (linearAcceleration) add("linear acceleration")
            if (magneticField) add("magnetometer")
        }
}

/** Display state the guest writes every tick (`live.imu_state`). */
data class ImuReadout(
    val status: ImuStatus,
    val statusText: String,
    val noticeText: String?,
    val calibrated: Boolean,
    val calibratedDate: String?,
    val pitchDeg: Double?,
    val rollDeg: Double?,
    val tiltDeg: Double?,
    val headingDeg: Double?,
    val sensors: ImuSensorsPresent,
)

object ImuReadoutParser {
    /** Parse the guest state JSON; null when it is not a recognisable state. */
    fun parse(json: String?): ImuReadout? {
        if (json.isNullOrBlank()) return null
        return try {
            val o = JSONObject(json)
            val sensors = o.optJSONObject("sensors")
            ImuReadout(
                status = status(o.getString("status")) ?: return null,
                statusText = o.optString("status_text", ""),
                noticeText = o.optJSONObject("notice")?.optString("text")?.takeIf { it.isNotBlank() },
                calibrated = o.optBoolean("calibrated", false),
                calibratedDate = o.optStringOrNull("calibrated_date"),
                pitchDeg = o.optDoubleOrNull("pitch_deg"),
                rollDeg = o.optDoubleOrNull("roll_deg"),
                tiltDeg = o.optDoubleOrNull("tilt_deg"),
                headingDeg = o.optDoubleOrNull("heading_deg"),
                sensors =
                    ImuSensorsPresent(
                        gravity = sensors?.optBoolean("gravity", false) ?: false,
                        accelerometer = sensors?.optBoolean("accelerometer", false) ?: false,
                        gyroscope = sensors?.optBoolean("gyroscope", false) ?: false,
                        linearAcceleration = sensors?.optBoolean("linear_acceleration", false) ?: false,
                        magneticField = sensors?.optBoolean("magnetic_field", false) ?: false,
                    ),
            )
        } catch (_: JSONException) {
            null
        }
    }

    private fun status(raw: String): ImuStatus? =
        when (raw) {
            "no_sensor" -> ImuStatus.NO_SENSOR
            "uncalibrated" -> ImuStatus.UNCALIBRATED
            "calibrated" -> ImuStatus.CALIBRATED
            "collecting" -> ImuStatus.COLLECTING
            else -> null
        }

    private fun JSONObject.optDoubleOrNull(key: String): Double? = if (isNull(key) || !has(key)) null else optDouble(key).takeIf { it.isFinite() }

    private fun JSONObject.optStringOrNull(key: String): String? = if (isNull(key) || !has(key)) null else optString(key).takeIf { it.isNotBlank() }
}

/** One-line degree formatting for the live readout. */
fun formatImuDegrees(value: Double?): String = if (value == null) "--" else String.format(Locale.US, "%+.1f\u00B0", value)

/** Compass heading 0..360, or "--" without a magnetometer. */
fun formatImuHeading(value: Double?): String =
    if (value == null) {
        "--"
    } else {
        val d = ((value % 360.0) + 360.0) % 360.0
        String.format(Locale.US, "%03.0f\u00B0", d)
    }
