package no.navi.app

import android.graphics.Color
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import org.maplibre.android.maps.Style
import org.maplibre.android.style.layers.HillshadeLayer
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.SymbolLayer
import org.maplibre.android.style.sources.RasterDemSource
import org.maplibre.android.style.sources.TileSet
import java.io.File

/**
 * Mapterhorn DEM hillshade for opt-in “3D” basemap mode.
 *
 * Online: TileJSON at [TILEJSON_URL].
 * Offline: local PMTiles extract next to the Protomaps basemap
 * (`{region_key}_dem.pmtiles`), produced via range-fetch from
 * `https://download.mapterhorn.com/planet.pmtiles` (z≤12).
 *
 * MapLibre Native does **not** support style `terrain` / `sky` mesh
 * ([#252](https://github.com/maplibre/maplibre-native/issues/252)); hillshade only.
 */
object MapterhornTerrain {
    const val TILEJSON_URL = "https://tiles.mapterhorn.com/tilejson.json"
    const val MAPTERHORN_PLANET_URL = "https://download.mapterhorn.com/planet.pmtiles"
    const val TERRAIN_SOURCE_ID = "terrainSource"
    const val HILLSHADE_SOURCE_ID = "hillshadeSource"
    const val HILLS_LAYER_ID = "navi-hills"
    const val DEM_FILE_SUFFIX = "_dem.pmtiles"

    const val ATTRIBUTION =
        "<a href=\"https://mapterhorn.com/attribution\">© Mapterhorn</a>"

    const val VIEW_TILT_DEG = 50.0

    private const val TAG = "MapterhornTerrain"

    /** Local DEM beside a completed basemap PMTiles path, if present. */
    fun localDemBesideBasemap(basemapPmtilesPath: String): File? {
        val base = File(basemapPmtilesPath)
        if (!base.isFile) return null
        val dem = File(base.parentFile, base.nameWithoutExtension + "_dem.pmtiles")
        return dem.takeIf { it.isFile && it.length() > 1000L }
    }

    fun demPmtilesUri(demFile: File): String = "pmtiles://file://${demFile.absolutePath}"

    /**
     * Inject DEM sources + hillshade. [demSourceUri] is either the online TileJSON
     * URL or a local `pmtiles://file://…` URI. Idempotent.
     */
    fun attach(
        style: Style,
        demSourceUri: String = TILEJSON_URL,
    ): Boolean =
        try {
            detach(style)
            val useLocal = demSourceUri.startsWith("pmtiles://")
            if (useLocal) {
                addLocalDemSource(style, HILLSHADE_SOURCE_ID, demSourceUri)
                addLocalDemSource(style, TERRAIN_SOURCE_ID, demSourceUri)
            } else {
                // Explicit terrarium TileSet — TileJSON URL alone often fails to
                // configure encoding/tileSize on MapLibre Native after a live attach.
                addOnlineDemSource(style, HILLSHADE_SOURCE_ID)
                addOnlineDemSource(style, TERRAIN_SOURCE_ID)
            }
            val hills =
                HillshadeLayer(HILLS_LAYER_ID, HILLSHADE_SOURCE_ID)
                    .withProperties(
                        PropertyFactory.hillshadeShadowColor(Color.parseColor("#473B24")),
                    )
            // Insert under the first hydro fill/line so hillshade does not
            // composite on top of soft water edges (worsens shoreline bleed).
            // Fall back to under labels if the style has no water layers yet.
            val belowId =
                firstHydroLayerId(style) ?: style.layers.firstOrNull { it is SymbolLayer }?.id
            if (belowId != null) {
                style.addLayerBelow(hills, belowId)
            } else {
                style.addLayer(hills)
            }
            true
        } catch (e: Exception) {
            Log.e(TAG, "Failed to attach Mapterhorn hillshade ($demSourceUri)", e)
            runCatching { detach(style) }
            false
        }

    private fun addOnlineDemSource(
        style: Style,
        id: String,
    ) {
        try {
            val tileSet = TileSet("3.0.0", "https://tiles.mapterhorn.com/{z}/{x}/{y}.webp")
            tileSet.encoding = "terrarium"
            tileSet.attribution = ATTRIBUTION
            style.addSource(RasterDemSource(id, tileSet, 512))
        } catch (e: Exception) {
            Log.w(TAG, "TileSet online dem attach failed, retrying TileJSON url", e)
            style.addSource(RasterDemSource(id, TILEJSON_URL))
        }
    }

    private fun addLocalDemSource(
        style: Style,
        id: String,
        pmtilesUri: String,
    ) {
        try {
            val tileSet = TileSet("3.0.0", pmtilesUri)
            tileSet.encoding = "terrarium"
            tileSet.attribution = ATTRIBUTION
            style.addSource(RasterDemSource(id, tileSet, 512))
        } catch (e: Exception) {
            Log.w(TAG, "TileSet dem attach failed, retrying url form", e)
            style.addSource(RasterDemSource(id, pmtilesUri))
        }
    }

    fun detach(style: Style) {
        runCatching { style.removeLayer(HILLS_LAYER_ID) }
        runCatching { style.removeSource(HILLSHADE_SOURCE_ID) }
        runCatching { style.removeSource(TERRAIN_SOURCE_ID) }
    }

    fun isAttached(style: Style): Boolean =
        style.getLayer(HILLS_LAYER_ID) != null &&
            style.getSource(HILLSHADE_SOURCE_ID) != null

    fun augmentStyleJson(
        style: JSONObject,
        demSourceUri: String = TILEJSON_URL,
    ): JSONObject {
        val sources = style.getJSONObject("sources")
        // Prefer `tiles` + `encoding` over `url`. MapLibre Native historically
        // ignored style-level encoding when only a TileJSON/PMTiles `url` was set
        // (maplibre-native#3564); explicit tiles keep terrarium decoding correct.
        val dem =
            JSONObject()
                .put("type", "raster-dem")
                .put("attribution", ATTRIBUTION)
                .put("encoding", "terrarium")
                .put("tileSize", 512)
        if (demSourceUri.startsWith("pmtiles://")) {
            dem.put("tiles", JSONArray().put(demSourceUri))
        } else if (demSourceUri.contains("tilejson")) {
            dem.put("tiles", JSONArray().put("https://tiles.mapterhorn.com/{z}/{x}/{y}.webp"))
        } else {
            dem.put("tiles", JSONArray().put(demSourceUri))
        }
        if (!sources.has(TERRAIN_SOURCE_ID)) {
            sources.put(TERRAIN_SOURCE_ID, JSONObject(dem.toString()))
        }
        if (!sources.has(HILLSHADE_SOURCE_ID)) {
            sources.put(HILLSHADE_SOURCE_ID, JSONObject(dem.toString()))
        }

        val layers = style.getJSONArray("layers")
        var hasHills = false
        for (i in 0 until layers.length()) {
            if (layers.getJSONObject(i).optString("id") == HILLS_LAYER_ID) {
                hasHills = true
                break
            }
        }
        if (!hasHills) {
            val hills =
                JSONObject()
                    .put("id", HILLS_LAYER_ID)
                    .put("type", "hillshade")
                    .put("source", HILLSHADE_SOURCE_ID)
                    .put(
                        "paint",
                        JSONObject().put("hillshade-shadow-color", "#473B24"),
                    )
            val insertAt = firstHydroLayerIndex(layers).takeIf { it >= 0 }
                ?: firstSymbolLayerIndex(layers)
            val rewritten = JSONArray()
            for (i in 0 until layers.length()) {
                if (i == insertAt) rewritten.put(hills)
                rewritten.put(layers.getJSONObject(i))
            }
            if (insertAt >= layers.length()) rewritten.put(hills)
            style.put("layers", rewritten)
        }
        return style
    }

    /** First water fill / waterway line id, if present. */
    internal fun firstHydroLayerId(style: Style): String? =
        style.layers.firstOrNull { isHydroLayerId(it.id) }?.id

    internal fun isHydroLayerId(id: String): Boolean {
        val lower = id.lowercase()
        return lower == "water" ||
            lower.startsWith("water_") ||
            lower.startsWith("waterway") ||
            lower.contains("waterway")
    }

    private fun firstHydroLayerIndex(layers: JSONArray): Int {
        for (i in 0 until layers.length()) {
            if (isHydroLayerId(layers.getJSONObject(i).optString("id"))) return i
        }
        return -1
    }

    private fun firstSymbolLayerIndex(layers: JSONArray): Int {
        for (i in 0 until layers.length()) {
            if (layers.getJSONObject(i).optString("type") == "symbol") return i
        }
        return layers.length()
    }
}
