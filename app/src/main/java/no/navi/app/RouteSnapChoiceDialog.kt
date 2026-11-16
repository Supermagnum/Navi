package no.navi.app

import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import java.util.Locale

/**
 * Not-on-road choice for START_NOT_ON_ROAD / END_NOT_ON_ROAD. Renders the
 * current [SnapChoiceFlow.State]; all transitions are reported to the caller,
 * which owns the flow and the actual routing.
 */
@Composable
fun RouteSnapChoiceDialog(
    state: SnapChoiceFlow.State,
    onRequestSnap: () -> Unit,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    when (state) {
        SnapChoiceFlow.State.Idle -> Unit
        is SnapChoiceFlow.State.NotOnRoad ->
            AlertDialog(
                onDismissRequest = onDismiss,
                title = { Text(stringResource(titleFor(state.role))) },
                text = { Text(stringResource(R.string.dialog_not_on_road_body)) },
                confirmButton = {
                    TextButton(
                        onClick = onRequestSnap,
                        modifier = Modifier.testTag("btn_route_to_nearest_road"),
                    ) { Text(stringResource(R.string.action_route_to_nearest_road)) }
                },
                dismissButton = {
                    TextButton(
                        onClick = onDismiss,
                        modifier = Modifier.testTag("btn_not_on_road_dismiss"),
                    ) { Text(stringResource(R.string.action_cancel)) }
                },
            )
        is SnapChoiceFlow.State.RequestingSnap ->
            AlertDialog(
                onDismissRequest = onDismiss,
                title = { Text(stringResource(titleFor(state.role))) },
                text = { Text(stringResource(R.string.dialog_nearest_road_finding)) },
                confirmButton = {},
                dismissButton = {
                    TextButton(onClick = onDismiss) { Text(stringResource(R.string.action_cancel)) }
                },
            )
        is SnapChoiceFlow.State.Preview ->
            AlertDialog(
                onDismissRequest = onDismiss,
                title = { Text(stringResource(R.string.dialog_nearest_road_title)) },
                text = {
                    Text(
                        stringResource(
                            R.string.dialog_nearest_road_body,
                            SnapChoiceFlow.formatDistance(state.offer.distanceM),
                            String.format(
                                Locale.US,
                                "%.5f, %.5f",
                                state.offer.snapLat,
                                state.offer.snapLon,
                            ),
                        ),
                        modifier = Modifier.testTag("nearest_road_preview"),
                    )
                },
                confirmButton = {
                    TextButton(
                        onClick = onConfirm,
                        modifier = Modifier.testTag("btn_confirm_nearest_road"),
                    ) { Text(stringResource(R.string.action_route_to_this_point)) }
                },
                dismissButton = {
                    TextButton(
                        onClick = onDismiss,
                        modifier = Modifier.testTag("btn_cancel_nearest_road"),
                    ) { Text(stringResource(R.string.action_cancel)) }
                },
            )
        is SnapChoiceFlow.State.NoRoadNearby ->
            AlertDialog(
                onDismissRequest = onDismiss,
                title = { Text(stringResource(titleFor(state.role))) },
                text = { Text(stringResource(R.string.dialog_nearest_road_none)) },
                confirmButton = {
                    TextButton(onClick = onDismiss) { Text(stringResource(R.string.action_cancel)) }
                },
            )
    }
}

private fun titleFor(role: NotOnRoadRole): Int =
    when (role) {
        NotOnRoadRole.START -> R.string.dialog_not_on_road_title_start
        NotOnRoadRole.END -> R.string.dialog_not_on_road_title_end
    }
