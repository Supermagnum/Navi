package no.navi.app

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import uniffi.navi.FfiIconTheme
import uniffi.navi.approachPhaseForDistance
import uniffi.navi.formatApproachDistance
import uniffi.navi.rasterizeIconPng

/**
 * Temporary approach-instruction overlay (docs/approach-instructions.md).
 * Informational only — no tap action. Eco stays on the bottom HUD bar.
 *
 * Address stack (top to bottom, each on its own line):
 * 1. Street name — never mid-word wrapped
 * 2. House number (omit if unknown)
 * 3. Postcode (omit if unknown)
 */
data class ApproachGuidanceState(
    val active: Boolean = false,
    /** Distance to next maneuver in meters (shared with voice guidance). */
    val distanceM: Double = Double.POSITIVE_INFINITY,
    /** Navit icon key stem, e.g. nav_right_1 (theme suffix applied at rasterize). */
    val iconKey: String = "nav_straight",
    /** Next street (OSM name, else ref). Null/blank = omit line. */
    val nextStreet: String? = null,
    /** House number on its own line under the street. */
    val houseNumber: String? = null,
    /** Postcode on its own line under the house number. */
    val postcode: String? = null,
    /** Roundabout exit 1..=3 when applicable. */
    val roundaboutExit: Int? = null,
    val preferMetric: Boolean = true,
    /**
     * When true, show an explicit Off route label instead of a (likely wrong)
     * distance / maneuver from global nearest-sample snap.
     */
    val offRoute: Boolean = false,
)

/**
 * Split a freeform place/address label into street / house number / postcode lines.
 * Does not invent values — only separates what is already present.
 */
fun parseAddressDisplayLines(
    street: String? = null,
    houseNumber: String? = null,
    postcode: String? = null,
    combined: String? = null,
): Triple<String?, String?, String?> {
    var s = street?.trim()?.takeIf { it.isNotEmpty() }
    var hn = houseNumber?.trim()?.takeIf { it.isNotEmpty() }
    var pc = postcode?.trim()?.takeIf { it.isNotEmpty() }
    val raw = combined?.trim()?.takeIf { it.isNotEmpty() }
    if (raw != null && s == null) {
        // Trailing 4-digit Norwegian postcode: "Storgata 12 2312" or "2312".
        val postMatch = Regex("""^(.*?)(?:\s+)(\d{4})$""").matchEntire(raw)
        val withoutPost =
            if (postMatch != null && pc == null) {
                pc = postMatch.groupValues[2]
                postMatch.groupValues[1].trim()
            } else {
                raw
            }
        // Trailing housenumber: "Ommangsgutua 12" / "Ommangsgutua 12B".
        val hnMatch = Regex("""^(.*?)(?:\s+)(\d+[A-Za-z]?)$""").matchEntire(withoutPost)
        if (hnMatch != null && hn == null) {
            s = hnMatch.groupValues[1].trim().takeIf { it.isNotEmpty() }
            hn = hnMatch.groupValues[2]
        } else {
            s = withoutPost.takeIf { it.isNotEmpty() }
        }
    }
    return Triple(s, hn, pc)
}

enum class ApproachUiPhase {
    Hidden,
    Appear,
    Urgency,
}

fun approachUiPhase(state: ApproachGuidanceState): ApproachUiPhase {
    if (state.offRoute && state.active) return ApproachUiPhase.Appear
    return when (approachPhaseForDistance(state.active, state.distanceM)) {
        "appear" -> ApproachUiPhase.Appear
        "urgency" -> ApproachUiPhase.Urgency
        else -> ApproachUiPhase.Hidden
    }
}

@Composable
fun ApproachInstructionBox(
    state: ApproachGuidanceState,
    iconsDir: String,
    /** When false (no planned corridor), the box stays hidden even if guidance is active. */
    routePlanned: Boolean,
    modifier: Modifier = Modifier,
) {
    val phase = approachUiPhase(state)
    if (!routePlanned || phase == ApproachUiPhase.Hidden) return

    if (state.offRoute) {
        Box(
            modifier =
                modifier
                    .width(IntrinsicSize.Max)
                    .widthIn(min = 160.dp, max = 280.dp)
                    .background(Color(0xFFFFF3E0), RectangleShape)
                    .border(2.dp, Color(0xFFE65100), RectangleShape)
                    .padding(horizontal = 12.dp, vertical = 10.dp)
                    .testTag("approach_instruction_box")
                    .semantics {
                        contentDescription = "Off route"
                    },
        ) {
            Column {
                Text(
                    text = "Off route",
                    style = MaterialTheme.typography.titleMedium,
                    color = Color(0xFFE65100),
                    modifier = Modifier.testTag("approach_off_route"),
                )
                Text(
                    text = "Recalculating may take several seconds",
                    style = MaterialTheme.typography.bodySmall,
                    color = Color(0xFF5D4037),
                    modifier = Modifier.testTag("approach_off_route_hint"),
                )
            }
        }
        return
    }

    val urgency = phase == ApproachUiPhase.Urgency
    val dist = formatApproachDistance(state.distanceM, state.preferMetric)
    val exitLabel =
        when (state.roundaboutExit) {
            1 -> "first exit"
            2 -> "second exit"
            3 -> "third exit"
            else -> null
        }
    val png =
        remember(state.iconKey, iconsDir) {
            runCatching {
                rasterizeIconPng(
                    key = state.iconKey,
                    theme = FfiIconTheme.DAY,
                    width = if (urgency) 96u else 72u,
                    height = if (urgency) 96u else 72u,
                    bundledDir = iconsDir,
                )
            }.getOrDefault(ByteArray(0))
        }
    val bmp =
        remember(png) {
            if (png.isEmpty()) null else BitmapFactory.decodeByteArray(png, 0, png.size)
        }

    // Compact left-aligned card: intrinsic width hugs icon + text (not fillMaxWidth).
    // Wide enough that common Norwegian street names stay on one line.
    val fill = if (urgency) Color(0xFFE8E0F0) else Color(0xFFEDE8F5)
    val (streetLine, houseLine, postLine) =
        parseAddressDisplayLines(
            street = state.nextStreet,
            houseNumber = state.houseNumber,
            postcode = state.postcode,
        )
    Box(
        modifier =
            modifier
                .width(IntrinsicSize.Max)
                .widthIn(max = 420.dp)
                .heightIn(min = if (urgency) 96.dp else 80.dp)
                .background(fill, RectangleShape)
                .border(1.dp, Color(0xFF9E9E9E), RectangleShape)
                .testTag("approach_instruction_box")
                .semantics {
                    contentDescription =
                        buildString {
                            append("Next maneuver. ")
                            append(dist)
                            streetLine?.let {
                                append(". Onto ")
                                append(it)
                            }
                            houseLine?.let {
                                append(" ")
                                append(it)
                            }
                            postLine?.let {
                                append(". ")
                                append(it)
                            }
                            exitLabel?.let {
                                append(". ")
                                append(it)
                            }
                        }
                },
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            if (bmp != null) {
                Image(
                    bitmap = bmp.asImageBitmap(),
                    contentDescription = null,
                    modifier =
                        Modifier
                            .size(if (urgency) 72.dp else 56.dp)
                            .testTag("approach_maneuver_icon"),
                )
            }
            Column {
                Text(
                    text = dist,
                    style =
                        if (urgency) {
                            MaterialTheme.typography.headlineMedium
                        } else {
                            MaterialTheme.typography.headlineSmall
                        },
                    modifier = Modifier.testTag("approach_distance"),
                )
                if (streetLine != null) {
                    Text(
                        text = streetLine,
                        style =
                            if (urgency) {
                                MaterialTheme.typography.titleLarge
                            } else {
                                MaterialTheme.typography.titleMedium
                            },
                        maxLines = 1,
                        softWrap = false,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.testTag("approach_street"),
                    )
                }
                if (houseLine != null) {
                    Text(
                        text = houseLine,
                        style = MaterialTheme.typography.titleMedium,
                        maxLines = 1,
                        softWrap = false,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.testTag("approach_housenumber"),
                    )
                }
                if (postLine != null) {
                    Text(
                        text = postLine,
                        style = MaterialTheme.typography.bodyLarge,
                        maxLines = 1,
                        softWrap = false,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.testTag("approach_postcode"),
                    )
                }
                if (exitLabel != null) {
                    Text(
                        text = exitLabel,
                        style =
                            MaterialTheme.typography.bodyMedium.copy(
                                fontSize = if (urgency) 16.sp else 14.sp,
                            ),
                        modifier = Modifier.testTag("approach_roundabout_exit"),
                    )
                }
            }
        }
    }
}
