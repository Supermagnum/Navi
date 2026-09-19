package no.navi.app

import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.SymbolLayer
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.geojson.Feature
import org.maplibre.geojson.FeatureCollection
import org.maplibre.geojson.Point
import uniffi.navi.namedBuildingsInBbox
import java.util.concurrent.atomic.AtomicLong

/**
 * Named OSM buildings (`building=*` + `name=*`) from the offline place index.
 *
 * Protomaps tiles do not carry `name` on the buildings layer; labels come from
 * [namedBuildingsInBbox] instead of a dead style JSON symbol layer.
 *
 * Camera-idle only captures zoom/bounds on the main thread. The UniFFI SQLite
 * open runs on [Dispatchers.IO] so a place-index write cannot ANR the looper.
 */
object NamedBuildingLabels {
    private const val TAG = "NamedBuildingLabels"
    const val SRC_ID = "named-buildings-src"
    const val LAYER_ID = "named-buildings-label"

    /** Match housenumber / building label density (Protomaps housenumber floor). */
    const val MIN_ZOOM = 15.0

    private const val MAX_LABELS = 250
    private const val DEBOUNCE_MS = 350L
    private const val TEXT_COLOR = "#4a4a4a"
    private const val HALO_COLOR = "#f8f4f0"

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var debounceJob: Job? = null
    private val fetchGen = AtomicLong(0)

    fun clear(style: Style) {
        runCatching {
            style.getLayer(LAYER_ID)?.let { style.removeLayer(it) }
            style.getSource(SRC_ID)?.let { style.removeSource(it) }
        }
    }

    /** Schedule a debounced refresh after camera idle / style apply. */
    fun scheduleRefresh(
        map: MapLibreMap,
        placeIndexDb: String,
    ) {
        debounceJob?.cancel()
        debounceJob =
            scope.launch {
                delay(DEBOUNCE_MS)
                refreshNow(map, placeIndexDb)
            }
    }

    fun refreshNow(
        map: MapLibreMap,
        placeIndexDb: String,
    ) {
        val style = map.style ?: return
        if (placeIndexDb.isBlank()) {
            clear(style)
            return
        }
        val zoom = map.cameraPosition.zoom
        if (zoom < MIN_ZOOM) {
            applyFeatures(style, emptyList())
            return
        }
        val bounds =
            try {
                map.projection.visibleRegion.latLngBounds
            } catch (_: Exception) {
                applyFeatures(style, emptyList())
                return
            }
        val minLat = bounds.latitudeSouth
        val minLon = bounds.longitudeWest
        val maxLat = bounds.latitudeNorth
        val maxLon = bounds.longitudeEast
        val gen = fetchGen.incrementAndGet()
        scope.launch {
            val hits =
                withContext(Dispatchers.IO) {
                    runCatching {
                        namedBuildingsInBbox(
                            indexDbPath = placeIndexDb,
                            minLat = minLat,
                            minLon = minLon,
                            maxLat = maxLat,
                            maxLon = maxLon,
                            limit = MAX_LABELS.toUInt(),
                        )
                    }.getOrElse {
                        Log.w(TAG, "namedBuildingsInBbox failed: ${it.message}")
                        emptyList()
                    }
                }
            if (gen != fetchGen.get()) return@launch
            val liveStyle = map.style ?: return@launch
            val features =
                hits.map { hit ->
                    Feature.fromGeometry(Point.fromLngLat(hit.lon, hit.lat)).also { f ->
                        f.addStringProperty("name", hit.name)
                        f.addNumberProperty("osm_id", hit.osmId.toDouble())
                    }
                }
            applyFeatures(liveStyle, features)
            Log.i(TAG, "labels=${features.size} zoom=$zoom")
        }
    }

    private fun applyFeatures(
        style: Style,
        features: List<Feature>,
    ) {
        val collection = FeatureCollection.fromFeatures(features)
        val existing = style.getSource(SRC_ID) as? GeoJsonSource
        if (existing != null) {
            existing.setGeoJson(collection)
        } else {
            style.addSource(GeoJsonSource(SRC_ID, collection))
        }
        if (style.getLayer(LAYER_ID) == null) {
            val labels =
                SymbolLayer(LAYER_ID, SRC_ID).apply {
                    setMinZoom(MIN_ZOOM.toFloat())
                    setProperties(
                        PropertyFactory.textField(Expression.get("name")),
                        PropertyFactory.textFont(arrayOf("Noto Sans Regular")),
                        PropertyFactory.textSize(
                            Expression.interpolate(
                                Expression.linear(),
                                Expression.zoom(),
                                Expression.stop(15, 10f),
                                Expression.stop(18, 13f),
                            ),
                        ),
                        PropertyFactory.textColor(TEXT_COLOR),
                        PropertyFactory.textHaloColor(HALO_COLOR),
                        PropertyFactory.textHaloWidth(1.2f),
                        PropertyFactory.textOptional(true),
                        PropertyFactory.textAllowOverlap(false),
                        PropertyFactory.textIgnorePlacement(false),
                        PropertyFactory.textPadding(2f),
                        PropertyFactory.symbolPlacement(Property.SYMBOL_PLACEMENT_POINT),
                        PropertyFactory.textAnchor(Property.TEXT_ANCHOR_CENTER),
                    )
                }
            BasemapLayerOrder.addSymbolLayer(style, labels)
        }
    }
}
