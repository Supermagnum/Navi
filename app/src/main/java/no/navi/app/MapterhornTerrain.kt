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

    private const val HILLSHADE_EXAGGERATION = 0.5f
    private const val HILLSHADE_SHADOW_COLOR = "#473B24"
    private const val HILLSHADE_HIGHLIGHT_COLOR = "#FFFFFF"
    private const val HILLSHADE_ILLUMINATION_DEG = 335f

    private fun hillshadeExaggeration(): Float = NaviMapTestHooks.hillshadeExaggerationOverride ?: HILLSHADE_EXAGGERATION

    private fun hillshadePaintJson(): JSONObject =
        JSONObject()
            .put("hillshade-exaggeration", hillshadeExaggeration().toDouble())
            .put("hillshade-shadow-color", HILLSHADE_SHADOW_COLOR)
            .put("hillshade-highlight-color", HILLSHADE_HIGHLIGHT_COLOR)
            .put("hillshade-illumination-direction", HILLSHADE_ILLUMINATION_DEG.toDouble())

    /** Local DEM beside a completed basemap PMTiles path, if present. */
    fun localDemBesideBasemap(basemapPmtilesPath: String): File? {
        val base = File(basemapPmtilesPath)
        if (!base.isFile) return null
        val dem = File(base.parentFile, base.nameWithoutExtension + "_dem.pmtiles")
        return dem.takeIf { it.isFile && it.length() > 1000L }
    }

    fun demPmtilesUri(demFile: File): String = "pmtiles://file://${demFile.absolutePath}"

    /**
     * Start loopback DEM tiles and return the local TileJSON URL (matches online Mapterhorn).
     */
    fun ensureLocalDemTileJsonUrl(demFile: File): String {
        LocalDemTileServer.ensureServing(demFile)
        return LocalDemTileServer.tileJsonUrl()
            ?: error("LocalDemTileServer not bound for ${demFile.absolutePath}")
    }

    /**
     * Serve [demFile] over loopback HTTP as Mapbox Terrain-RGB PNG tiles
     * (terrarium decoded from PMTiles, re-encoded for MapLibre `encoding: mapbox`).
     */
    fun ensureLocalDemTilesUrl(demFile: File): String = ensureLocalDemTileJsonUrl(demFile)

    /**
     * Inject DEM sources + hillshade. [demSourceUri] is the online TileJSON URL,
     * a loopback Mapbox `{z}/{x}/{y}.png` template, or (legacy) `pmtiles://`.
     */
    fun attach(
        style: Style,
        demSourceUri: String = TILEJSON_URL,
    ): Boolean =
        try {
            detach(style)
            when {
                demSourceUri.startsWith("pmtiles://") -> {
                    addLocalDemSource(style, HILLSHADE_SOURCE_ID, demSourceUri)
                }
                demSourceUri.contains("tilejson") || demSourceUri == TILEJSON_URL -> {
                    if (demSourceUri.contains("127.0.0.1")) {
                        addLocalTileJsonDemSource(style, HILLSHADE_SOURCE_ID, demSourceUri)
                    } else {
                        addOnlineDemSource(style, HILLSHADE_SOURCE_ID)
                    }
                }
                else -> {
                    // Loopback Mapbox Terrain-RGB PNG template (legacy baked styles).
                    addHttpDemSource(style, HILLSHADE_SOURCE_ID, demSourceUri)
                }
            }
            val hills =
                HillshadeLayer(HILLS_LAYER_ID, HILLSHADE_SOURCE_ID)
                    .withProperties(
                        PropertyFactory.hillshadeExaggeration(hillshadeExaggeration()),
                        PropertyFactory.hillshadeShadowColor(Color.parseColor(HILLSHADE_SHADOW_COLOR)),
                        PropertyFactory.hillshadeHighlightColor(Color.parseColor(HILLSHADE_HIGHLIGHT_COLOR)),
                        PropertyFactory.hillshadeIlluminationDirection(HILLSHADE_ILLUMINATION_DEG),
                    )
            // Insert under the first hydro fill/line so hillshade does not
            // darken water fill when 3D is on (keeps DEM shading under water).
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

    private fun addTileJsonDemSource(
        style: Style,
        id: String,
        tilejsonUri: String,
    ) {
        style.addSource(RasterDemSource(id, tilejsonUri, 512))
    }

    /** Loopback TileJSON with style-level mapbox encoding override (#3570). */
    private fun addLocalTileJsonDemSource(
        style: Style,
        id: String,
        tilejsonUri: String,
    ) {
        val template = LocalDemTileServer.activeTileTemplate()
        if (template != null) {
            addHttpDemSource(style, id, template)
            return
        }
        try {
            addTileJsonDemSource(style, id, tilejsonUri)
        } catch (e: Exception) {
            Log.w(TAG, "TileJSON local dem attach failed, retrying explicit tiles", e)
            val port =
                Regex("""127\.0\.0\.1:(\d+)""")
                    .find(tilejsonUri)
                    ?.groupValues
                    ?.get(1)
                    ?: throw e
            val fallback = "http://127.0.0.1:$port/{z}/{x}/{y}.png"
            addHttpDemSource(style, id, fallback)
        }
    }

    /** Local loopback DEM: raw terrarium WebP (same encoding as online Mapterhorn). */
    private fun addHttpDemSource(
        style: Style,
        id: String,
        tileTemplate: String,
    ) {
        val tileSet = TileSet("3.0.0", tileTemplate)
        tileSet.encoding = "terrarium"
        tileSet.attribution = ATTRIBUTION
        tileSet.maxZoom = 12f
        style.addSource(RasterDemSource(id, tileSet, 512))
    }

    fun detach(style: Style) {
        runCatching { style.removeLayer(HILLS_LAYER_ID) }
        runCatching { style.removeSource(HILLSHADE_SOURCE_ID) }
        runCatching { style.removeSource(TERRAIN_SOURCE_ID) }
    }

    fun isAttached(style: Style): Boolean =
        style.getLayer(HILLS_LAYER_ID) != null &&
            style.getSource(HILLSHADE_SOURCE_ID) != null

    /** Offline 3D: DEM hillshade baked in style JSON; loopback URI for diagnostics. */
    fun usesBakedOfflineHillshade(resolved: BasemapStyleResolver.ResolvedStyle): Boolean =
        !resolved.attachMapterhornTerrain &&
            resolved.demSourceUri?.startsWith("http://127.0.0.1") == true

    fun wantHillshadeAttached(resolved: BasemapStyleResolver.ResolvedStyle): Boolean = resolved.attachMapterhornTerrain || usesBakedOfflineHillshade(resolved)

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
                .put("tileSize", 512)
                .put("maxzoom", 12)
        if (demSourceUri.startsWith("pmtiles://")) {
            dem.put("encoding", "terrarium")
            dem.put("url", demSourceUri)
        } else if (demSourceUri.contains("127.0.0.1") && demSourceUri.contains("tilejson")) {
            dem.put("url", demSourceUri)
            dem.put("encoding", "terrarium")
            LocalDemTileServer.activeTileTemplate()?.let { template ->
                dem.put("tiles", JSONArray().put(template))
            }
        } else if (demSourceUri.contains("tilejson") || demSourceUri == TILEJSON_URL) {
            dem.put("url", demSourceUri)
        } else if (demSourceUri.contains("127.0.0.1")) {
            dem.put("encoding", "terrarium")
            dem.put("tiles", JSONArray().put(demSourceUri))
        } else {
            dem.put("encoding", "terrarium")
            dem.put("tiles", JSONArray().put(demSourceUri))
        }
        if (!sources.has(HILLSHADE_SOURCE_ID)) {
            sources.put(HILLSHADE_SOURCE_ID, dem)
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
                    .put("paint", hillshadePaintJson())
            val insertAt =
                firstHydroLayerIndex(layers).takeIf { it >= 0 }
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
    internal fun firstHydroLayerId(style: Style): String? = style.layers.firstOrNull { isHydroLayerId(it.id) }?.id

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
