package no.navi.app

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import uniffi.navi.TravelProfile

@Composable
fun CampingSessionDisableBanner(
    message: String,
    onReEnable: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(
        color = MaterialTheme.colorScheme.errorContainer,
        modifier = modifier.fillMaxWidth().testTag("camping_session_disable_banner"),
    ) {
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                "Camping plugin disabled for this session",
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.SemiBold,
            )
            Text(message, style = MaterialTheme.typography.bodySmall)
            Button(onClick = onReEnable, modifier = Modifier.testTag("camping_session_reenable")) {
                Text("Re-enable camping")
            }
        }
    }
}

@Composable
fun CampingSuggestionSheet(
    result: CampingSuggestResult,
    profile: TravelProfile,
    listDisclaimer: String,
    sessionDisableMessage: String?,
    onReEnableSession: () -> Unit,
    onClose: () -> Unit,
    onCampHereTonight: (CampingCardModel) -> Unit = {},
    onUndoCampHereTonight: (CampingCardModel) -> Unit = {},
    modifier: Modifier = Modifier,
) {
    val motorised = isMotorisedTravelProfile(profile)
    Surface(
        shape = RoundedCornerShape(12.dp),
        tonalElevation = 8.dp,
        modifier =
            modifier
                .fillMaxWidth()
                .height(420.dp)
                .testTag("camping_suggestion_sheet"),
    ) {
        LazyColumn(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item(key = "header") {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Text("Overnight camping suggestions", style = MaterialTheme.typography.titleSmall)
                    TextButton(onClick = onClose, modifier = Modifier.testTag("camping_sheet_close")) {
                        Text("Close")
                    }
                }
            }
            if (!sessionDisableMessage.isNullOrBlank()) {
                item(key = "session_banner") {
                    CampingSessionDisableBanner(
                        message = sessionDisableMessage,
                        onReEnable = onReEnableSession,
                    )
                }
            }
            item(key = "top_disclaimer") {
                CampingDisclaimerBlock(listDisclaimer.ifBlank { result.disclaimer })
            }
                if (motorised) {
                    item(key = "hdr_vehicle") {
                        Text(
                            "Vehicle overnight",
                            style = MaterialTheme.typography.titleSmall,
                            modifier = Modifier.testTag("camping_section_vehicle"),
                        )
                    }
                    if (result.vehicle.cards.isEmpty()) {
                        item(key = "vehicle_empty") {
                            Text(
                                "No vehicle overnight spots along this corridor.",
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                    } else {
                        itemsIndexed(result.vehicle.cards, key = { idx, c -> "veh-$idx-${c.locationId}" }) { idx, card ->
                            CampingCardBlock(
                                card,
                                sectionTag = "vehicle",
                                index = idx,
                                onCampHereTonight = onCampHereTonight,
                                onUndoCampHereTonight = onUndoCampHereTonight,
                            )
                        }
                    }
                    item(key = "hdr_on_foot") {
                        Text(
                            "On foot from here",
                            style = MaterialTheme.typography.titleSmall,
                            modifier =
                                Modifier
                                    .padding(top = 4.dp)
                                    .testTag("camping_section_on_foot"),
                        )
                    }
                    if (result.onFootFromHere.cards.isEmpty()) {
                        item(key = "on_foot_empty") {
                            Text(
                                "No walk-in tent spots from the corridor.",
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                    } else {
                        itemsIndexed(
                            result.onFootFromHere.cards,
                            key = { idx, c -> "foot-$idx-${c.locationId}" },
                        ) { idx, card ->
                            CampingCardBlock(
                                card,
                                sectionTag = "on_foot",
                                index = idx,
                                onCampHereTonight = onCampHereTonight,
                                onUndoCampHereTonight = onUndoCampHereTonight,
                            )
                        }
                    }
                } else {
                    if (result.list.cards.isEmpty()) {
                        item(key = "list_empty") {
                            Text(
                                "No tent spots found along this corridor.",
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                    } else {
                        itemsIndexed(result.list.cards, key = { idx, c -> "list-$idx-${c.locationId}" }) { idx, card ->
                            CampingCardBlock(
                                card,
                                sectionTag = "list",
                                index = idx,
                                onCampHereTonight = onCampHereTonight,
                                onUndoCampHereTonight = onUndoCampHereTonight,
                            )
                        }
                    }
                }
            item(key = "bottom_disclaimer") {
                CampingDisclaimerBlock(result.disclaimer)
            }
        }
    }
}

@Composable
private fun CampingDisclaimerBlock(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.testTag("camping_disclaimer"),
    )
}

@Composable
private fun CampingCardBlock(
    card: CampingCardModel,
    sectionTag: String,
    index: Int,
    onCampHereTonight: (CampingCardModel) -> Unit,
    onUndoCampHereTonight: (CampingCardModel) -> Unit,
) {
    val uriHandler = LocalUriHandler.current
    val svalbard = card.decline == CampingDeclineKind.SVALBARD
    val containerColor =
        if (svalbard) {
            MaterialTheme.colorScheme.errorContainer
        } else if (!card.accepted) {
            MaterialTheme.colorScheme.surfaceVariant
        } else {
            MaterialTheme.colorScheme.surface
        }
    Surface(
        color = containerColor,
        shape = RoundedCornerShape(8.dp),
        modifier =
            Modifier
                .fillMaxWidth()
                .testTag("camping_card_${sectionTag}_$index"),
    ) {
        Column(
            modifier = Modifier.padding(10.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                buildString {
                    append(String.format("%.5f, %.5f", card.lat, card.lon))
                    card.walkM?.let { append(" · walk ${it.toInt()} m") }
                    card.seedRoadHighway?.let { append(" · access $it") }
                },
                style = MaterialTheme.typography.labelMedium,
                fontWeight = FontWeight.Medium,
            )
            Text(
                "Tier ${card.tier.name} · ${card.countryIso.uppercase()}${card.subdivisionIso?.let { " · $it" } ?: ""}",
                style = MaterialTheme.typography.bodySmall,
            )
            if (card.legalBasis.isNotBlank()) {
                Text(card.legalBasis, style = MaterialTheme.typography.bodySmall)
            }
            card.fireText?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.bodySmall,
                    fontWeight = FontWeight.SemiBold,
                    modifier = Modifier.testTag("camping_fire_text"),
                )
            }
            card.bareRockNote?.let {
                Text(it, style = MaterialTheme.typography.bodySmall)
            }
            if (card.notChecked.protectedArea || card.notChecked.landcover) {
                Text(
                    "Not checked",
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = FontWeight.Bold,
                    color = MaterialTheme.colorScheme.error,
                )
                if (card.notChecked.protectedArea) {
                    Text(
                        "protected_area — national parks and reserves may have separate rules",
                        style = MaterialTheme.typography.bodySmall,
                        fontWeight = FontWeight.Medium,
                    )
                }
                if (card.notChecked.landcover) {
                    Text(
                        "landcover — farmland, pasture, and cultivated land may be off limits",
                        style = MaterialTheme.typography.bodySmall,
                        fontWeight = FontWeight.Medium,
                    )
                }
            }
            for (note in card.notes) {
                val nightHighlight = note.contains("night", ignoreCase = true)
                Text(
                    note,
                    style = MaterialTheme.typography.bodySmall,
                    fontWeight = if (nightHighlight) FontWeight.SemiBold else FontWeight.Normal,
                )
            }
            card.rejectReason?.let {
                Text("Reject: $it", style = MaterialTheme.typography.bodySmall)
            }
            if (card.sources.isNotEmpty()) {
                Text("Sources", style = MaterialTheme.typography.labelSmall)
                for (url in card.sources) {
                    Text(
                        url,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.primary,
                        modifier =
                            Modifier
                                .testTag("camping_source_link")
                                .clickable { runCatching { uriHandler.openUri(url) } },
                    )
                }
            }
            if (card.disclaimer.isNotBlank() && card.disclaimer != CAMPING_PLUGIN_DISCLAIMER) {
                Text(card.disclaimer, style = MaterialTheme.typography.bodySmall)
            }
            if (card.accepted) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Button(
                        onClick = { onCampHereTonight(card) },
                        modifier = Modifier.testTag("camping_camp_here_${sectionTag}_$index"),
                    ) {
                        Text("Camp here tonight")
                    }
                    TextButton(
                        onClick = { onUndoCampHereTonight(card) },
                        modifier = Modifier.testTag("camping_undo_camp_${sectionTag}_$index"),
                    ) {
                        Text("Not camping here")
                    }
                }
            }
        }
    }
}
