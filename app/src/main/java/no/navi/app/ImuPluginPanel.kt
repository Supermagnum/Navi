package no.navi.app

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Button
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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

/**
 * IMU plugin controls: enable switch, level calibration (Calibrate / Reset with the
 * verbatim instruction), calibration date and status, live pitch / tilt / compass,
 * and calibration.
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
    val sensors = readout.sensors.labels()
    Text(
        "Sensors: ${if (sensors.isEmpty()) "none" else sensors.joinToString(", ")}",
        style = MaterialTheme.typography.bodySmall,
        modifier = Modifier.testTag("imu_sensors"),
    )
}
