package no.navi.app

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex

/** Continuous press duration before a map mark menu opens. */
const val MAP_LONG_PRESS_HOLD_MS = 4_000L

/**
 * Finger may drift this many CSS/Compose pixels without cancelling the hold.
 * Larger motion cancels so a normal pan does not fire the menu.
 */
const val MAP_LONG_PRESS_MOVE_SLOP_PX = 28f

/** Pending map mark after a completed 4 s hold. */
data class MapMarkPending(
    val lat: Double,
    val lon: Double,
    val suggestedName: String,
    val kind: String,
)

@Composable
fun MapLongPressHoldRing(
    screenX: Float,
    screenY: Float,
    progress: Float,
    modifier: Modifier = Modifier,
) {
    if (progress <= 0f) return
    Canvas(
        modifier =
            modifier
                .fillMaxSize()
                .zIndex(3f)
                .testTag("map_long_press_hold_ring"),
    ) {
        val center = Offset(screenX, screenY)
        val radius = 42f
        drawCircle(Color(0x55000000), radius = radius + 8f, center = center)
        drawCircle(Color(0x88FFFFFF), radius = radius, center = center, style = Stroke(width = 6f))
        drawArc(
            color = Color(0xFF1565C0),
            startAngle = -90f,
            sweepAngle = 360f * progress.coerceIn(0f, 1f),
            useCenter = false,
            topLeft = Offset(center.x - radius, center.y - radius),
            size = Size(radius * 2f, radius * 2f),
            style = Stroke(width = 8f, cap = StrokeCap.Round),
        )
        drawCircle(Color(0xFF1565C0), radius = 8f, center = center)
    }
}

@Composable
fun MapMarkActionSheet(
    pending: MapMarkPending,
    onSetFrom: () -> Unit,
    onSetVia: () -> Unit,
    onSetTo: () -> Unit,
    onSavePlace: () -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier =
            modifier
                .fillMaxSize()
                .background(Color(0x66000000))
                .clickable(onClick = onCancel)
                .zIndex(4f)
                .testTag("map_mark_action_sheet_scrim"),
    ) {
        Surface(
            shape = RoundedCornerShape(topStart = 16.dp, topEnd = 16.dp),
            tonalElevation = 8.dp,
            modifier =
                Modifier
                    .align(Alignment.BottomCenter)
                    .fillMaxWidth()
                    .clickable(enabled = false) {}
                    .testTag("map_mark_action_sheet"),
        ) {
            Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Text("Marked location", style = MaterialTheme.typography.titleMedium)
                Text(
                    pending.suggestedName,
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.testTag("map_mark_suggested_name"),
                )
                Text(
                    formatCoordWaypointName(pending.lat, pending.lon),
                    style = MaterialTheme.typography.bodySmall,
                )
                Spacer(Modifier.height(8.dp))
                Button(
                    onClick = onSetFrom,
                    modifier = Modifier.fillMaxWidth().testTag("map_mark_set_from"),
                ) { Text("Set as From / Start") }
                Button(
                    onClick = onSetVia,
                    modifier = Modifier.fillMaxWidth().testTag("map_mark_set_via"),
                ) { Text("Set as Via") }
                Button(
                    onClick = onSetTo,
                    modifier = Modifier.fillMaxWidth().testTag("map_mark_set_to"),
                ) { Text("Set as To / Destination") }
                Button(
                    onClick = onSavePlace,
                    modifier = Modifier.fillMaxWidth().testTag("map_mark_save_place"),
                ) { Text("Save this place") }
                TextButton(
                    onClick = onCancel,
                    modifier = Modifier.fillMaxWidth().testTag("map_mark_cancel"),
                ) { Text("Cancel") }
            }
        }
    }
}

@Composable
fun SavePlaceNameDialog(
    name: String,
    onNameChange: (String) -> Unit,
    onConfirm: () -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier =
            modifier
                .fillMaxSize()
                .background(Color(0x66000000))
                .clickable(onClick = onCancel)
                .zIndex(5f)
                .testTag("save_place_dialog_scrim"),
    ) {
        Surface(
            shape = RoundedCornerShape(16.dp),
            tonalElevation = 8.dp,
            modifier =
                Modifier
                    .align(Alignment.Center)
                    .fillMaxWidth()
                    .padding(24.dp)
                    .clickable(enabled = false) {}
                    .testTag("save_place_dialog"),
        ) {
            Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text("Save place", style = MaterialTheme.typography.titleMedium)
                OutlinedTextField(
                    value = name,
                    onValueChange = onNameChange,
                    label = { Text("Name") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth().testTag("field_save_place_name"),
                )
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.End,
                ) {
                    TextButton(onClick = onCancel, modifier = Modifier.testTag("btn_save_place_cancel")) {
                        Text("Cancel")
                    }
                    Button(onClick = onConfirm, modifier = Modifier.testTag("btn_save_place_confirm")) {
                        Text("Save")
                    }
                }
            }
        }
    }
}

/** Fallback label when no nearby name is found at a map mark. */
fun formatMapMarkFallback(
    lat: Double,
    lon: Double,
): String = "Marked (${formatCoordWaypointName(lat, lon)})"
