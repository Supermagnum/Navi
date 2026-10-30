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
import no.navi.app.cat.BenshiBondedDevice
import no.navi.app.cat.CatRigModelPreset
import no.navi.app.cat.CatSerialLoopbackBridge
import no.navi.app.cat.catRigModelLabel
import no.navi.app.cat.catRigModelPresets
import org.json.JSONArray
import org.json.JSONObject

/** Same green as pack-ready chips in Tools — connected success, not mere selection. */
private val CatConnectedChipGreen = Color(0xFF2E7D32)
private val CatConnectedChipGreenMuted = CatConnectedChipGreen.copy(alpha = 0.30f)

/** `link` values from `catPluginStatusJson` for the Benshi backend. */
const val CAT_LINK_BENSHI_RFCOMM = "benshi_rfcomm"
const val CAT_LINK_BENSHI_LAB = "benshi_lab"

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

/** Real Benshi RFCOMM radio up (not lab/demo, not disconnected). */
internal fun catBenshiRfcommLive(
    connected: Boolean,
    link: String,
): Boolean = connected && link == CAT_LINK_BENSHI_RFCOMM

/**
 * Green success colors for CAT connected pills (Hamlib model or Benshi radio).
 * Disconnected / mere selection keep the theme secondary container.
 */
@Composable
internal fun catConnectedSuccessChipColors(connectedSuccess: Boolean) =
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
    )

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
    /** Bluetooth MAC of the bonded Benshi radio chosen in the picker. */
    val benshiAddress: String = "",
    val benshiDevices: List<BenshiBondedDevice> = emptyList(),
    /** Why [benshiDevices] could not be listed (permission, Bluetooth off), or empty. */
    val benshiDevicesNotice: String = "",
    val benshiConnecting: Boolean = false,
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

/**
 * Build network chips from the CAT nearby-repeaters JSON.
 *
 * JSON `null` / missing `network_id` must not become a chip named `"null"`
 * ([org.json.JSONObject.optString] stringifies JSON null that way). Those
 * sites are covered by "Standalone repeaters (nearest)".
 */
fun networkNamesFromNearby(json: String): List<CatNetworkChip> {
    val arr = runCatching { JSONArray(json) }.getOrNull() ?: return emptyList()
    val counts = linkedMapOf<String, Int>()
    val nearest = linkedMapOf<String, Double>()
    for (i in 0 until arr.length()) {
        val o = arr.optJSONObject(i) ?: continue
        if (!o.optBoolean("position_accurate", true)) continue
        if (o.optBoolean("exclude_recommended", false)) continue
        val name = o.optString("network_id").trim()
        if (name.isEmpty() || name.equals("null", ignoreCase = true)) continue
        val dist = o.optDouble("distance_km", Double.NaN)
        if (!dist.isFinite() || dist > 150.0) continue
        val modulation = o.optString("modulation")
        if (modulation.contains("DMR", ignoreCase = true) ||
            modulation.contains("DSTAR", ignoreCase = true) ||
            modulation.contains("D-STAR", ignoreCase = true) ||
            modulation.contains("C4FM", ignoreCase = true) ||
            modulation.contains("APRS", ignoreCase = true)
        ) {
            continue
        }
        counts[name] = (counts[name] ?: 0) + 1
        val prev = nearest[name]
        if (prev == null || dist < prev) nearest[name] = dist
    }
    return counts.keys
        .map { name ->
            CatNetworkChip(name, counts[name] ?: 0, nearest[name] ?: 0.0)
        }.sortedBy { it.nearestKm }
}

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
    onBenshiDeviceSelect: (String) -> Unit,
    onBenshiRefreshDevices: () -> Unit,
    onDemoModeDeveloperChange: (Boolean) -> Unit,
    onFollowNetworkChange: (String) -> Unit,
    /** Draft typing in the free-text network field — must not apply CAT follow mid-keystroke. */
    onFollowNetworkDraft: (String) -> Unit = {},
    onConnect: () -> Unit,
    onDisconnect: () -> Unit,
    onRefresh: () -> Unit,
    onFollowEnable: () -> Unit,
    onFollowDisable: () -> Unit,
    onUpdateRepeaters: () -> Unit,
    onConsensusRelocationChange: (Boolean) -> Unit,
    onClose: () -> Unit,
    modifier: Modifier = Modifier,
    updateRepeatersBusy: Boolean = false,
) {
    val connected =
        runCatching { JSONObject(state.statusJson).optBoolean("connected") }.getOrDefault(false)
    val model =
        runCatching { JSONObject(state.statusJson).optString("model") }.getOrDefault("")
    val follow =
        runCatching {
            JSONObject(state.statusJson).optString("follow_network_id")
        }.getOrDefault("")
    val link =
        runCatching { JSONObject(state.statusJson).optString("link") }.getOrDefault("")
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
            Text(stringResource(R.string.cat_plugin_toggle_label), style = MaterialTheme.typography.titleMedium)
            Text(
                stringResource(R.string.cat_licence_listen_only),
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.testTag("cat_licence_listen_only"),
            )
            Text(
                stringResource(R.string.ui_a_1750_hz_burst_repeater_is_tuned_with_ctcss_and_dcs_off_nav),
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.testTag("cat_toneburst_note"),
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(stringResource(R.string.cat_consensus_relocation))
                    Text(
                        stringResource(R.string.ui_off_by_default_uses_agreeing_radioid_and_file_positions_only),
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
                stringResource(R.string.ui_receive_only_programming_navi_never_transmits_no_ptt),
                style = MaterialTheme.typography.bodySmall,
            )
            Text(
                stringResource(R.string.ui_copy_repeaterbook_kml_kmz_exports_into_this_folder_networked),
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
                enabled = !updateRepeatersBusy,
                modifier = Modifier.fillMaxWidth().testTag("cat_update_repeaters"),
            ) {
                Text(stringResource(R.string.cat_update_repeaters))
            }
            Text(
                stringResource(R.string.cat_update_repeaters_hint),
                style = MaterialTheme.typography.bodySmall,
            )
            Text(stringResource(R.string.cat_repeater_network), style = MaterialTheme.typography.titleSmall)
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
                    label = { Text(stringResource(R.string.cat_nearest_network_auto)) },
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
                    label = { Text(stringResource(R.string.cat_standalone_nearest)) },
                    modifier = Modifier.testTag("cat_network_standalone"),
                )
                FilterChip(
                    selected = state.networkChoice == MapHudPrefs.CAT_NETWORK_OFF,
                    onClick = { onFollowNetworkChange(MapHudPrefs.CAT_NETWORK_OFF) },
                    label = { Text(stringResource(R.string.cat_off_manual)) },
                    modifier = Modifier.testTag("cat_network_off"),
                )
            }
            // Status pill: green only for live Benshi RFCOMM (VR-N76); lab stays error;
            // disconnected is not green.
            val benshiLive = catBenshiRfcommLive(connected, link)
            val statusLabel =
                when {
                    benshiLive ->
                        stringResource(R.string.cat_link_benshi_radio)
                    connected && link == CAT_LINK_BENSHI_LAB ->
                        stringResource(R.string.cat_link_benshi_demo, model)
                    connected -> "Connected${if (model.isNotBlank()) " ($model)" else ""}"
                    else -> "Disconnected"
                }
            FilterChip(
                // Non-interactive status indicator; keep enabled so Material3
                // selectedContainerColor (green) is not replaced by disabled grey.
                selected = connected,
                onClick = {},
                label = { Text(statusLabel) },
                colors =
                    when {
                        benshiLive -> catConnectedSuccessChipColors(connectedSuccess = true)
                        connected && link == CAT_LINK_BENSHI_LAB ->
                            FilterChipDefaults.filterChipColors(
                                selectedContainerColor = MaterialTheme.colorScheme.errorContainer,
                                selectedLabelColor = MaterialTheme.colorScheme.error,
                            )
                        else ->
                            FilterChipDefaults.filterChipColors(
                                containerColor =
                                    MaterialTheme.colorScheme.surfaceContainerHighest,
                                selectedContainerColor =
                                    MaterialTheme.colorScheme.surfaceContainerHighest,
                                labelColor = MaterialTheme.colorScheme.onSurfaceVariant,
                                selectedLabelColor = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                    },
                modifier = Modifier.testTag("cat_connection_label"),
            )
            if (state.backend == MapHudPrefs.CAT_BACKEND_HAMLIB) {
                Text(
                    "Radio: ${catRigModelLabel(state.rigModel)}  |  Baud: ${state.baudRate}",
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.testTag("cat_radio_baud_summary"),
                )
            }
            if (follow.isNotBlank()) {
                Text(
                    stringResource(R.string.network_follow, follow),
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            Text(stringResource(R.string.cat_backend), style = MaterialTheme.typography.titleSmall)
            Text(
                stringResource(R.string.ui_plugin_controls_vfo_1_only_vfo_2_is_left_to_the_user),
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
                    label = { Text(stringResource(R.string.cat_backend_hamlib)) },
                    modifier = Modifier.testTag("cat_backend_hamlib"),
                )
                FilterChip(
                    selected = state.backend == MapHudPrefs.CAT_BACKEND_BENSHI,
                    onClick = { onBackendChange(MapHudPrefs.CAT_BACKEND_BENSHI) },
                    label = { Text(stringResource(R.string.cat_backend_benshi)) },
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
                    label = { Text(stringResource(R.string.cat_scratch_channel)) },
                    singleLine = true,
                    supportingText = {
                        Text(stringResource(R.string.cat_scratch_channel_hint))
                    },
                    modifier = Modifier.fillMaxWidth().testTag("cat_scratch_channel_field"),
                )
                Text(stringResource(R.string.cat_benshi_radio_title), style = MaterialTheme.typography.titleSmall)
                if (state.demoModeDeveloper) {
                    Text(
                        stringResource(R.string.cat_benshi_demo_note),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error,
                        modifier = Modifier.testTag("cat_benshi_demo_note"),
                    )
                }
                if (state.benshiDevicesNotice.isNotBlank()) {
                    Text(
                        state.benshiDevicesNotice,
                        style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.testTag("cat_benshi_devices_notice"),
                    )
                } else if (state.benshiDevices.isEmpty()) {
                    Text(
                        stringResource(R.string.cat_benshi_no_paired),
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                    modifier = Modifier.testTag("cat_benshi_devices"),
                ) {
                    for (device in state.benshiDevices) {
                        val selected =
                            device.address.equals(state.benshiAddress, ignoreCase = true)
                        // Same green success pill as Hamlib model chips when RFCOMM is live.
                        val connectedSuccess = benshiLive && selected
                        FilterChip(
                            selected = selected || connectedSuccess,
                            onClick = { onBenshiDeviceSelect(device.address) },
                            label = { Text("${device.name} (${device.address})") },
                            colors = catConnectedSuccessChipColors(connectedSuccess),
                            modifier = Modifier.testTag("cat_benshi_device_${device.address}"),
                        )
                    }
                }
                OutlinedButton(
                    onClick = onBenshiRefreshDevices,
                    modifier = Modifier.testTag("cat_benshi_refresh_devices"),
                ) {
                    Text(stringResource(R.string.cat_benshi_refresh_paired))
                }
            }
            Text(
                stringResource(R.string.ui_developer_test_only),
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.testTag("cat_demo_mode_section"),
            )
            FilterChip(
                selected = state.demoModeDeveloper,
                onClick = { onDemoModeDeveloperChange(!state.demoModeDeveloper) },
                label = { Text(stringResource(R.string.cat_demo_mode)) },
                modifier = Modifier.testTag("cat_demo_mode_developer"),
            )
            Text(
                stringResource(R.string.ui_off_by_default_steps_network_follow_along_a_planned_corridor),
                style = MaterialTheme.typography.bodySmall,
            )
            // Hamlib TCP/FFI controls (including Kenwood TS-2000 model chips) only
            // when that backend is selected — never alongside Benshi / VR-N76.
            if (state.backend == MapHudPrefs.CAT_BACKEND_HAMLIB) {
                OutlinedTextField(
                    value = state.host,
                    onValueChange = onHostChange,
                    label = { Text(stringResource(R.string.cat_rigctld_host)) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth().testTag("cat_host_field"),
                )
                OutlinedTextField(
                    value = state.port,
                    onValueChange = onPortChange,
                    label = { Text(stringResource(R.string.cat_port)) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth().testTag("cat_port_field"),
                )
                Text(stringResource(R.string.cat_hamlib_model_title), style = MaterialTheme.typography.titleSmall)
                Text(
                    stringResource(R.string.ui_tcp_path_remote_daemon_model_informational_onboard_ffi_usb_b),
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
                            colors = catConnectedSuccessChipColors(connectedSuccess),
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
                    label = { Text(stringResource(R.string.cat_hamlib_model_number)) },
                    singleLine = true,
                    supportingText = { Text(catRigModelLabel(state.rigModel)) },
                    modifier = Modifier.fillMaxWidth().testTag("cat_rig_model_field"),
                )
                Text(stringResource(R.string.cat_serial_baud), style = MaterialTheme.typography.titleSmall)
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
            }
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                OutlinedButton(
                    onClick = onConnect,
                    enabled = !state.benshiConnecting,
                    modifier = Modifier.testTag("cat_btn_connect"),
                ) {
                    Text(stringResource(R.string.action_connect))
                }
                OutlinedButton(
                    onClick = onDisconnect,
                    modifier = Modifier.testTag("cat_btn_disconnect"),
                ) {
                    Text(stringResource(R.string.action_disconnect))
                }
                OutlinedButton(onClick = onRefresh, modifier = Modifier.testTag("cat_btn_refresh")) {
                    Text(stringResource(R.string.action_refresh))
                }
            }
            OutlinedTextField(
                value = state.followNetworkId,
                // Draft only: accidental soft-key / focus input must not mutate the live
                // follow id (e.g. LA5MR → LA5AMR) until Follow on or a chip commits it.
                onValueChange = onFollowNetworkDraft,
                label = { Text(stringResource(R.string.ui_network_id_e_g_la5mr)) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().testTag("cat_follow_network_field"),
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(
                    onClick = onFollowEnable,
                    modifier = Modifier.testTag("cat_btn_follow_on"),
                ) {
                    Text(stringResource(R.string.follow_on))
                }
                OutlinedButton(
                    onClick = onFollowDisable,
                    modifier = Modifier.testTag("cat_btn_follow_off"),
                ) {
                    Text(stringResource(R.string.follow_off))
                }
            }
            Text(stringResource(R.string.ui_nearby_repeaters_150_km), style = MaterialTheme.typography.titleSmall)
            if (nearby.isEmpty()) {
                Text(
                    stringResource(R.string.ui_none_in_the_onboard_db_yet_import_osm_csv_fixtures_or_wait_f),
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
                Text(stringResource(R.string.action_close))
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
