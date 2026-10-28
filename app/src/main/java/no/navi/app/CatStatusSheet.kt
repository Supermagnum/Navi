package no.navi.app

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import no.navi.app.cat.CatRigModelPreset
import no.navi.app.cat.CatSerialLoopbackBridge
import no.navi.app.cat.catRigModelLabel
import no.navi.app.cat.catRigModelPresets
import org.json.JSONArray
import org.json.JSONObject

/** Same green as pack-ready chips in Tools — connected success, not mere selection. */
private val CatConnectedChipGreen = Color(0xFF2E7D32)
private val CatConnectedChipGreenMuted = CatConnectedChipGreen.copy(alpha = 0.30f)

/**
 * True when [preset] is the Hamlib model reported connected in status JSON.
 * Matches Hamlib model_name (e.g. "Dummy") against curated preset labels.
 */
internal fun catPresetMatchesConnectedModel(
    preset: CatRigModelPreset,
    connectedModelName: String,
): Boolean {
    val n = connectedModelName.trim()
    if (n.isEmpty()) return false
    if (preset.model == 1 && n.equals("Dummy", ignoreCase = true)) return true
    if (preset.name.contains(n, ignoreCase = true)) return true
    return n
        .split(Regex("[\\s/]+"))
        .filter { it.length >= 3 }
        .any { token -> preset.name.contains(token, ignoreCase = true) }
}

data class CatUiState(
    val statusJson: String = """{"connected":false}""",
    val nearbyJson: String = "[]",
    val host: String = "10.0.2.2",
    val port: String = "4532",
    val rigModel: Int = MapHudPrefs.CAT_RIG_MODEL_DEFAULT,
    val baudRate: Int = MapHudPrefs.CAT_BAUD_RATE_DEFAULT,
    /** [MapHudPrefs.CAT_BACKEND_HAMLIB] or [MapHudPrefs.CAT_BACKEND_BENSHI]. */
    val backend: String = MapHudPrefs.CAT_BACKEND_DEFAULT,
    /** Scratch channel index for Benshi channel-A programming. */
    val scratchChannel: Int = MapHudPrefs.CAT_SCRATCH_CHANNEL_DEFAULT,
    /**
     * Developer/test only: step network follow at 5 s/site. Off by default;
     * must stay labelled as non-product.
     */
    val demoModeDeveloper: Boolean = MapHudPrefs.CAT_DEMO_MODE_DEFAULT_ENABLED,
    val followNetworkId: String = "LA5MR",
    val networkChoice: String = MapHudPrefs.CAT_NETWORK_AUTO,
    val networkNotice: String = "",
    val repeaterDropPath: String = "",
    val networkNames: List<CatNetworkChip> = emptyList(),
    val lastMessage: String = "",
    val consensusRelocation: Boolean = false,
)

/** One dropdown row: name, programmable members in the region, nearest member. */
data class CatNetworkChip(
    val name: String,
    val members: Int,
    val nearestKm: Double,
)

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun CatStatusSheet(
    state: CatUiState,
    onHostChange: (String) -> Unit,
    onPortChange: (String) -> Unit,
    onRigModelChange: (Int) -> Unit,
    onBaudRateChange: (Int) -> Unit,
    onBackendChange: (String) -> Unit,
    onScratchChannelChange: (Int) -> Unit,
    onDemoModeDeveloperChange: (Boolean) -> Unit,
    onFollowNetworkChange: (String) -> Unit,
    onConnect: () -> Unit,
    onDisconnect: () -> Unit,
    onRefresh: () -> Unit,
    onFollowEnable: () -> Unit,
    onFollowDisable: () -> Unit,
    onUpdateRepeaters: () -> Unit,
    onConsensusRelocationChange: (Boolean) -> Unit,
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
    val presets = catRigModelPresets()

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
                stringResource(R.string.cat_licence_listen_only),
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.testTag("cat_licence_listen_only"),
            )
            Text(
                "A 1750 Hz burst repeater is tuned with CTCSS and DCS off. Navi never sends the burst; you do that on the radio.",
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.testTag("cat_toneburst_note"),
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Move non-OSM repeaters to the most likely site")
                    Text(
                        "Off by default. Uses agreeing RadioID and file positions only. OSM sites stay put.",
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                Switch(
                    checked = state.consensusRelocation,
                    onCheckedChange = onConsensusRelocationChange,
                    modifier = Modifier.testTag("cat_consensus_relocation"),
                )
            }
            Text(
                "Receive-only programming. Navi never transmits (no PTT).",
                style = MaterialTheme.typography.bodySmall,
            )
            Text(
                "Download one RepeaterBook KML per country and copy it into this folder.",
                style = MaterialTheme.typography.bodySmall,
            )
            SelectionContainer {
                Text(
                    state.repeaterDropPath.ifBlank { "Android/data/no.navi.app/files/repeaters" },
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.testTag("cat_repeater_drop_path"),
                )
            }
            OutlinedButton(
                onClick = onUpdateRepeaters,
                modifier = Modifier.fillMaxWidth().testTag("cat_update_repeaters"),
            ) {
                Text(stringResource(R.string.cat_update_repeaters))
            }
            Text(
                stringResource(R.string.cat_update_repeaters_hint),
                style = MaterialTheme.typography.bodySmall,
            )
            Text("Repeater network", style = MaterialTheme.typography.titleSmall)
            if (state.networkNotice.isNotBlank()) {
                Text(
                    state.networkNotice,
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.testTag("cat_network_notice"),
                )
            }
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
                modifier = Modifier.testTag("cat_network_choices"),
            ) {
                FilterChip(
                    selected = state.networkChoice == MapHudPrefs.CAT_NETWORK_AUTO,
                    onClick = { onFollowNetworkChange(MapHudPrefs.CAT_NETWORK_AUTO) },
                    label = { Text("Nearest network (auto)") },
                    modifier = Modifier.testTag("cat_network_auto"),
                )
                for (chip in state.networkNames) {
                    val km = String.format(java.util.Locale.US, "%.0f", chip.nearestKm)
                    FilterChip(
                        selected = state.networkChoice == chip.name,
                        onClick = { onFollowNetworkChange(chip.name) },
                        label = { Text("${chip.name} (${chip.members}, $km km)") },
                    )
                }
                FilterChip(
                    selected = state.networkChoice == MapHudPrefs.CAT_NETWORK_STANDALONE,
                    onClick = { onFollowNetworkChange(MapHudPrefs.CAT_NETWORK_STANDALONE) },
                    label = { Text("Standalone repeaters (nearest)") },
                    modifier = Modifier.testTag("cat_network_standalone"),
                )
                FilterChip(
                    selected = state.networkChoice == MapHudPrefs.CAT_NETWORK_OFF,
                    onClick = { onFollowNetworkChange(MapHudPrefs.CAT_NETWORK_OFF) },
                    label = { Text("Off / manual") },
                    modifier = Modifier.testTag("cat_network_off"),
                )
            }
            Text(
                if (connected) {
                    "Connected${if (model.isNotBlank()) " ($model)" else ""}"
                } else {
                    "Disconnected"
                },
                style = MaterialTheme.typography.bodyMedium,
                modifier = Modifier.testTag("cat_connection_label"),
            )
            Text(
                "Radio: ${catRigModelLabel(state.rigModel)}  |  Baud: ${state.baudRate}",
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.testTag("cat_radio_baud_summary"),
            )
            if (follow.isNotBlank()) {
                Text("Network follow: $follow", style = MaterialTheme.typography.bodySmall)
            }
            Text("Backend", style = MaterialTheme.typography.titleSmall)
            Text(
                "Plugin controls VFO 1 only; VFO 2 is left to the user.",
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.testTag("cat_vfo1_only_note"),
            )
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
                modifier = Modifier.testTag("cat_backend_presets"),
            ) {
                FilterChip(
                    selected = state.backend == MapHudPrefs.CAT_BACKEND_HAMLIB,
                    onClick = { onBackendChange(MapHudPrefs.CAT_BACKEND_HAMLIB) },
                    label = { Text("Hamlib") },
                    modifier = Modifier.testTag("cat_backend_hamlib"),
                )
                FilterChip(
                    selected = state.backend == MapHudPrefs.CAT_BACKEND_BENSHI,
                    onClick = { onBackendChange(MapHudPrefs.CAT_BACKEND_BENSHI) },
                    label = { Text("Benshi") },
                    modifier = Modifier.testTag("cat_backend_benshi"),
                )
            }
            if (state.backend == MapHudPrefs.CAT_BACKEND_BENSHI) {
                OutlinedTextField(
                    value = state.scratchChannel.toString(),
                    onValueChange = { raw ->
                        val digits = raw.filter { it.isDigit() }
                        onScratchChannelChange(
                            digits.toIntOrNull()?.coerceIn(0, 255)
                                ?: MapHudPrefs.CAT_SCRATCH_CHANNEL_DEFAULT,
                        )
                    },
                    label = { Text("Scratch channel (A)") },
                    singleLine = true,
                    supportingText = {
                        Text("Used when frequency mode is unavailable (e.g. on VFO 2)")
                    },
                    modifier = Modifier.fillMaxWidth().testTag("cat_scratch_channel_field"),
                )
            }
            Text(
                "Developer / test only",
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.testTag("cat_demo_mode_section"),
            )
            FilterChip(
                selected = state.demoModeDeveloper,
                onClick = { onDemoModeDeveloperChange(!state.demoModeDeveloper) },
                label = { Text("Demo mode (5 s/site) — not for driving") },
                modifier = Modifier.testTag("cat_demo_mode_developer"),
            )
            Text(
                "Off by default. Steps network follow along a planned corridor for FakeBenshiPeer / lab checks.",
                style = MaterialTheme.typography.bodySmall,
            )
            OutlinedTextField(
                value = state.host,
                onValueChange = onHostChange,
                label = { Text("rigctld host") },
                singleLine = true,
                enabled = state.backend == MapHudPrefs.CAT_BACKEND_HAMLIB,
                modifier = Modifier.fillMaxWidth().testTag("cat_host_field"),
            )
            OutlinedTextField(
                value = state.port,
                onValueChange = onPortChange,
                label = { Text("port") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().testTag("cat_port_field"),
            )
            Text("Hamlib radio model", style = MaterialTheme.typography.titleSmall)
            Text(
                "TCP path: remote daemon model (informational). Onboard FFI / USB-BT: model used when opening the rig.",
                style = MaterialTheme.typography.bodySmall,
            )
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
                modifier = Modifier.testTag("cat_rig_model_presets"),
            ) {
                for (preset in presets) {
                    val selected = state.rigModel == preset.model
                    val connectedSuccess =
                        connected &&
                            (
                                catPresetMatchesConnectedModel(preset, model) ||
                                    (model.isBlank() && selected)
                            )
                    FilterChip(
                        selected = selected || connectedSuccess,
                        onClick = { onRigModelChange(preset.model) },
                        label = { Text(preset.name) },
                        colors =
                            FilterChipDefaults.filterChipColors(
                                containerColor =
                                    if (connectedSuccess) {
                                        CatConnectedChipGreenMuted
                                    } else {
                                        MaterialTheme.colorScheme.surfaceContainerHighest
                                    },
                                selectedContainerColor =
                                    if (connectedSuccess) {
                                        CatConnectedChipGreen
                                    } else {
                                        MaterialTheme.colorScheme.secondaryContainer
                                    },
                                labelColor = MaterialTheme.colorScheme.onSurface,
                                selectedLabelColor =
                                    if (connectedSuccess) {
                                        Color.White
                                    } else {
                                        MaterialTheme.colorScheme.onSecondaryContainer
                                    },
                            ),
                        modifier = Modifier.testTag("cat_rig_model_${preset.model}"),
                    )
                }
            }
            OutlinedTextField(
                value = state.rigModel.toString(),
                onValueChange = { raw ->
                    val digits = raw.filter { it.isDigit() }
                    if (digits.isEmpty()) {
                        onRigModelChange(MapHudPrefs.CAT_RIG_MODEL_DEFAULT)
                    } else {
                        onRigModelChange(digits.toIntOrNull()?.coerceAtLeast(1) ?: state.rigModel)
                    }
                },
                label = { Text("Hamlib model number") },
                singleLine = true,
                supportingText = { Text(catRigModelLabel(state.rigModel)) },
                modifier = Modifier.fillMaxWidth().testTag("cat_rig_model_field"),
            )
            Text("Serial baud (USB/BT loopback)", style = MaterialTheme.typography.titleSmall)
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
                modifier = Modifier.testTag("cat_baud_presets"),
            ) {
                for (baud in CatSerialLoopbackBridge.BAUD_RATES) {
                    FilterChip(
                        selected = state.baudRate == baud,
                        onClick = { onBaudRateChange(baud) },
                        label = { Text("$baud") },
                        modifier = Modifier.testTag("cat_baud_$baud"),
                    )
                }
            }
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

private fun parseNearby(json: String): List<String> =
    try {
        val arr = JSONArray(json.ifBlank { "[]" })
        buildList {
            for (i in 0 until arr.length()) {
                val o = arr.optJSONObject(i) ?: continue
                val call = o.optString("callsign")
                val freq = o.optDouble("freq_out_mhz", Double.NaN)
                val dist = o.optDouble("distance_km", Double.NaN)
                val mod = o.optString("modulation")
                val src = o.optString("source")
                val attr = o.optString("attribution").takeIf { it.isNotBlank() && it != "null" }
                val excluded = o.optBoolean("exclude_recommended", false)
                val posOk = o.optBoolean("position_accurate", true)
                val distLabel =
                    if (dist.isFinite()) String.format(java.util.Locale.US, "%.1f km", dist) else "?"
                val freqLabel =
                    if (freq.isFinite()) String.format(java.util.Locale.US, "%.3f", freq) else "?"
                val access = o.optString("access")
                val relocated = o.optBoolean("relocated", false)
                val relocation = o.optString("relocation_note")
                val flags =
                    buildList {
                        if (excluded) add("excluded")
                        if (!posOk) add("manual-only (no position)")
                        if (mod.equals("DMR", ignoreCase = true)) add("DMR info-only")
                        if (access.contains("toneburst", ignoreCase = true) ||
                            access.contains("1750", ignoreCase = true)
                        ) {
                            add("1750 Hz burst")
                        }
                        if (relocated && relocation.isNotBlank() && relocation != "null") {
                            add(relocation)
                        }
                    }.joinToString(", ")
                val attrLabel = attr?.let { " — $it" }.orEmpty()
                val srcLabel = if (src.isNotBlank()) " [$src]" else ""
                val flagLabel = if (flags.isNotEmpty()) " ($flags)" else ""
                add("$call  $freqLabel MHz  $mod  $distLabel$srcLabel$flagLabel$attrLabel")
            }
        }
    } catch (_: Throwable) {
        emptyList()
    }
