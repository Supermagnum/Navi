package no.navi.app

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import uniffi.navi.ClusterSinkKindFfi

private fun sinkLabel(kind: ClusterSinkKindFfi): String =
    when (kind) {
        ClusterSinkKindFfi.LOG -> "Log sink (debug)"
        ClusterSinkKindFfi.USB_NDJSON -> "USB accessory NDJSON"
        ClusterSinkKindFfi.KUKSA -> "Kuksa / VSS TCP bridge"
        ClusterSinkKindFfi.JSON_UDP -> "JSON UDP"
    }

private fun nextSink(kind: ClusterSinkKindFfi): ClusterSinkKindFfi {
    val all = ClusterSinkKindFfi.values()
    return all[(kind.ordinal + 1) % all.size]
}

@Composable
fun ClusterPluginPanel(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val ui by ClusterPluginRuntime.ui.collectAsState()
    val scope = rememberCoroutineScope()
    var udpHost by remember(ui.jsonUdpHost) { mutableStateOf(ui.jsonUdpHost) }
    var udpPort by remember(ui.jsonUdpPort) { mutableStateOf(ui.jsonUdpPort.toString()) }
    var kuksaHost by remember(ui.kuksaHost) { mutableStateOf(ui.kuksaHost) }
    var kuksaPort by remember(ui.kuksaPort) { mutableStateOf(ui.kuksaPort.toString()) }

    Column(
        modifier = modifier.fillMaxWidth().testTag("cluster_plugin_section"),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(stringResource(R.string.cluster_plugin_toggle_label))
            Switch(
                checked = ui.enabled,
                onCheckedChange = { on ->
                    scope.launch(Dispatchers.IO) {
                        ClusterPluginRuntime.setEnabled(context, on)
                    }
                },
                modifier = Modifier.testTag("toggle_cluster_plugin"),
            )
        }
        Text(
            stringResource(R.string.cluster_plugin_purpose),
            style = MaterialTheme.typography.bodySmall,
            modifier = Modifier.testTag("cluster_purpose"),
        )
        if (!ui.enabled) return@Column

        Text(
            stringResource(R.string.cluster_sink_label) + ": " + sinkLabel(ui.sinkKind),
            style = MaterialTheme.typography.bodySmall,
            modifier = Modifier.testTag("cluster_sink_field"),
        )
        OutlinedButton(
            onClick = {
                val next = nextSink(ui.sinkKind)
                scope.launch(Dispatchers.IO) {
                    ClusterPluginRuntime.setSinkKind(context, next)
                }
            },
            modifier = Modifier.testTag("cluster_sink_cycle"),
        ) {
            Text(stringResource(R.string.cluster_sink_cycle))
        }

        OutlinedTextField(
            value = udpHost,
            onValueChange = { udpHost = it },
            label = { Text(stringResource(R.string.cluster_json_udp_host)) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth().testTag("cluster_udp_host"),
        )
        OutlinedTextField(
            value = udpPort,
            onValueChange = { udpPort = it.filter { ch -> ch.isDigit() }.take(5) },
            label = { Text(stringResource(R.string.cluster_json_udp_port)) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth().testTag("cluster_udp_port"),
        )
        OutlinedTextField(
            value = kuksaHost,
            onValueChange = { kuksaHost = it },
            label = { Text(stringResource(R.string.cluster_kuksa_host)) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth().testTag("cluster_kuksa_host"),
        )
        OutlinedTextField(
            value = kuksaPort,
            onValueChange = { kuksaPort = it.filter { ch -> ch.isDigit() }.take(5) },
            label = { Text(stringResource(R.string.cluster_kuksa_port)) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth().testTag("cluster_kuksa_port"),
        )
        OutlinedButton(
            onClick = {
                val udpP = udpPort.toIntOrNull() ?: 8765
                val kuksaP = kuksaPort.toIntOrNull() ?: 55556
                scope.launch(Dispatchers.IO) {
                    ClusterPluginRuntime.setEndpoints(
                        context,
                        udpHost.trim().ifEmpty { "127.0.0.1" },
                        udpP.coerceIn(1, 65535),
                        kuksaHost.trim().ifEmpty { "127.0.0.1" },
                        kuksaP.coerceIn(1, 65535),
                    )
                }
            },
            modifier = Modifier.testTag("cluster_save_endpoints"),
        ) {
            Text(stringResource(R.string.cluster_save_endpoints))
        }

        ui.usbStatus?.let {
            Text(it, style = MaterialTheme.typography.bodySmall, modifier = Modifier.testTag("cluster_usb_status"))
        }
        ui.message?.let {
            Text(it, style = MaterialTheme.typography.bodySmall, modifier = Modifier.testTag("cluster_message"))
        }
        ui.lastPublishPreview?.let {
            Text(
                "Last publish: $it",
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.testTag("cluster_last_publish"),
            )
        }
    }
}
