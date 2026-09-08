package no.navi.app

import android.graphics.Color
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import org.maplibre.android.maps.Style
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.geojson.Feature
import org.maplibre.geojson.FeatureCollection
import org.maplibre.geojson.Point

private const val TAG = "NaviDatex"
private const val SRC_ID = "datex-situations-src"
private const val LAYER_ID = "datex-situations-layer"

data class DatexHudState(
    val overlayEnabled: Boolean = false,
    val activeCount: Int = 0,
    val inactiveCount: Int = 0,
    val warning: String? = null,
    val attribution: String? = null,
    /** `server-lan` / `server-duckdns` / `none` — same tags as pack acquisition. */
    val dataSource: String = "none",
    val activeJson: String = "[]",
    val inactiveJson: String = "[]",
    val rawJson: String = "{}",
)

fun datexHudFromRefreshJson(raw: String): DatexHudState =
    runCatching {
        val o = JSONObject(raw)
        DatexHudState(
            overlayEnabled = o.optBoolean("overlay_enabled", false),
            activeCount = o.optInt("active_count", o.optJSONArray("active")?.length() ?: 0),
            inactiveCount = o.optInt("inactive_count", o.optJSONArray("inactive")?.length() ?: 0),
            warning = o.optString("warning").takeIf { it.isNotBlank() && it != "null" },
            attribution = o.optString("attribution").takeIf { it.isNotBlank() && it != "null" },
            dataSource =
                o.optString("data_source").takeIf { it.isNotBlank() && it != "null" } ?: "none",
            activeJson = o.optJSONArray("active")?.toString() ?: "[]",
            inactiveJson = o.optJSONArray("inactive")?.toString() ?: "[]",
            rawJson = raw,
        )
    }.getOrElse {
        DatexHudState(warning = "parse_failed", rawJson = raw)
    }

fun routeSamplesToLatLonJson(samples: List<RouteSimSample>): String {
    val arr = JSONArray()
    for (s in samples) {
        arr.put(JSONArray().put(s.lat).put(s.lon))
    }
    return arr.toString()
}

/** Paint active DATEX situations as map circles; clear when overlay is off. */
fun applyDatexOverlay(
    style: Style,
    hud: DatexHudState,
) {
    if (!hud.overlayEnabled || hud.activeCount == 0) {
        clearDatexOverlay(style)
        return
    }
    val features = ArrayList<Feature>()
    val arr = runCatching { JSONArray(hud.activeJson) }.getOrNull() ?: JSONArray()
    for (i in 0 until arr.length()) {
        val o = arr.optJSONObject(i) ?: continue
        val lat = o.optDouble("lat", Double.NaN)
        val lon = o.optDouble("lon", Double.NaN)
        if (!lat.isFinite() || !lon.isFinite()) continue
        val f = Feature.fromGeometry(Point.fromLngLat(lon, lat))
        f.addStringProperty("id", o.optString("id"))
        f.addStringProperty("kind", o.optString("kind"))
        f.addStringProperty(
            "label",
            o.optString("road_number").ifBlank { o.optString("kind") },
        )
        features.add(f)
    }
    val fc = FeatureCollection.fromFeatures(features)
    val existing = style.getSource(SRC_ID) as? GeoJsonSource
    if (existing != null) {
        existing.setGeoJson(fc)
    } else {
        style.addSource(GeoJsonSource(SRC_ID, fc))
    }
    if (style.getLayer(LAYER_ID) == null) {
        val layer =
            CircleLayer(LAYER_ID, SRC_ID).withProperties(
                PropertyFactory.circleRadius(8f),
                PropertyFactory.circleColor(Color.parseColor("#C62828")),
                PropertyFactory.circleStrokeWidth(2f),
                PropertyFactory.circleStrokeColor(Color.WHITE),
                PropertyFactory.circleOpacity(0.85f),
            )
        runCatching { style.addLayer(layer) }
            .onFailure { Log.w(TAG, "addLayer failed: ${it.message}") }
    }
    Log.i(TAG, "overlay active=${features.size}")
}

fun clearDatexOverlay(style: Style) {
    runCatching { style.removeLayer(LAYER_ID) }
    runCatching { style.removeSource(SRC_ID) }
}
