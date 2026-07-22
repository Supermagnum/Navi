package no.navi.app

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
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
    /** GPS altitude in meters (WGS84); null when no fix. */
    val altitudeM: Double? = null,
)

/**
 * Collapsed top drive HUD: altitude (+ short rotation hint). Tap opens [MapSettingsSheet].
 *
 * Height is content-driven (compact single row). A Garmin reference top instruction
 * bar was ~14% of screen height — that figure informs the temporary approach box
 * ([docs/approach-instructions.md]), not this collapsed chrome strip.
 */
@Composable
fun TopDriveHud(
    state: DriveHudState,
    expanded: Boolean,
    onToggleExpanded: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val rotHint = when (state.rotationMode) {
        MapRotationMode.Compass -> "Compass"
        MapRotationMode.DirectionOfTravel -> "Travel"
        MapRotationMode.NorthUp -> "N-up"
    }
    val altTxt = state.altitudeM?.let { String.format("Alt %.0f m", it) } ?: "Alt --"
    Surface(
        shape = RoundedCornerShape(10.dp),
        tonalElevation = 3.dp,
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .testTag("top_drive_hud")
            .clickable(onClick = onToggleExpanded)
            .semantics {
                contentDescription = if (expanded) {
                    "Map settings open. $altTxt"
                } else {
                    "Map bar. $altTxt. Tap for map settings"
                }
            },
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Map", style = MaterialTheme.typography.labelLarge)
            Text(
                altTxt,
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.testTag("hud_altitude"),
            )
            Text(
                if (expanded) "Close" else rotHint,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.testTag("top_drive_hud_hint"),
            )
        }
    }
}

/**
 * Map / display settings opened from the collapsed top bar.
 * Apply is not required for toggles (they apply immediately); Close collapses the sheet.
 */
@Composable
fun MapSettingsSheet(
    state: DriveHudState,
    onRotation: (MapRotationMode) -> Unit,
    onToggleTripEta: (Boolean) -> Unit,
    onToggleBreakReminders: (Boolean) -> Unit,
    onToggleAutoZoom: (Boolean) -> Unit,
    onAutoZoomLevelChange: (Double) -> Unit,
    onClose: () -> Unit,
) {
    Surface(
        shape = RoundedCornerShape(12.dp),
        tonalElevation = 6.dp,
        modifier = Modifier
            .fillMaxWidth()
            .testTag("map_settings_sheet")
            .padding(bottom = 4.dp),
    ) {
        // No verticalScroll here: parent top chrome Column already scrolls.
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                "Map / display settings",
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.testTag("map_settings_title"),
            )
            Row(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth(),
            ) {
                FilterChip(
                    selected = state.rotationMode == MapRotationMode.Compass,
                    onClick = { onRotation(MapRotationMode.Compass) },
                    label = { Text("Compass") },
                    modifier = Modifier.testTag("rot_compass"),
                )
                FilterChip(
                    selected = state.rotationMode == MapRotationMode.DirectionOfTravel,
                    onClick = { onRotation(MapRotationMode.DirectionOfTravel) },
                    label = { Text("Travel") },
                    modifier = Modifier.testTag("rot_travel"),
                )
                FilterChip(
                    selected = state.rotationMode == MapRotationMode.NorthUp,
                    onClick = { onRotation(MapRotationMode.NorthUp) },
                    label = { Text("N-up") },
                    modifier = Modifier.testTag("rot_north_up"),
                )
            }
            Row(
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Trip ETA", style = MaterialTheme.typography.bodySmall)
                    Switch(
                        checked = state.showTripEta,
                        onCheckedChange = onToggleTripEta,
                        modifier = Modifier.testTag("toggle_trip_eta"),
                    )
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Breaks", style = MaterialTheme.typography.bodySmall)
                    Switch(
                        checked = state.breakRemindersEnabled,
                        onCheckedChange = onToggleBreakReminders,
                        modifier = Modifier.testTag("toggle_breaks"),
                    )
                }
            }
            Row(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("Auto-zoom", style = MaterialTheme.typography.bodySmall)
                Switch(
                    checked = state.autoZoomWhileMoving,
                    onCheckedChange = onToggleAutoZoom,
                    modifier = Modifier.testTag("toggle_auto_zoom"),
                )
                Text(
                    String.format("z %.1f", state.autoZoomLevel),
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.testTag("auto_zoom_level_label"),
                )
                TextButton(
                    onClick = {
                        onAutoZoomLevelChange(
                            MapHudPrefs.clampZoom(state.autoZoomLevel - 0.5),
                        )
                    },
                    modifier = Modifier.testTag("auto_zoom_level_out"),
                ) { Text("-") }
                TextButton(
                    onClick = {
                        onAutoZoomLevelChange(
                            MapHudPrefs.clampZoom(state.autoZoomLevel + 0.5),
                        )
                    },
                    modifier = Modifier.testTag("auto_zoom_level_in"),
                ) { Text("+") }
            }
            TextButton(
                onClick = onClose,
                modifier = Modifier.testTag("btn_close_map_settings"),
            ) { Text("Close") }
        }
    }
}

/**
 * Collapsed bottom drive HUD: the app's only map zoom −/+, break countdown, trip ETA,
 * and eco leaf when eco is active. Tap the status area (not zoom) to open drive settings.
 *
 * Turn / maneuver stubs belong to the approach-instruction box (deferred) — not this bar.
 * AAOS system chrome may show separate − N + climate controls; those are not app zoom.
 *
 * Height is content-driven (~one compact row). A Garmin reference bottom strip was
 * ~6.4% of screen height — use that as a floor for legibility, not a hard cap.
 */
@Composable
fun BottomDriveHud(
    state: DriveHudState,
    iconDir: String,
    onZoomIn: () -> Unit,
    onZoomOut: () -> Unit,
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
            .heightIn(min = 46.dp)
            .testTag("bottom_drive_hud"),
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 8.dp, vertical = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // Sole app-owned map zoom controls (always visible when chrome is on).
            TextButton(
                onClick = onZoomOut,
                modifier = Modifier.testTag("zoom_out"),
            ) { Text("-") }
            TextButton(
                onClick = onZoomIn,
                modifier = Modifier.testTag("zoom_in"),
            ) { Text("+") }
            Column(
                modifier = Modifier
                    .weight(1f)
                    .testTag("btn_open_drive_settings")
                    .clickable(onClick = onOpenSettings)
                    .padding(horizontal = 4.dp),
            ) {
                val breakTxt = when {
                    !state.breakRemindersEnabled -> "Break reminders off"
                    state.minutesToBreak != null ->
                        String.format("Break in %.0f min", state.minutesToBreak)
                    else -> "Break --"
                }
                Text(breakTxt, style = MaterialTheme.typography.titleMedium)
                val etaTxt = when {
                    !state.showTripEta -> "ETA off"
                    state.tripEtaMinutes != null ->
                        String.format("ETA %.0f min", state.tripEtaMinutes)
                    else -> "ETA --"
                }
                Text(
                    etaTxt,
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.testTag("hud_trip_eta"),
                )
            }
            // Eco lives on the bottom bar only (not the approach box; not the top bar).
            if (state.ecoActive) {
                if (leafBmp != null) {
                    Image(
                        bitmap = leafBmp!!.asImageBitmap(),
                        contentDescription = "Eco mode",
                        modifier = Modifier
                            .size(36.dp)
                            .testTag("hud_eco_icon"),
                    )
                } else {
                    Text(
                        "ECO",
                        color = Color(0xFF2E7D32),
                        style = MaterialTheme.typography.titleMedium,
                        modifier = Modifier.testTag("hud_eco_icon"),
                    )
                }
            }
        }
    }
}

/**
 * Drive / rest / fuel settings opened from the collapsed bottom HUD.
 * Apply persists and dismisses; Cancel / Close dismisses without requiring a change.
 * Auto-zoom lives in [MapSettingsSheet], not here.
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
            .padding(10.dp)
            .heightIn(max = 360.dp)
            .testTag("drive_settings_sheet"),
    ) {
        Column(
            modifier = Modifier
                .padding(12.dp)
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                "Drive / vehicle settings",
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.testTag("drive_settings_title"),
            )
            Text(
                "Break and rest values save as the Car profile default (not a one-trip override).",
                style = MaterialTheme.typography.bodySmall,
            )
            OutlinedTextField(
                value = breakHours,
                onValueChange = { breakHours = it },
                label = { Text("Desired hours between breaks (Car)") },
                singleLine = true,
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("field_break_hours"),
            )
            OutlinedTextField(
                value = restMins,
                onValueChange = { restMins = it },
                label = { Text("Rest time (minutes)") },
                singleLine = true,
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("field_rest_mins"),
            )
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Eco mode")
                Switch(
                    checked = ecoActive,
                    onCheckedChange = onEcoChange,
                    modifier = Modifier.testTag("toggle_eco"),
                )
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(if (preferLiters) "Units: liters" else "Units: gallons")
                Switch(
                    checked = preferLiters,
                    onCheckedChange = { preferLiters = it },
                    modifier = Modifier.testTag("toggle_fuel_units"),
                )
            }
            val unit = if (preferLiters) "L" else "gal"
            OutlinedTextField(
                value = tank,
                onValueChange = { tank = it },
                label = { Text("Fuel tank capacity ($unit)") },
                singleLine = true,
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("field_tank"),
            )
            OutlinedTextField(
                value = fuelAdded,
                onValueChange = { fuelAdded = it },
                label = { Text("Fuel added ($unit)") },
                singleLine = true,
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("field_fuel_added"),
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
                        @Suppress("UNUSED_EXPRESSION")
                        iconDir
                    },
                    modifier = Modifier.testTag("btn_apply_drive_settings"),
                ) { Text("Apply") }
                TextButton(
                    onClick = onDismiss,
                    modifier = Modifier.testTag("btn_cancel_drive_settings"),
                ) { Text("Cancel") }
            }
        }
    }
}
