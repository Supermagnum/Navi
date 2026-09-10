package no.navi.app

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import org.json.JSONObject
import uniffi.navi.FfiIconTheme
import uniffi.navi.rasterizeIconPng

data class PoiLookaheadHitUi(
    val osmId: Long,
    val label: String,
    val iconKey: String,
    val distanceM: Double,
    val openNow: String,
)

data class PoiLookaheadHudState(
    val active: Boolean = false,
    val hits: List<PoiLookaheadHitUi> = emptyList(),
)

fun poiLookaheadHudFromQueryJson(
    raw: String,
    dismissedIds: Set<String> = emptySet(),
): PoiLookaheadHudState {
    if (raw.isBlank() || raw == "{}") return PoiLookaheadHudState()
    return runCatching {
        val o = JSONObject(raw)
        val arr = o.optJSONArray("hits") ?: return PoiLookaheadHudState()
        val hits = mutableListOf<PoiLookaheadHitUi>()
        for (i in 0 until arr.length()) {
            val h = arr.getJSONObject(i)
            val osmId = h.optLong("osm_id", 0L)
            if (dismissedIds.contains(osmId.toString())) continue
            // Host must already drop closed; belt-and-suspenders.
            if (h.optString("open_now", "") == "false") continue
            hits.add(
                PoiLookaheadHitUi(
                    osmId = osmId,
                    label = h.optString("label", ""),
                    iconKey = h.optString("icon_key", ""),
                    distanceM = h.optDouble("distance_m", Double.POSITIVE_INFINITY),
                    openNow = h.optString("open_now", "unknown"),
                ),
            )
        }
        if (hits.isEmpty()) PoiLookaheadHudState() else PoiLookaheadHudState(active = true, hits = hits)
    }.getOrDefault(PoiLookaheadHudState())
}

private val PoiLookaheadHudFill = Color(0xE8E3F2FD)
private val PoiLookaheadHudBorder = Color(0xFF90CAF9)

/**
 * Quiet discovery chip for Nearby attractions — not urgency/hazard chrome.
 */
@Composable
fun PoiLookaheadHudChip(
    state: PoiLookaheadHudState,
    iconsDir: String,
    onDismissNearest: ((Long) -> Unit)? = null,
    modifier: Modifier = Modifier,
) {
    if (!state.active || state.hits.isEmpty()) return
    val nearest = state.hits.first()
    val png =
        remember(iconsDir, nearest.iconKey) {
            if (nearest.iconKey.isBlank()) {
                ByteArray(0)
            } else {
                runCatching {
                    rasterizeIconPng(
                        key = nearest.iconKey,
                        theme = FfiIconTheme.DAY,
                        width = 64u,
                        height = 64u,
                        bundledDir = iconsDir,
                    )
                }.getOrDefault(ByteArray(0))
            }
        }
    val bmp =
        remember(png) {
            if (png.isEmpty()) null else BitmapFactory.decodeByteArray(png, 0, png.size)
        }
    val more =
        if (state.hits.size > 1) {
            " +${state.hits.size - 1}"
        } else {
            ""
        }
    Row(
        modifier =
            modifier
                .background(PoiLookaheadHudFill, RoundedCornerShape(8.dp))
                .border(1.dp, PoiLookaheadHudBorder, RoundedCornerShape(8.dp))
                .padding(horizontal = 10.dp, vertical = 6.dp)
                .testTag("poi_lookahead_hud_chip")
                .semantics { contentDescription = nearest.label },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (bmp != null) {
            Image(
                bitmap = bmp.asImageBitmap(),
                contentDescription = null,
                modifier = Modifier.size(32.dp),
            )
        }
        Column(modifier = Modifier.weight(1f, fill = false)) {
            Text(
                nearest.label + more,
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.testTag("poi_lookahead_label"),
            )
        }
        if (onDismissNearest != null) {
            TextButton(
                onClick = { onDismissNearest(nearest.osmId) },
                modifier = Modifier.testTag("poi_lookahead_dismiss"),
            ) {
                Text("Dismiss")
            }
        }
    }
}
