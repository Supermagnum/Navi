package no.navi.app

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import org.json.JSONObject
import uniffi.navi.FfiIconTheme
import uniffi.navi.formatApproachDistance
import uniffi.navi.rasterizeIconPng

/**
 * Speed-camera HUD: point warnings reuse approach distance phases; average-speed
 * zones use a distinct enter / in-zone presentation (not a single-point beep).
 */
data class SpeedCameraWarningState(
    val active: Boolean = false,
    val kind: String = "point",
    val phase: String = "hidden",
    val distanceM: Double = Double.POSITIVE_INFINITY,
    val limitKmh: Double? = null,
    val zoneRemainingM: Double? = null,
    val zoneTimeBudgetS: Double? = null,
    val label: String = "",
    val preferMetric: Boolean = true,
)

fun speedCameraWarningFromJson(raw: String): SpeedCameraWarningState {
    if (raw.isBlank() || raw == "{}") return SpeedCameraWarningState()
    return runCatching {
        val o = JSONObject(raw)
        if (!o.has("kind")) return SpeedCameraWarningState()
        SpeedCameraWarningState(
            active = true,
            kind = o.optString("kind", "point"),
            phase = o.optString("phase", "hidden"),
            distanceM = o.optDouble("distance_m", Double.POSITIVE_INFINITY),
            limitKmh =
                if (o.isNull("applicable_limit_kmh")) {
                    null
                } else {
                    o.optDouble("applicable_limit_kmh")
                },
            zoneRemainingM =
                if (o.isNull("zone_remaining_m")) {
                    null
                } else {
                    o.optDouble("zone_remaining_m")
                },
            zoneTimeBudgetS =
                if (o.isNull("zone_time_budget_s")) {
                    null
                } else {
                    o.optDouble("zone_time_budget_s")
                },
            label = o.optString("label", ""),
        )
    }.getOrDefault(SpeedCameraWarningState())
}

@Composable
fun SpeedCameraWarningBox(
    state: SpeedCameraWarningState,
    iconsDir: String,
    modifier: Modifier = Modifier,
) {
    if (!state.active || state.phase == "hidden") return
    val urgency = state.phase == "urgency"
    val isZone = state.kind == "average_speed"
    val fill =
        when {
            isZone && urgency -> Color(0xFFFFE0B2)
            isZone -> Color(0xFFFFF3E0)
            urgency -> Color(0xFFFFCDD2)
            else -> Color(0xFFFFEBEE)
        }
    val border =
        when {
            isZone -> Color(0xFFE65100)
            else -> Color(0xFFC62828)
        }
    val png =
        remember(iconsDir, urgency) {
            runCatching {
                rasterizeIconPng(
                    key = "speed_camera",
                    theme = FfiIconTheme.DAY,
                    width = if (urgency) 96u else 72u,
                    height = if (urgency) 96u else 72u,
                    bundledDir = iconsDir,
                )
            }.getOrDefault(ByteArray(0))
        }
    val bmp =
        remember(png) {
            if (png.isEmpty()) null else BitmapFactory.decodeByteArray(png, 0, png.size)
        }
    val dist =
        if (isZone && state.zoneRemainingM != null) {
            formatApproachDistance(state.zoneRemainingM, state.preferMetric)
        } else {
            formatApproachDistance(state.distanceM, state.preferMetric)
        }
    val title =
        state.label.ifBlank {
            if (isZone) "Average-speed zone" else "Speed camera"
        }
    Box(
        modifier =
            modifier
                .width(IntrinsicSize.Max)
                .widthIn(min = 160.dp, max = 300.dp)
                .background(fill, RectangleShape)
                .border(2.dp, border, RectangleShape)
                .padding(horizontal = 12.dp, vertical = 10.dp)
                .testTag(if (isZone) "speed_camera_zone_box" else "speed_camera_point_box")
                .semantics {
                    contentDescription = title
                },
    ) {
        Row(
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (bmp != null) {
                Image(
                    bitmap = bmp.asImageBitmap(),
                    contentDescription = null,
                    modifier =
                        Modifier
                            .size(if (urgency) 48.dp else 36.dp)
                            .testTag("speed_camera_icon"),
                )
            }
            Column {
                Text(
                    text = title,
                    style = MaterialTheme.typography.titleSmall,
                    color = border,
                    modifier = Modifier.testTag("speed_camera_label"),
                )
                Text(
                    text = if (isZone) "Remaining $dist" else dist,
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.testTag("speed_camera_distance"),
                )
                if (isZone && state.zoneTimeBudgetS != null && state.zoneTimeBudgetS.isFinite()) {
                    val mins = (state.zoneTimeBudgetS / 60.0).coerceAtLeast(0.0)
                    Text(
                        text = "Time budget ~${"%.0f".format(mins)} min at limit",
                        style = MaterialTheme.typography.bodySmall,
                        color = Color(0xFF5D4037),
                        modifier = Modifier.testTag("speed_camera_zone_budget"),
                    )
                }
            }
        }
    }
}
