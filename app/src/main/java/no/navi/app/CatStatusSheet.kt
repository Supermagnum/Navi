package no.navi.app

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import org.json.JSONArray
import org.json.JSONObject

data class CatUiState(
    val statusJson: String = """{"connected":false}""",
    val nearbyJson: String = "[]",
    val host: String = "10.0.2.2",
    val port: String = "4532",
    val followNetworkId: String = "LA5MR",
    val lastMessage: String = "",
)

@Composable
fun CatStatusSheet(
    state: CatUiState,
    onHostChange: (String) -> Unit,
    onPortChange: (String) -> Unit,
    onFollowNetworkChange: (String) -> Unit,
    onConnect: () -> Unit,
    onDisconnect: () -> Unit,
    onRefresh: () -> Unit,
    onFollowEnable: () -> Unit,
    onFollowDisable: () -> Unit,
    onClose: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val connected =
        runCatching { JSONObject(state.statusJson).optBoolean("connected") }.getOrDefault(false)
    val model =
        runCatching { JSONObject(state.statusJson).optString("model") }.getOrDefault("")
    val follow =
        runCatching {
            JSONObject(state.statusJson).optString("follow_network_id")
        }.getOrDefault("")
    val nearby = parseNearby(state.nearbyJson)

    Surface(
        tonalElevation = 8.dp,
        shadowElevation = 8.dp,
        modifier = modifier.fillMaxWidth().testTag("cat_status_sheet"),
    ) {
        Column(
            modifier =
                Modifier
                    .fillMaxWidth()
                    .padding(12.dp)
                    .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text("CAT / CATS radio", style = MaterialTheme.typography.titleMedium)
            Text(
                "Receive-only programming. Navi never transmits (no PTT).",
                style = MaterialTheme.typography.bodySmall,
            )
            Text(
                "AnyTone CPS CSV: put channel.csv (and zone / gps-roaming / offset) under filesDir/cat/import/. Format: docs/cat-test.md.",
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.testTag("cat_import_path_hint"),
            )
            Text(
                if (connected) "Connected${if (model.isNotBlank()) " ($model)" else ""}"
                else "Disconnected",
                style = MaterialTheme.typography.bodyMedium,
                modifier = Modifier.testTag("cat_connection_label"),
            )
            if (follow.isNotBlank()) {
                Text("Network follow: $follow", style = MaterialTheme.typography.bodySmall)
            }
            OutlinedTextField(
                value = state.host,
                onValueChange = onHostChange,
                label = { Text("rigctld host") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().testTag("cat_host_field"),
            )
            OutlinedTextField(
                value = state.port,
                onValueChange = onPortChange,
                label = { Text("port") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().testTag("cat_port_field"),
            )
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                OutlinedButton(onClick = onConnect, modifier = Modifier.testTag("cat_btn_connect")) {
                    Text("Connect")
                }
                OutlinedButton(
                    onClick = onDisconnect,
                    modifier = Modifier.testTag("cat_btn_disconnect"),
                ) {
                    Text("Disconnect")
                }
                OutlinedButton(onClick = onRefresh, modifier = Modifier.testTag("cat_btn_refresh")) {
                    Text("Refresh")
                }
            }
            OutlinedTextField(
                value = state.followNetworkId,
                onValueChange = onFollowNetworkChange,
                label = { Text("Network id (e.g. LA5MR)") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().testTag("cat_follow_network_field"),
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(
                    onClick = onFollowEnable,
                    modifier = Modifier.testTag("cat_btn_follow_on"),
                ) {
                    Text("Follow on")
                }
                OutlinedButton(
                    onClick = onFollowDisable,
                    modifier = Modifier.testTag("cat_btn_follow_off"),
                ) {
                    Text("Follow off")
                }
            }
            Text("Nearby repeaters (≤150 km)", style = MaterialTheme.typography.titleSmall)
            if (nearby.isEmpty()) {
                Text(
                    "None in the onboard DB yet (import OSM / CSV fixtures, or wait for host seed).",
                    style = MaterialTheme.typography.bodySmall,
                )
            } else {
                for (row in nearby.take(12)) {
                    Text(row, style = MaterialTheme.typography.bodySmall)
                }
            }
            if (state.lastMessage.isNotBlank()) {
                Text(state.lastMessage, style = MaterialTheme.typography.bodySmall)
            }
            TextButton(onClick = onClose, modifier = Modifier.testTag("cat_btn_close")) {
                Text("Close")
            }
        }
    }
}

private fun parseNearby(json: String): List<String> {
    return try {
        val arr = JSONArray(json.ifBlank { "[]" })
        buildList {
            for (i in 0 until arr.length()) {
                val o = arr.optJSONObject(i) ?: continue
                val call = o.optString("callsign")
                val freq = o.optDouble("freq_out_mhz", Double.NaN)
                val dist = o.optDouble("distance_km", Double.NaN)
                val mod = o.optString("modulation")
                val distLabel =
                    if (dist.isFinite()) String.format(java.util.Locale.US, "%.1f km", dist) else "?"
                val freqLabel =
                    if (freq.isFinite()) String.format(java.util.Locale.US, "%.3f", freq) else "?"
                add("$call  $freqLabel MHz  $mod  $distLabel")
            }
        }
    } catch (_: Throwable) {
        emptyList()
    }
}
