package no.navi.app

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlin.math.roundToInt

/**
 * Small map overlay. Compose it only while the overlay toggle is on so the
 * 2 Hz poll does not run when the user has not asked for it.
 */
@Composable
fun LocationDiagnosticsOverlay() {
    var snap by remember { mutableStateOf(LocationDiagnostics.overlay()) }
    LaunchedEffect(Unit) {
        while (isActive) {
            snap = LocationDiagnostics.overlay()
            delay(500)
        }
    }
    val acc =
        snap.accuracyM?.let { "${it.roundToInt()} m" } ?: "--"
    val stand = snap.standstill
    val along =
        if (snap.alongM.isFinite()) {
            "${snap.alongM.roundToInt()} m"
        } else {
            "--"
        }
    val route =
        when {
            !snap.hasSample -> "no route snap"
            snap.offRoute -> "OFF ROUTE"
            else -> "on route"
        }
    val sats =
        if (snap.satsUsed < 0) {
            "sat --/--"
        } else {
            "sat ${snap.satsUsed}/${snap.satsInView}"
        }
    Column(
        modifier =
            Modifier
                .padding(8.dp)
                .background(Color.Black.copy(alpha = 0.65f))
                .padding(horizontal = 8.dp, vertical = 4.dp)
                .testTag("location_diag_overlay"),
    ) {
        Text(
            text = "acc $acc  $sats  src ${snap.source.ifBlank { "--" }}",
            color = Color.White,
        )
        Text(
            text = "standstill $stand  snap ${snap.snapIndex}  along $along  $route",
            color = Color.White,
        )
    }
}
