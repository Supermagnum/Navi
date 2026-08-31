package no.navi.app

import android.util.Log
import org.maplibre.android.geometry.LatLngBounds
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.LineLayer
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.sources.CustomGeometrySource
import org.maplibre.android.style.sources.GeometryTileProvider
import org.maplibre.geojson.FeatureCollection

/**
 * Elevation contour lines derived from Mapterhorn terrarium DEM tiles — the
 * same elevation source as [MapterhornTerrain] hillshade.
 *
 * MapLibre Native has no built-in contour source (see maplibre-native#4283), so
 * contours are generated on-device via [CustomGeometrySource] +
 * [ContourGenerator] marching squares.
 *
 * Performance: marching squares runs when MapLibre requests a new geometry
 * tile (pan/zoom), not every frame. [ContourGenerationCache] and
 * [DemTileFetcher]'s DEM grid cache avoid recomputing when revisiting tiles or
 * when adjacent viewport tiles share a parent DEM cell.
 *
 * Layer stack: above hillshade / land fills, below roads and all symbol labels
 * ([BasemapLayerOrder.addFillOrLineBelowAnchors]).
 */
object MapterhornContours {
    const val SOURCE_ID = "navi-contour-source"
    const val LAYER_MINOR_ID = "navi-contours-minor"
    const val LAYER_MAJOR_ID = "navi-contours-major"

    /** Warm brown tones legible over hillshade shadow/highlight. */
    private const val MINOR_COLOR = "#7A5C44"
    private const val MAJOR_COLOR = "#3D2914"

    private const val TAG = "MapterhornContours"
    private const val MIN_ZOOM = 9f

    /** Match [MapHudPrefs.MAX_ZOOM] so contours stay visible at close-in street zoom. */
    const val MAX_ZOOM = 20f

    private val BELOW_ROAD_ANCHORS =
        listOf(
            "tunnel_motorway_link_casing",
            "tunnel_service_track_casing",
            "road_motorway_casing",
            "highway_motorway_casing",
            "road_path",
            "bridge_motorway_link_casing",
        )

    @Volatile
    private var activeFetcher: DemTileFetcher? = null

    private class ContourTileProvider(
        private val fetcher: DemTileFetcher,
    ) : GeometryTileProvider {
        override fun getFeaturesForBounds(
            bounds: LatLngBounds,
            zoomLevel: Int,
        ): FeatureCollection {
            if (zoomLevel < 9) return FeatureCollection.fromFeatures(emptyArray())
            val west = bounds.longitudeWest
            val south = bounds.latitudeSouth
            val east = bounds.longitudeEast
            val north = bounds.latitudeNorth
            val tileBounds = DemTileLatLngBounds(west, south, east, north)
            val demZ = minOf(zoomLevel, 12)
            val allFeatures = ArrayList<org.maplibre.geojson.Feature>()
            for ((tx, ty) in DemTileCover.tilesIntersecting(tileBounds, demZ)) {
                val cacheKey = ContourGenerationCache.key(demZ, tx, ty, zoomLevel)
                val cached = ContourGenerationCache.get(cacheKey)
                val tileFeatures =
                    if (cached != null) {
                        cached
                    } else {
                        NaviMapTestHooks.contourGenCacheMiss++
                        val demTile = fetcher.fetchTile(demZ, tx, ty)
                        if (demTile == null) {
                            continue
                        }
                        val sampleDim = DemTileFetcher.sampleDimForZoom(zoomLevel)
                        val demBounds =
                            DemTileLatLngBounds(
                                demTile.west,
                                demTile.south,
                                demTile.east,
                                demTile.north,
                            )
                        val grid =
                            DemElevationGridDecoder.resample(
                                demTile,
                                demBounds,
                                sampleDim,
                            )
                        val generated = ContourGenerator.generateFeatures(grid, zoomLevel)
                        ContourGenerationCache.put(cacheKey, generated)
                        generated
                    }
                allFeatures.addAll(
                    ContourFeatureClip.clipToBounds(tileFeatures, west, south, east, north),
                )
            }
            return FeatureCollection.fromFeatures(allFeatures)
        }
    }

    fun attach(
        style: Style,
        fetcher: DemTileFetcher,
        unitSystem: UnitSystem = UnitSystem.METRIC,
    ): Boolean =
        try {
            detach(style, clearCaches = false)
            activeFetcher = fetcher
            val source = CustomGeometrySource(SOURCE_ID, ContourTileProvider(fetcher))
            style.addSource(source)
            val minorFilter = Expression.eq(Expression.get(ContourGenerator.PROP_MAJOR), false)
            val majorFilter = Expression.eq(Expression.get(ContourGenerator.PROP_MAJOR), true)
            val minor =
                LineLayer(LAYER_MINOR_ID, SOURCE_ID).apply {
                    setMinZoom(MIN_ZOOM)
                    setMaxZoom(MAX_ZOOM)
                    setFilter(minorFilter)
                    setProperties(
                        PropertyFactory.lineColor(MINOR_COLOR),
                        PropertyFactory.lineWidth(1.0f),
                        PropertyFactory.lineOpacity(0.62f),
                    )
                }
            val major =
                LineLayer(LAYER_MAJOR_ID, SOURCE_ID).apply {
                    setMinZoom(MIN_ZOOM)
                    setMaxZoom(MAX_ZOOM)
                    setFilter(majorFilter)
                    setProperties(
                        PropertyFactory.lineColor(MAJOR_COLOR),
                        PropertyFactory.lineWidth(1.6f),
                        PropertyFactory.lineOpacity(0.82f),
                    )
                }
            BasemapLayerOrder.addFillOrLineBelowAnchors(style, minor, BELOW_ROAD_ANCHORS)
            BasemapLayerOrder.addFillOrLineBelowAnchors(style, major, BELOW_ROAD_ANCHORS)
            BasemapContourLabelStyle.attach(style, unitSystem)
            Log.i(TAG, "attached contour layers")
            true
        } catch (e: Exception) {
            Log.e(TAG, "Failed to attach contours", e)
            runCatching { detach(style) }
            false
        }

    fun detach(
        style: Style,
        clearCaches: Boolean = true,
    ) {
        activeFetcher = null
        BasemapContourLabelStyle.detach(style)
        runCatching { style.removeLayer(LAYER_MAJOR_ID) }
        runCatching { style.removeLayer(LAYER_MINOR_ID) }
        runCatching { style.removeSource(SOURCE_ID) }
        if (clearCaches) {
            ContourGenerationCache.clear()
            DemTileFetcher.clearDemGridCache()
        }
    }

    fun isAttached(style: Style): Boolean = style.getLayer(LAYER_MINOR_ID) != null && style.getSource(SOURCE_ID) != null
}
