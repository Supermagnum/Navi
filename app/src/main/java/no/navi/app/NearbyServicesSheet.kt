package no.navi.app

import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import uniffi.navi.FfiNearbyServiceCategory
import uniffi.navi.FfiNearbyServiceHit

/**
 * Offline Nearby services results: category chips + distance-sorted list.
 * Tap a row to centre the map; [onRouteHere] sets the destination and plans.
 */
@Composable
fun NearbyServicesSheet(
    hits: List<FfiNearbyServiceHit>,
    selected: Set<FfiNearbyServiceCategory>,
    onToggleCategory: (FfiNearbyServiceCategory) -> Unit,
    missingRegionsMessage: String,
    noGpsMessage: String?,
    busy: Boolean,
    queryTimeMs: Long,
    onClose: () -> Unit,
    onSelect: (FfiNearbyServiceHit) -> Unit,
    onRouteHere: (FfiNearbyServiceHit) -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(
        shape = RoundedCornerShape(12.dp),
        tonalElevation = 8.dp,
        modifier =
            modifier
                .fillMaxWidth()
                .height(440.dp)
                .testTag("nearby_services_sheet"),
    ) {
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Text(
                    stringResource(R.string.nearby_services_title),
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                    modifier = Modifier.testTag("nearby_services_title"),
                )
                TextButton(onClick = onClose, modifier = Modifier.testTag("nearby_services_close")) {
                    Text(stringResource(R.string.action_close))
                }
            }
            if (!noGpsMessage.isNullOrBlank()) {
                Text(
                    noGpsMessage,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.testTag("nearby_services_no_gps"),
                )
                return@Column
            }
            Row(
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .horizontalScroll(rememberScrollState())
                        .testTag("nearby_services_chips"),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                for (cat in FfiNearbyServiceCategory.entries) {
                    FilterChip(
                        selected = selected.contains(cat),
                        onClick = { onToggleCategory(cat) },
                        label = { Text(nearbyServiceCategoryLabel(cat)) },
                        modifier = Modifier.testTag("nearby_chip_${cat.name.lowercase()}"),
                    )
                }
            }
            if (missingRegionsMessage.isNotBlank()) {
                Text(
                    missingRegionsMessage,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.tertiary,
                    modifier = Modifier.testTag("nearby_services_gap"),
                )
            }
            when {
                busy -> {
                    Text(
                        stringResource(R.string.nearby_services_searching),
                        style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.testTag("nearby_services_busy"),
                    )
                }
                hits.isEmpty() -> {
                    Text(
                        stringResource(R.string.nearby_services_empty),
                        style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.testTag("nearby_services_empty"),
                    )
                }
                else -> {
                    if (queryTimeMs > 0L) {
                        Text(
                            stringResource(R.string.nearby_services_query_ms, queryTimeMs),
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                    LazyColumn(
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .weight(1f)
                                .testTag("nearby_services_list"),
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        itemsIndexed(hits, key = { _, h -> h.osmId }) { idx, hit ->
                            NearbyServiceRow(
                                hit = hit,
                                index = idx,
                                onSelect = { onSelect(hit) },
                                onRouteHere = { onRouteHere(hit) },
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun NearbyServiceRow(
    hit: FfiNearbyServiceHit,
    index: Int,
    onSelect: () -> Unit,
    onRouteHere: () -> Unit,
) {
    Column(
        modifier =
            Modifier
                .fillMaxWidth()
                .clickable(onClick = onSelect)
                .padding(vertical = 6.dp, horizontal = 4.dp)
                .testTag("nearby_service_hit_$index"),
    ) {
        Text(hit.name, style = MaterialTheme.typography.bodyLarge)
        Text(
            "${nearbyServiceCategoryLabel(hit.category)} · ${hit.distanceLabel} · ${hit.compass}",
            style = MaterialTheme.typography.bodySmall,
        )
        TextButton(
            onClick = onRouteHere,
            modifier = Modifier.testTag("nearby_service_route_$index"),
        ) {
            Text(stringResource(R.string.nearby_services_route_here))
        }
    }
}

@Composable
fun nearbyServiceCategoryLabel(cat: FfiNearbyServiceCategory): String =
    stringResource(
        when (cat) {
            FfiNearbyServiceCategory.GAS_STATION -> R.string.nearby_cat_gas_station
            FfiNearbyServiceCategory.CAFE -> R.string.nearby_cat_cafe
            FfiNearbyServiceCategory.FAST_FOOD -> R.string.nearby_cat_fast_food
            FfiNearbyServiceCategory.HOSPITAL -> R.string.nearby_cat_hospital
            FfiNearbyServiceCategory.CAR_REPAIR -> R.string.nearby_cat_car_repair
            FfiNearbyServiceCategory.HOTEL_MOTEL -> R.string.nearby_cat_hotel_motel
            FfiNearbyServiceCategory.DOCTOR -> R.string.nearby_cat_doctor
        },
    )
