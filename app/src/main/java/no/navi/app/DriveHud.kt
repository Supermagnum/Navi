package no.navi.app

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
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
import androidx.compose.material3.Slider
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
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import uniffi.navi.FfiCarRestSettings
import uniffi.navi.FfiEbikeConfig
import uniffi.navi.FfiEvCarConfig
import uniffi.navi.FfiFuelConfig
import uniffi.navi.FfiIconTheme
import uniffi.navi.FfiTruckRestSettings
import uniffi.navi.TravelProfile
import uniffi.navi.ecoModeDefault
import uniffi.navi.ecoModeToggleable
import uniffi.navi.loadCarRestSettings
import uniffi.navi.loadEbikeConfig
import uniffi.navi.loadEvCarConfig
import uniffi.navi.loadFuelConfig
import uniffi.navi.loadTruckRestSettings
import uniffi.navi.rasterizeIconPng
import uniffi.navi.saveCarRestSettings
import uniffi.navi.saveEbikeConfig
import uniffi.navi.saveEvCarConfig
import uniffi.navi.saveFuelConfig
import uniffi.navi.saveTruckRestSettings
import uniffi.navi.travelProfileMenuFocus

/** Profiles that use TruckRestParams / EC 561 (commercial HGV cadence). */
fun usesTruckRestSettings(profile: TravelProfile): Boolean =
    profile == TravelProfile.TRUCK ||
        profile == TravelProfile.TRUCK_ELECTRIC
// Mobile home uses car-style soft break reminders — not EC 561 legal tracking.

fun usesEbikeVehicleSpecs(profile: TravelProfile): Boolean = profile == TravelProfile.BICYCLE_ELECTRIC

fun usesEvCarBatterySpecs(profile: TravelProfile): Boolean = profile == TravelProfile.CAR_ELECTRIC

private val EBIKE_WHEEL_PRESETS_IN = listOf(20.0, 26.0, 27.5, 29.0)

fun travelProfileChipLabel(profile: TravelProfile): String =
    when (profile) {
        TravelProfile.CAR -> "Car"
        TravelProfile.CAR_ELECTRIC -> "Electric car"
        TravelProfile.BICYCLE -> "Bicycle"
        TravelProfile.BICYCLE_ELECTRIC -> "Electric cycle"
        TravelProfile.HIKING -> "Hiking"
        TravelProfile.MOTORCYCLE -> "Motorcycle"
        TravelProfile.TRUCK -> "Truck"
        TravelProfile.MOBILE_HOME -> "Mobile home"
        else -> profile.name
    }

/** Hours until the next mandatory break for the active profile. */
fun breakIntervalHoursForProfile(
    dataDir: String,
    profile: TravelProfile,
): Double =
    if (usesTruckRestSettings(profile)) {
        runCatching { loadTruckRestSettings(dataDir).mandatoryBreakAfterHours }
            .getOrDefault(4.5)
    } else {
        runCatching { loadCarRestSettings(dataDir).breakIntervalHours }
            .getOrDefault(4.0)
    }

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
    /** When true, format break remaining as distance; else as time. */
    val breakAsDistance: Boolean = false,
    val preferMetric: Boolean = true,
    val rotationMode: MapRotationMode = MapRotationMode.NorthUp,
    val autoZoomWhileMoving: Boolean = false,
    val autoZoomLevel: Double = 16.5,
    /** Terrain / GPS altitude in meters; null when unknown. Prefer DEM when present. */
    val altitudeM: Double? = null,
    /** Opt-in Mapterhorn DEM hillshade 3D (online). Never default-on. */
    val optIn3d: Boolean = false,
    /** Camera tilt in degrees; snapped to [MapHudPrefs.CAMERA_TILT_PRESETS]. */
    val cameraTiltDeg: Double = MapHudPrefs.DEFAULT_CAMERA_TILT_DEG,
    /** Vulkan SDK linked — gate for offering 3D. */
    val vulkanAvailable: Boolean = true,
    /**
     * Road the vehicle is currently on (bottom bar, low visual weight).
     * Null/blank = omit the line. Updated from live route sample snap when a
     * corridor is active; cleared when navigation ends (see docs/current-street.md).
     */
    val currentStreet: String? = null,
)

/**
 * Format the bottom-HUD break line, or null when nothing should be shown.
 *
 * Hard rule: callers must pass [routePlanned]=false when no corridor is active —
 * this function never invents break copy without a route.
 */
fun formatBreakHudLine(
    routePlanned: Boolean,
    breakRemindersEnabled: Boolean,
    minutesToBreak: Double?,
    breakAsDistance: Boolean,
    preferMetric: Boolean,
    cruiseSpeedKmh: Double = MapHudPrefs.BREAK_DISPLAY_SPEED_KMH,
): String? {
    if (!routePlanned) return null
    if (!breakRemindersEnabled) return "Break reminders off"
    val mins = minutesToBreak ?: return null
    return if (breakAsDistance) {
        val km = (mins / 60.0) * cruiseSpeedKmh
        if (preferMetric) {
            String.format("Break in %.0f km", km)
        } else {
            String.format("Break in %.0f mi", km / 1.609344)
        }
    } else {
        String.format("Break in %.0f min", mins)
    }
}

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
    val rotHint =
        when (state.rotationMode) {
            MapRotationMode.Compass -> "Compass"
            MapRotationMode.DirectionOfTravel -> "Travel"
            MapRotationMode.NorthUp -> "N-up"
        }
    val altTxt = state.altitudeM?.let { String.format("Alt %.0f m", it) } ?: "Alt --"
    Surface(
        shape = RoundedCornerShape(10.dp),
        tonalElevation = 3.dp,
        modifier =
            modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp)
                .testTag("top_drive_hud")
                .clickable(onClick = onToggleExpanded)
                .semantics {
                    contentDescription =
                        if (expanded) {
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
 * Toggles apply immediately; Save persists prefs and closes; Close dismisses.
 */
@Composable
fun MapSettingsSheet(
    state: DriveHudState,
    onRotation: (MapRotationMode) -> Unit,
    onToggleTripEta: (Boolean) -> Unit,
    onToggleBreakReminders: (Boolean) -> Unit,
    onToggleAutoZoom: (Boolean) -> Unit,
    onAutoZoomLevelChange: (Double) -> Unit,
    onToggle3d: (Boolean) -> Unit = {},
    onCameraTiltChange: (Double) -> Unit = {},
    onSave: () -> Unit,
    onClose: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(
        shape = RoundedCornerShape(12.dp),
        tonalElevation = 8.dp,
        shadowElevation = 8.dp,
        modifier =
            modifier
                .fillMaxWidth()
                .testTag("map_settings_sheet"),
    ) {
        Column(
            modifier =
                Modifier
                    .padding(12.dp)
                    .heightIn(max = 420.dp)
                    .verticalScroll(rememberScrollState()),
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
                    String.format(java.util.Locale.US, "z %.1f", state.autoZoomLevel),
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
            Row(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("3D (experimental)", style = MaterialTheme.typography.bodySmall)
                Switch(
                    checked = state.optIn3d && state.vulkanAvailable,
                    enabled = state.vulkanAvailable,
                    onCheckedChange = onToggle3d,
                    modifier = Modifier.testTag("toggle_basemap_3d"),
                )
                if (!state.vulkanAvailable) {
                    Text(
                        "Unavailable on this GPU path",
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
            Column(
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .testTag("map_tilt_control"),
            ) {
                val presets = MapHudPrefs.CAMERA_TILT_PRESETS
                val tilt = MapHudPrefs.snapTilt(state.cameraTiltDeg)
                val tiltIdx = presets.indexOfFirst { it == tilt }.coerceAtLeast(0)
                Text(
                    String.format("Map tilt %.0f deg", tilt),
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.testTag("map_tilt_label"),
                )
                Slider(
                    value = tiltIdx.toFloat(),
                    onValueChange = { v ->
                        val i = v.toInt().coerceIn(0, presets.lastIndex)
                        onCameraTiltChange(presets[i])
                    },
                    valueRange = 0f..presets.lastIndex.toFloat(),
                    steps = (presets.size - 2).coerceAtLeast(0),
                    enabled = state.vulkanAvailable,
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .testTag("map_tilt_slider"),
                )
                Row(
                    horizontalArrangement = Arrangement.SpaceBetween,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    for (p in presets) {
                        Text(
                            String.format("%.0f", p),
                            style = MaterialTheme.typography.labelSmall,
                        )
                    }
                }
                if (!state.vulkanAvailable) {
                    Text(
                        "Tilt locked at 0 without Vulkan",
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(
                    onClick = onSave,
                    modifier = Modifier.testTag("btn_save_map_settings"),
                ) { Text("Save") }
                TextButton(
                    onClick = onClose,
                    modifier = Modifier.testTag("btn_close_map_settings"),
                ) { Text("Close") }
            }
        }
    }
}

/**
 * Collapsed bottom drive HUD: the app's only map zoom −/+, optional Recenter
 * (after a manual pan away from GPS), current street (low weight), break
 * countdown, trip ETA, and eco leaf when eco is active. Tap the status area
 * (not zoom) to open drive settings.
 *
 * Turn / maneuver stubs belong to the approach-instruction box — not this bar.
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
    /** Break countdown only applies while a corridor/route is active. */
    routePlanned: Boolean = false,
    /** Show a control to re-enable GPS camera follow after a manual pan. */
    showRecenter: Boolean = false,
    onRecenter: () -> Unit = {},
    modifier: Modifier = Modifier,
) {
    var leafBmp by remember { mutableStateOf<android.graphics.Bitmap?>(null) }
    LaunchedEffect(iconDir, state.ecoActive) {
        if (!state.ecoActive) {
            leafBmp = null
            return@LaunchedEffect
        }
        // Same resolution path as POI/nav/status: aliases eco-mode / eco → leaf.svg.
        var decoded: android.graphics.Bitmap? = null
        for (key in listOf("eco-mode", "eco", "leaf")) {
            val png =
                runCatching {
                    rasterizeIconPng(
                        key = key,
                        theme = FfiIconTheme.DAY,
                        width = 64u,
                        height = 64u,
                        bundledDir = iconDir,
                    )
                }.getOrNull() ?: continue
            decoded = BitmapFactory.decodeByteArray(png, 0, png.size)
            if (decoded != null) break
        }
        leafBmp = decoded
    }

    Surface(
        shape = RoundedCornerShape(10.dp),
        tonalElevation = 4.dp,
        modifier =
            modifier
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
            if (showRecenter) {
                TextButton(
                    onClick = onRecenter,
                    modifier = Modifier.testTag("btn_recenter_gps"),
                ) { Text("Recenter") }
            }
            Column(
                modifier =
                    Modifier
                        .weight(1f)
                        .testTag("btn_open_drive_settings")
                        .clickable(onClick = onOpenSettings)
                        .padding(horizontal = 4.dp),
            ) {
                val street = state.currentStreet?.trim().orEmpty()
                if (street.isNotEmpty()) {
                    Text(
                        "Currently on $street",
                        style = MaterialTheme.typography.labelSmall,
                        maxLines = 1,
                        softWrap = false,
                        modifier = Modifier.testTag("hud_current_street"),
                    )
                }
                val breakTxt =
                    formatBreakHudLine(
                        routePlanned = routePlanned,
                        breakRemindersEnabled = state.breakRemindersEnabled,
                        minutesToBreak = state.minutesToBreak,
                        breakAsDistance = state.breakAsDistance,
                        preferMetric = state.preferMetric,
                    )
                if (breakTxt != null) {
                    Text(
                        breakTxt,
                        style = MaterialTheme.typography.titleMedium,
                        modifier = Modifier.testTag("hud_break_countdown"),
                    )
                }
                val etaTxt =
                    when {
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
            // Eco lives on the bottom bar only — rasterized leaf.svg (not a text label).
            if (state.ecoActive && leafBmp != null) {
                Image(
                    bitmap = leafBmp!!.asImageBitmap(),
                    contentDescription = "Eco mode",
                    modifier =
                        Modifier
                            .size(36.dp)
                            .testTag("hud_eco_icon"),
                )
            }
        }
    }
}

/**
 * Drive / rest / fuel settings opened from the collapsed bottom HUD.
 * Save persists and dismisses; Close dismisses without requiring a change.
 * Auto-zoom lives in [MapSettingsSheet], not here.
 */
@Composable
fun DriveSettingsSheet(
    dataDir: String,
    iconDir: String,
    travelProfile: TravelProfile,
    onTravelProfileChange: (TravelProfile) -> Unit,
    ecoActive: Boolean,
    onEcoChange: (Boolean) -> Unit,
    breakAsDistance: Boolean,
    onBreakAsDistanceChange: (Boolean) -> Unit,
    preferMetric: Boolean,
    onPreferMetricChange: (Boolean) -> Unit,
    onApplied: () -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var breakHours by remember { mutableStateOf("4.0") }
    var restMins by remember { mutableStateOf("15") }
    var preferSplitBreak by remember { mutableStateOf(false) }
    var exceptionalArmed by remember { mutableStateOf(false) }
    var tank by remember { mutableStateOf("") }
    var fuelAdded by remember { mutableStateOf("") }
    var preferLiters by remember { mutableStateOf(true) }
    var batteryWh by remember { mutableStateOf("800") }
    var motorTorqueNm by remember { mutableStateOf("85") }
    var wheelDiameterIn by remember { mutableStateOf("27.5") }
    var wheelCustom by remember { mutableStateOf(false) }
    var evBatteryKwh by remember { mutableStateOf("60") }
    var status by remember { mutableStateOf("") }
    var asDistance by remember { mutableStateOf(breakAsDistance) }
    var metric by remember { mutableStateOf(preferMetric) }
    val truckRest = usesTruckRestSettings(travelProfile)
    val ebikeSpecs = usesEbikeVehicleSpecs(travelProfile)
    val evCarSpecs = usesEvCarBatterySpecs(travelProfile)

    LaunchedEffect(dataDir, travelProfile) {
        if (usesTruckRestSettings(travelProfile)) {
            val rest = runCatching { loadTruckRestSettings(dataDir) }.getOrNull()
            if (rest != null) {
                breakHours = rest.mandatoryBreakAfterHours.toString()
                restMins = rest.breakDurationMinutes.toString()
                preferSplitBreak = rest.preferSplitBreak
                exceptionalArmed = rest.exceptionalExtensionArmed
                onEcoChange(rest.ecoModeEnabled)
            }
        } else {
            val rest = runCatching { loadCarRestSettings(dataDir) }.getOrNull()
            if (rest != null) {
                breakHours = rest.breakIntervalHours.toString()
                restMins = rest.restDurationMinutes.toString()
                preferSplitBreak = false
                exceptionalArmed = false
                onEcoChange(rest.ecoModeEnabled)
            }
        }
        val fuel = runCatching { loadFuelConfig(dataDir) }.getOrNull()
        if (fuel != null) {
            tank = fuel.tankCapacityL?.toString().orEmpty()
            fuelAdded = fuel.fuelAddedL?.toString().orEmpty()
            preferLiters = fuel.preferLiters
        }
        val ebike = runCatching { loadEbikeConfig(dataDir) }.getOrNull()
        if (ebike != null) {
            batteryWh = ebike.batteryCapacityWh?.toString() ?: "800"
            motorTorqueNm = ebike.motorTorqueNm?.toString() ?: "85"
            val wd = ebike.wheelDiameterIn ?: 27.5
            wheelDiameterIn = wd.toString()
            wheelCustom = EBIKE_WHEEL_PRESETS_IN.none { kotlin.math.abs(it - wd) < 1e-6 }
        }
        val evCar = runCatching { loadEvCarConfig(dataDir) }.getOrNull()
        if (evCar != null) {
            evBatteryKwh = evCar.batteryCapacityKwh?.toString() ?: "60"
        }
        asDistance = breakAsDistance
        metric = preferMetric
    }

    Surface(
        shape = RoundedCornerShape(12.dp),
        tonalElevation = 8.dp,
        shadowElevation = 8.dp,
        modifier =
            modifier
                .fillMaxWidth()
                .heightIn(max = 420.dp)
                .testTag("drive_settings_sheet"),
    ) {
        Column(
            modifier =
                Modifier
                    .padding(12.dp)
                    .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                "Drive / vehicle settings",
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.testTag("drive_settings_title"),
            )
            Text("Travel mode", style = MaterialTheme.typography.labelLarge)
            Row(
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .horizontalScroll(rememberScrollState())
                        .testTag("drive_settings_profiles"),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                TravelProfile.entries.filter { travelProfileMenuFocus(it) }.forEach { p ->
                    FilterChip(
                        selected = travelProfile == p,
                        onClick = {
                            onTravelProfileChange(p)
                            if (!ecoModeToggleable(p)) {
                                onEcoChange(ecoModeDefault(p))
                            }
                        },
                        label = { Text(travelProfileChipLabel(p)) },
                        modifier =
                            Modifier.testTag(
                                when (p) {
                                    TravelProfile.HIKING -> "drive_chip_profile_hiking"
                                    TravelProfile.CAR -> "drive_chip_profile_car"
                                    TravelProfile.BICYCLE_ELECTRIC ->
                                        "drive_chip_profile_bicycle_electric"
                                    else -> "drive_chip_profile_${p.name.lowercase()}"
                                },
                            ),
                    )
                }
            }
            Text(
                if (truckRest) {
                    "Break / rest values save as Truck EC 561/2006 defaults (not a one-trip override)."
                } else {
                    "Break and rest values save as the Car profile default (not a one-trip override)."
                },
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.testTag("drive_rest_profile_hint"),
            )
            OutlinedTextField(
                value = breakHours,
                onValueChange = { breakHours = it },
                label = {
                    Text(
                        if (truckRest) {
                            "Mandatory break after (hours, Truck)"
                        } else {
                            "Desired hours between breaks (Car)"
                        },
                    )
                },
                singleLine = true,
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .testTag("field_break_hours"),
            )
            OutlinedTextField(
                value = restMins,
                onValueChange = { restMins = it },
                label = {
                    Text(
                        if (truckRest) {
                            "Break duration (minutes, continuous)"
                        } else {
                            "Rest time (minutes)"
                        },
                    )
                },
                singleLine = true,
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .testTag("field_rest_mins"),
            )
            if (truckRest) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Split break 15+30 min")
                    Switch(
                        checked = preferSplitBreak,
                        onCheckedChange = { preferSplitBreak = it },
                        modifier = Modifier.testTag("toggle_truck_split_break"),
                    )
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Arm +1 h exceptional extension")
                    Switch(
                        checked = exceptionalArmed,
                        onCheckedChange = { exceptionalArmed = it },
                        modifier = Modifier.testTag("toggle_truck_exceptional"),
                    )
                }
            }
            Text("Next break shown as", style = MaterialTheme.typography.labelLarge)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(
                    selected = !asDistance,
                    onClick = { asDistance = false },
                    label = { Text("Time") },
                    modifier = Modifier.testTag("chip_break_as_time"),
                )
                FilterChip(
                    selected = asDistance,
                    onClick = { asDistance = true },
                    label = { Text("Distance") },
                    modifier = Modifier.testTag("chip_break_as_distance"),
                )
            }
            if (asDistance) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(if (metric) "Distance units: km" else "Distance units: miles")
                    Switch(
                        checked = metric,
                        onCheckedChange = { metric = it },
                        modifier = Modifier.testTag("toggle_break_metric"),
                    )
                }
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Eco mode")
                Switch(
                    checked = ecoActive,
                    onCheckedChange = onEcoChange,
                    modifier = Modifier.testTag("toggle_eco"),
                )
            }
            if (ebikeSpecs) {
                Text("Electric cycle specs", style = MaterialTheme.typography.labelLarge)
                Text(
                    "Legal assist caps (EU pedelec ~250 W / 25 km/h; US Class 1–3 up to 750 W / 20–28 mph) are not enforced — enter your bike's real specs for planning only.",
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.testTag("ebike_legal_note"),
                )
                OutlinedTextField(
                    value = batteryWh,
                    onValueChange = { batteryWh = it },
                    label = { Text("Battery capacity (Wh)") },
                    singleLine = true,
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .testTag("field_ebike_battery_wh"),
                )
                OutlinedTextField(
                    value = motorTorqueNm,
                    onValueChange = { motorTorqueNm = it },
                    label = { Text("Motor torque (Nm)") },
                    singleLine = true,
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .testTag("field_ebike_torque_nm"),
                )
                Text("Wheel diameter (inches)", style = MaterialTheme.typography.bodySmall)
                Row(
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .horizontalScroll(rememberScrollState())
                            .testTag("ebike_wheel_presets"),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    EBIKE_WHEEL_PRESETS_IN.forEach { preset ->
                        val selected =
                            !wheelCustom &&
                                wheelDiameterIn.toDoubleOrNull()?.let {
                                    kotlin.math.abs(it - preset) < 1e-6
                                } == true
                        FilterChip(
                            selected = selected,
                            onClick = {
                                wheelCustom = false
                                wheelDiameterIn = preset.toString()
                            },
                            label = { Text("${preset}\"") },
                            modifier =
                                Modifier.testTag(
                                    "chip_ebike_wheel_${preset.toString().replace('.', '_')}",
                                ),
                        )
                    }
                    FilterChip(
                        selected = wheelCustom,
                        onClick = { wheelCustom = true },
                        label = { Text("Custom") },
                        modifier = Modifier.testTag("chip_ebike_wheel_custom"),
                    )
                }
                // Always expose the diameter field so tests (and users) can set a
                // value without scrolling a clipped horizontal chip row into view.
                OutlinedTextField(
                    value = wheelDiameterIn,
                    onValueChange = {
                        wheelDiameterIn = it
                        val parsed = it.toDoubleOrNull()
                        wheelCustom = parsed == null ||
                            EBIKE_WHEEL_PRESETS_IN.none { p ->
                                kotlin.math.abs(p - parsed) < 1e-6
                            }
                    },
                    label = { Text("Wheel diameter (inches)") },
                    singleLine = true,
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .testTag("field_ebike_wheel_in"),
                )
            } else if (evCarSpecs) {
                Text("Electric car battery", style = MaterialTheme.typography.labelLarge)
                Text(
                    "Example default 60 kWh (mid-size EV pack). Used for route range estimate only — not a measured SoC.",
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.testTag("ev_car_battery_note"),
                )
                OutlinedTextField(
                    value = evBatteryKwh,
                    onValueChange = { evBatteryKwh = it },
                    label = { Text("Battery capacity (kWh)") },
                    singleLine = true,
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .testTag("field_ev_car_battery_kwh"),
                )
            } else {
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
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .testTag("field_tank"),
                )
                OutlinedTextField(
                    value = fuelAdded,
                    onValueChange = { fuelAdded = it },
                    label = { Text("Fuel added ($unit)") },
                    singleLine = true,
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .testTag("field_fuel_added"),
                )
            }
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
                        val restOk =
                            if (usesTruckRestSettings(travelProfile)) {
                                val prev = runCatching { loadTruckRestSettings(dataDir) }.getOrNull()
                                saveTruckRestSettings(
                                    dataDir,
                                    FfiTruckRestSettings(
                                        mandatoryBreakAfterHours = hours,
                                        breakDurationMinutes = mins.toUInt(),
                                        preferSplitBreak = preferSplitBreak,
                                        maxDailyDrivingHours = prev?.maxDailyDrivingHours ?: 9.0,
                                        maxDailyDrivingExtendedHours =
                                            prev?.maxDailyDrivingExtendedHours ?: 10.0,
                                        maxDailyExtensionsPerWeek =
                                            prev?.maxDailyExtensionsPerWeek ?: 2u,
                                        maxWeeklyDrivingHours = prev?.maxWeeklyDrivingHours ?: 56.0,
                                        maxFortnightlyDrivingHours =
                                            prev?.maxFortnightlyDrivingHours ?: 90.0,
                                        exceptionalExtensionArmed = exceptionalArmed,
                                        ecoModeEnabled = ecoActive,
                                    ),
                                )
                            } else {
                                saveCarRestSettings(
                                    dataDir,
                                    FfiCarRestSettings(
                                        breakIntervalHours = hours,
                                        restDurationMinutes = mins.toUInt(),
                                        ecoModeEnabled = ecoActive,
                                    ),
                                )
                            }
                        val energyOk =
                            if (ebikeSpecs) {
                                val wh = batteryWh.toDoubleOrNull()
                                val nm = motorTorqueNm.toDoubleOrNull()
                                val wd = wheelDiameterIn.toDoubleOrNull()
                                if (wh == null ||
                                    wh <= 0.0 ||
                                    nm == null ||
                                    nm <= 0.0 ||
                                    wd == null ||
                                    wd <= 0.0
                                ) {
                                    status = "Enter valid battery Wh, torque Nm, and wheel inches"
                                    return@Button
                                }
                                saveEbikeConfig(
                                    dataDir,
                                    FfiEbikeConfig(
                                        batteryCapacityWh = wh,
                                        motorTorqueNm = nm,
                                        wheelDiameterIn = wd,
                                    ),
                                )
                            } else if (evCarSpecs) {
                                val kwh = evBatteryKwh.toDoubleOrNull()
                                if (kwh == null || kwh <= 0.0) {
                                    status = "Enter valid battery capacity (kWh)"
                                    return@Button
                                }
                                saveEvCarConfig(
                                    dataDir,
                                    FfiEvCarConfig(batteryCapacityKwh = kwh),
                                )
                            } else {
                                val toLiters = if (preferLiters) 1.0 else 3.785411784
                                val tankL = tank.toDoubleOrNull()?.times(toLiters)
                                val addedL = fuelAdded.toDoubleOrNull()?.times(toLiters)
                                saveFuelConfig(
                                    dataDir,
                                    FfiFuelConfig(
                                        tankCapacityL = tankL,
                                        fuelAddedL = addedL,
                                        preferLiters = preferLiters,
                                    ),
                                )
                            }
                        onBreakAsDistanceChange(asDistance)
                        onPreferMetricChange(metric)
                        if (restOk && energyOk) {
                            onApplied()
                        } else {
                            status = "Save failed (rest=$restOk energy=$energyOk)"
                        }
                        @Suppress("UNUSED_EXPRESSION")
                        iconDir
                    },
                    modifier = Modifier.testTag("btn_save_drive_settings"),
                ) { Text("Save") }
                TextButton(
                    onClick = onDismiss,
                    modifier = Modifier.testTag("btn_close_drive_settings"),
                ) { Text("Close") }
            }
        }
    }
}
