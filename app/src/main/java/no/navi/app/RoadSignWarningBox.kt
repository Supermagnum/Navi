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
import uniffi.navi.rasterizeIconPng

data class RoadSignWarningState(
    val active: Boolean = false,
    val phase: String = "hidden",
    val distanceM: Double = Double.POSITIVE_INFINITY,
    val iconKey: String = "",
    val code: String = "",
    val label: String = "",
    val unitSystem: UnitSystem = UnitSystem.METRIC,
)

fun roadSignWarningFromJson(raw: String): RoadSignWarningState {
    if (raw.isBlank() || raw == "{}") return RoadSignWarningState()
    return runCatching {
        val o = JSONObject(raw)
        if (!o.has("icon_key")) return RoadSignWarningState()
        RoadSignWarningState(
            active = true,
            phase = o.optString("phase", "hidden"),
            distanceM = o.optDouble("distance_m", Double.POSITIVE_INFINITY),
            iconKey = o.optString("icon_key", ""),
            code = o.optString("code", ""),
            label = o.optString("label", o.optString("name_en", "")),
        )
    }.getOrDefault(RoadSignWarningState())
}

@Composable
fun RoadSignWarningBox(
    state: RoadSignWarningState,
    iconsDir: String,
    modifier: Modifier = Modifier,
) {
    if (!state.active || state.phase == "hidden" || state.iconKey.isBlank()) return
    val urgency = state.phase == "urgency"
    val fill = if (urgency) Color(0xFFFFF9C4) else Color(0xFFFFFDE7)
    val border = Color(0xFFF9A825)
    val png =
        remember(iconsDir, state.iconKey, urgency) {
            runCatching {
                rasterizeIconPng(
                    key = state.iconKey,
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
    val dist = DisplayUnits.formatDistanceM(state.distanceM, state.unitSystem)
    val title = state.label.ifBlank { "Road sign" }
    Box(
        modifier =
            modifier
                .width(IntrinsicSize.Max)
                .widthIn(min = 160.dp, max = 300.dp)
                .background(fill, RectangleShape)
                .border(2.dp, border, RectangleShape)
                .padding(horizontal = 12.dp, vertical = 10.dp)
                .testTag("road_sign_warning_box")
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
                            .testTag("road_sign_icon"),
                )
            }
            Column {
                Text(
                    text = title,
                    style = MaterialTheme.typography.titleSmall,
                    color = border,
                    modifier = Modifier.testTag("road_sign_label"),
                )
                Text(
                    text = dist,
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.testTag("road_sign_distance"),
                )
            }
        }
    }
}
