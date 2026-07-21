package no.navi.app

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.unit.dp
import uniffi.navi.FfiCarRestSettings
import uniffi.navi.FfiFuelConfig
import uniffi.navi.FfiIconTheme
import uniffi.navi.loadCarRestSettings
import uniffi.navi.loadFuelConfig
import uniffi.navi.rasterizeIconPng
import uniffi.navi.saveCarRestSettings
import uniffi.navi.saveFuelConfig

enum class MapRotationMode {
    Compass,
    DirectionOfTravel,
    NorthUp,
}

data class DriveHudState(
    val distanceToTurnKm: Double? = null,
    val minutesToBreak: Double? = null,
    val ecoActive: Boolean = false,
    val showTripEta: Boolean = false,
    val tripEtaMinutes: Double? = null,
    val breakRemindersEnabled: Boolean = true,
    val rotationMode: MapRotationMode = MapRotationMode.NorthUp,
    val autoZoomWhileMoving: Boolean = false,
    val autoZoomLevel: Double = 16.5,
)

/**
 * Top map-control bar: rotation mode, trip ETA toggle, break reminders, zoom, auto-zoom.
 */
@Composable
fun TopDriveHud(
    state: DriveHudState,
    onRotation: (MapRotationMode) -> Unit,
    onToggleTripEta: (Boolean) -> Unit,
    onToggleBreakReminders: (Boolean) -> Unit,
    onZoomIn: () -> Unit,
    onZoomOut: () -> Unit,
    onToggleAutoZoom: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(
        shape = RoundedCornerShape(10.dp),
        tonalElevation = 3.dp,
        modifier = modifier.fillMaxWidth(),
    ) {
        Column(
            modifier = Modifier.padding(8.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Row(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("Map", style = MaterialTheme.typography.labelLarge)
                FilterChip(
                    selected = state.rotationMode == MapRotationMode.Compass,
                    onClick = { onRotation(MapRotationMode.Compass) },
                    label = { Text("Compass") },
                )
                FilterChip(
                    selected = state.rotationMode == MapRotationMode.DirectionOfTravel,
                    onClick = { onRotation(MapRotationMode.DirectionOfTravel) },
                    label = { Text("Travel") },
                )
                FilterChip(
                    selected = state.rotationMode == MapRotationMode.NorthUp,
                    onClick = { onRotation(MapRotationMode.NorthUp) },
                    label = { Text("N-up") },
                )
            }
            Row(
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Trip ETA", style = MaterialTheme.typography.bodySmall)
                    Switch(checked = state.showTripEta, onCheckedChange = onToggleTripEta)
                }
                if (state.showTripEta) {
                    val eta = state.tripEtaMinutes?.let { String.format("%.0f min", it) } ?: "--"
                    Text(eta, style = MaterialTheme.typography.bodySmall)
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Breaks", style = MaterialTheme.typography.bodySmall)
                    Switch(
                        checked = state.breakRemindersEnabled,
                        onCheckedChange = onToggleBreakReminders,
                    )
                }
            }
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                TextButton(onClick = onZoomOut) { Text("-") }
                TextButton(onClick = onZoomIn) { Text("+") }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Auto-zoom", style = MaterialTheme.typography.bodySmall)
                    Switch(
                        checked = state.autoZoomWhileMoving,
                        onCheckedChange = onToggleAutoZoom,
                    )
                }
            }
        }
    }
}

/**
 * Bottom in-drive HUD: distance to turn, time to break, eco leaf when active.
 * Tap opens the drive settings sheet.
 */
@Composable
fun BottomDriveHud(
    state: DriveHudState,
    iconDir: String,
    onOpenSettings: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var leafBmp by remember { mutableStateOf<android.graphics.Bitmap?>(null) }
    LaunchedEffect(iconDir, state.ecoActive) {
        if (!state.ecoActive) {
            leafBmp = null
            return@LaunchedEffect
        }
        val png = runCatching {
            rasterizeIconPng(
                key = "eco-mode",
                theme = FfiIconTheme.DAY,
                width = 64u,
                height = 64u,
                bundledDir = iconDir,
            )
        }.getOrNull()
        leafBmp = png?.let { BitmapFactory.decodeByteArray(it, 0, it.size) }
    }

    Surface(
        shape = RoundedCornerShape(10.dp),
        tonalElevation = 4.dp,
        modifier = modifier
            .fillMaxWidth()
            .clickable(onClick = onOpenSettings),
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                val dist = state.distanceToTurnKm?.let { String.format("%.1f km", it) }
                    ?: "Turn --"
                Text(dist, style = MaterialTheme.typography.titleMedium)
                val breakTxt = when {
                    !state.breakRemindersEnabled -> "Break reminders off"
                    state.minutesToBreak != null ->
                        String.format("Break in %.0f min", state.minutesToBreak)
                    else -> "Break --"
                }
                Text(breakTxt, style = MaterialTheme.typography.bodySmall)
            }
            if (state.ecoActive && leafBmp != null) {
                Image(
                    bitmap = leafBmp!!.asImageBitmap(),
                    contentDescription = "Eco mode",
                    modifier = Modifier.size(36.dp),
                )
            } else if (state.ecoActive) {
                Text("ECO", color = Color(0xFF2E7D32))
            }
            Text("Settings", style = MaterialTheme.typography.labelMedium)
        }
    }
}

/**
 * Drive settings opened from the bottom HUD. Apply persists and dismisses.
 * Break interval / rest duration write the persisted RestConfig defaults (not trip-only).
 */
@Composable
fun DriveSettingsSheet(
    dataDir: String,
    iconDir: String,
    ecoActive: Boolean,
    onEcoChange: (Boolean) -> Unit,
    onApplied: () -> Unit,
    onDismiss: () -> Unit,
) {
    var breakHours by remember { mutableStateOf("4.0") }
    var restMins by remember { mutableStateOf("15") }
    var tank by remember { mutableStateOf("") }
    var fuelAdded by remember { mutableStateOf("") }
    var preferLiters by remember { mutableStateOf(true) }
    var status by remember { mutableStateOf("") }

    LaunchedEffect(dataDir) {
        val rest = runCatching { loadCarRestSettings(dataDir) }.getOrNull()
        if (rest != null) {
            breakHours = rest.breakIntervalHours.toString()
            restMins = rest.restDurationMinutes.toString()
            onEcoChange(rest.ecoModeEnabled)
        }
        val fuel = runCatching { loadFuelConfig(dataDir) }.getOrNull()
        if (fuel != null) {
            tank = fuel.tankCapacityL?.toString().orEmpty()
            fuelAdded = fuel.fuelAddedL?.toString().orEmpty()
            preferLiters = fuel.preferLiters
        }
    }

    Surface(
        shape = RoundedCornerShape(12.dp),
        tonalElevation = 6.dp,
        modifier = Modifier
            .fillMaxWidth()
            .background(Color.Transparent)
            .padding(10.dp),
    ) {
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text("Drive / vehicle settings", style = MaterialTheme.typography.titleMedium)
            Text(
                "Break and rest values save as the Car profile default (not a one-trip override).",
                style = MaterialTheme.typography.bodySmall,
            )
            OutlinedTextField(
                value = breakHours,
                onValueChange = { breakHours = it },
                label = { Text("Desired hours between breaks (Car)") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedTextField(
                value = restMins,
                onValueChange = { restMins = it },
                label = { Text("Rest time (minutes)") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Eco mode")
                Switch(checked = ecoActive, onCheckedChange = onEcoChange)
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(if (preferLiters) "Units: liters" else "Units: gallons")
                Switch(checked = preferLiters, onCheckedChange = { preferLiters = it })
            }
            val unit = if (preferLiters) "L" else "gal"
            OutlinedTextField(
                value = tank,
                onValueChange = { tank = it },
                label = { Text("Fuel tank capacity ($unit)") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedTextField(
                value = fuelAdded,
                onValueChange = { fuelAdded = it },
                label = { Text("Fuel added ($unit)") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            if (status.isNotBlank()) {
                Text(status, style = MaterialTheme.typography.bodySmall)
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(
                    onClick = {
                        val hours = breakHours.toDoubleOrNull()
                        val mins = restMins.toIntOrNull()
                        if (hours == null || mins == null) {
                            status = "Enter valid break hours and rest minutes"
                            return@Button
                        }
                        val toLiters = if (preferLiters) 1.0 else 3.785411784
                        val tankL = tank.toDoubleOrNull()?.times(toLiters)
                        val addedL = fuelAdded.toDoubleOrNull()?.times(toLiters)
                        val restOk = saveCarRestSettings(
                            dataDir,
                            FfiCarRestSettings(
                                breakIntervalHours = hours,
                                restDurationMinutes = mins.toUInt(),
                                ecoModeEnabled = ecoActive,
                            ),
                        )
                        val fuelOk = saveFuelConfig(
                            dataDir,
                            FfiFuelConfig(
                                tankCapacityL = tankL,
                                fuelAddedL = addedL,
                                preferLiters = preferLiters,
                            ),
                        )
                        if (restOk && fuelOk) {
                            onApplied()
                        } else {
                            status = "Save failed (rest=$restOk fuel=$fuelOk)"
                        }
                        // iconDir reserved for future leaf preview refresh
                        @Suppress("UNUSED_EXPRESSION")
                        iconDir
                    },
                ) { Text("Apply") }
                TextButton(onClick = onDismiss) { Text("Cancel") }
            }
        }
    }
}
