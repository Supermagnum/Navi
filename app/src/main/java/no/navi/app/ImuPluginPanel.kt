package no.navi.app

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.navi.ImuAlarmSettings

private fun fieldText(value: Double): String = if (value % 1.0 == 0.0) value.toInt().toString() else value.toString()

/**
 * IMU plugin controls: enable switch, level calibration (Calibrate / Reset with the
 * verbatim instruction), calibration date and status, live pitch / tilt / compass,
 * and the tilt alarm thresholds.
 */
@Composable
fun ImuPluginPanel(
    modifier: Modifier = Modifier,
    controller: ImuPluginController = ImuPluginRuntime.controller,
    onEnabledChange: (Boolean) -> Unit = { ImuPluginRuntime.setEnabled(it) },
) {
    val ui by controller.state.collectAsState()
    val scope = rememberCoroutineScope()
    Column(
        modifier = modifier.fillMaxWidth().testTag("imu_plugin_section"),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text("IMU pitch / tilt / compass")
            Switch(
                checked = ui.enabled,
                onCheckedChange = { on -> scope.launch(Dispatchers.IO) { onEnabledChange(on) } },
                modifier = Modifier.testTag("toggle_imu_plugin"),
            )
        }
        Text(
            IMU_PLUGIN_PURPOSE,
            style = MaterialTheme.typography.bodySmall,
            modifier = Modifier.testTag("imu_purpose"),
        )
        if (!ui.enabled) return@Column
        ui.message?.let {
            Text(it, style = MaterialTheme.typography.bodySmall, modifier = Modifier.testTag("imu_message"))
        }
        ImuReadoutBlock(ui.readout)
        Text(
            IMU_CALIBRATION_INSTRUCTION,
            style = MaterialTheme.typography.bodySmall,
            modifier = Modifier.testTag("imu_calibration_instruction"),
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(
                enabled = ui.readout != null,
                onClick = { scope.launch(Dispatchers.IO) { controller.calibrate() } },
                modifier = Modifier.testTag("btn_imu_calibrate"),
            ) { Text("Calibrate") }
            OutlinedButton(
                enabled = ui.readout != null,
                onClick = { scope.launch(Dispatchers.IO) { controller.reset() } },
                modifier = Modifier.testTag("btn_imu_reset"),
            ) { Text("Reset") }
        }
        ImuAlarmSettingsBlock(controller, active = ui.initialized)
    }
}

@Composable
private fun ImuReadoutBlock(readout: ImuReadout?) {
    if (readout == null) {
        Text(
            "Waiting for sensor data...",
            style = MaterialTheme.typography.bodySmall,
            modifier = Modifier.testTag("imu_status"),
        )
        return
    }
    Text(readout.statusText, modifier = Modifier.testTag("imu_status"))
    Text(
        "Calibration date: ${readout.calibratedDate ?: "not calibrated"}",
        style = MaterialTheme.typography.bodySmall,
        modifier = Modifier.testTag("imu_calibration_date"),
    )
    readout.noticeText?.let {
        Text(it, style = MaterialTheme.typography.bodySmall, modifier = Modifier.testTag("imu_notice"))
    }
    if (readout.status != ImuStatus.NO_SENSOR) {
        Text(
            "Pitch ${formatImuDegrees(readout.pitchDeg)}  Roll ${formatImuDegrees(readout.rollDeg)}  " +
                "Tilt ${formatImuDegrees(readout.tiltDeg)}  Heading ${formatImuHeading(readout.headingDeg)}",
            modifier = Modifier.testTag("imu_values"),
        )
    }
    if (readout.alarm != ImuAlarmLevel.NONE) {
        Text(
            if (readout.alarm == ImuAlarmLevel.DANGER) "Tilt alarm: DANGER" else "Tilt alarm: warning",
            color = MaterialTheme.colorScheme.error,
            modifier = Modifier.testTag("imu_alarm_level"),
        )
    }
    val sensors = readout.sensors.labels()
    Text(
        "Sensors: ${if (sensors.isEmpty()) "none" else sensors.joinToString(", ")}",
        style = MaterialTheme.typography.bodySmall,
        modifier = Modifier.testTag("imu_sensors"),
    )
}

@Composable
private fun ImuAlarmSettingsBlock(
    controller: ImuPluginController,
    active: Boolean,
) {
    val scope = rememberCoroutineScope()
    var settings by remember { mutableStateOf<ImuAlarmSettings?>(null) }
    var pitchWarn by remember { mutableStateOf("") }
    var pitchDanger by remember { mutableStateOf("") }
    var rollWarn by remember { mutableStateOf("") }
    var rollDanger by remember { mutableStateOf("") }
    var feedback by remember { mutableStateOf<String?>(null) }

    fun show(s: ImuAlarmSettings) {
        settings = s
        pitchWarn = fieldText(s.pitchWarnDeg)
        pitchDanger = fieldText(s.pitchDangerDeg)
        rollWarn = fieldText(s.rollWarnDeg)
        rollDanger = fieldText(s.rollDangerDeg)
    }

    LaunchedEffect(active) {
        if (active) show(withContext(Dispatchers.IO) { controller.alarmSettings() })
    }
    val current = settings ?: return

    Text("Tilt alarm", style = MaterialTheme.typography.titleSmall)
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text("Sound alarm on dangerous tilt")
        Switch(
            checked = current.enabled,
            onCheckedChange = { on ->
                scope.launch {
                    show(withContext(Dispatchers.IO) { controller.setAlarmSettings(current.copy(enabled = on)) })
                }
            },
            modifier = Modifier.testTag("toggle_imu_alarm"),
        )
    }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        ThresholdField("Pitch warn", pitchWarn, "imu_pitch_warn", Modifier.weight(1f)) { pitchWarn = it }
        ThresholdField("Pitch danger", pitchDanger, "imu_pitch_danger", Modifier.weight(1f)) { pitchDanger = it }
    }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        ThresholdField("Roll warn", rollWarn, "imu_roll_warn", Modifier.weight(1f)) { rollWarn = it }
        ThresholdField("Roll danger", rollDanger, "imu_roll_danger", Modifier.weight(1f)) { rollDanger = it }
    }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Button(
            onClick = {
                val pw = parseImuThresholdDegrees(pitchWarn)
                val pd = parseImuThresholdDegrees(pitchDanger)
                val rw = parseImuThresholdDegrees(rollWarn)
                val rd = parseImuThresholdDegrees(rollDanger)
                if (pw == null || pd == null || rw == null || rd == null) {
                    feedback = "Enter thresholds between 1 and 89 degrees."
                } else {
                    scope.launch {
                        val saved =
                            withContext(Dispatchers.IO) {
                                controller.setAlarmSettings(
                                    current.copy(
                                        pitchWarnDeg = pw,
                                        pitchDangerDeg = pd,
                                        rollWarnDeg = rw,
                                        rollDangerDeg = rd,
                                    ),
                                )
                            }
                        show(saved)
                        feedback = "Alarm thresholds saved."
                    }
                }
            },
            modifier = Modifier.testTag("btn_imu_alarm_save"),
        ) { Text("Save alarm") }
        OutlinedButton(
            onClick = { controller.testAlarm(ImuAlarmTone.DANGER) },
            modifier = Modifier.testTag("btn_imu_alarm_test"),
        ) { Text("Test sound") }
    }
    feedback?.let {
        Text(it, style = MaterialTheme.typography.bodySmall, modifier = Modifier.testTag("imu_alarm_feedback"))
    }
}

@Composable
private fun ThresholdField(
    label: String,
    value: String,
    tag: String,
    modifier: Modifier,
    onChange: (String) -> Unit,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text("$label (deg)") },
        singleLine = true,
        modifier = modifier.testTag(tag),
    )
}
