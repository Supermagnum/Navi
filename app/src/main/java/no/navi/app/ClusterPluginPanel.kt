package no.navi.app

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
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
        ClusterSinkKindFfi.USB_NDJSON -> "USB NDJSON (when cable open)"
        ClusterSinkKindFfi.KUKSA -> "Kuksa / VSS path log"
        ClusterSinkKindFfi.JSON_UDP -> "JSON UDP / WS record"
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
