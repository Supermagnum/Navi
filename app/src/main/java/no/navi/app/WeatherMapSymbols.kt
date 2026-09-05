package no.navi.app

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.SymbolLayer
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.geojson.Feature
import org.maplibre.geojson.FeatureCollection
import org.maplibre.geojson.Point
import uniffi.navi.rasterizeWeatherIconPng
import uniffi.navi.weatherMapMaxSymbols
import uniffi.navi.weatherMapMinPixelSpacing
import uniffi.navi.weatherMapSymbolsJson
import uniffi.navi.weatherMapZoomMax

private const val TAG = "NaviWeatherMap"
private const val SRC_ID = "weather-cities-src"
private const val LAYER_ID = "weather-cities-layer"
private const val HALO_LAYER_ID = "weather-cities-halo"

data class WeatherMapTickResult(
    val visibleCities: Int = 0,
    val kept: Int = 0,
    val fetches: Int = 0,
    val symbolCount: Int = 0,
    val zoom: Double = 0.0,
    val rawJson: String = "{}",
)

/**
 * Refresh city weather map symbols for the current camera.
 * No-op (clears layer) when plugin or map-symbols toggle is off, or zoom too high.
 */
fun refreshWeatherMapSymbols(
    map: MapLibreMap,
    mapStyle: Style,
    dataDir: String,
    placeIndexDb: String,
    weatherIconsDir: String,
    weatherPluginEnabled: Boolean,
    mapSymbolsEnabled: Boolean,
    appActive: Boolean,
): WeatherMapTickResult {
    if (!weatherPluginEnabled || !mapSymbolsEnabled) {
        clearWeatherMapSymbols(mapStyle)
        return WeatherMapTickResult(rawJson = """{"reasons":["map_weather_disabled"]}""")
    }
    val cam = map.cameraPosition
    val zoom = cam.zoom
    val zoomMax = weatherMapZoomMax()
    if (zoom > zoomMax) {
        clearWeatherMapSymbols(mapStyle)
        Log.i(TAG, "hide symbols zoom=$zoom > max=$zoomMax")
        return WeatherMapTickResult(zoom = zoom, rawJson = """{"reasons":["zoom_above_max"]}""")
    }
    val bounds =
        try {
            map.projection.visibleRegion.latLngBounds
        } catch (_: Exception) {
            clearWeatherMapSymbols(mapStyle)
            return WeatherMapTickResult(zoom = zoom, rawJson = """{"reasons":["no_bounds"]}""")
        }
    val raw =
        runCatching {
            weatherMapSymbolsJson(
                dataDir = dataDir,
                indexDbPath = placeIndexDb,
                minLat = bounds.latitudeSouth,
                minLon = bounds.longitudeWest,
                maxLat = bounds.latitudeNorth,
                maxLon = bounds.longitudeEast,
                zoom = zoom,
                weatherPluginEnabled = weatherPluginEnabled,
                mapSymbolsEnabled = mapSymbolsEnabled,
                appActive = appActive,
            )
        }.getOrDefault("{}")
    Log.i(TAG, "tick: $raw")
    val parsed = parseWeatherMapJson(raw)
    applyWeatherMapSymbols(mapStyle, weatherIconsDir, parsed.features)
    return parsed.result.copy(rawJson = raw)
}

/** Main-thread helper used from CorridorMapView. */
fun refreshWeatherMapOnMain(
    map: MapLibreMap,
    dataDir: String,
    placeIndexDb: String,
    weatherIconsDir: String,
    weatherPluginEnabled: Boolean,
    mapSymbolsEnabled: Boolean,
    appActive: Boolean,
) {
    val mapStyle = map.style ?: return
    refreshWeatherMapSymbols(
        map = map,
        mapStyle = mapStyle,
        dataDir = dataDir,
        placeIndexDb = placeIndexDb,
        weatherIconsDir = weatherIconsDir,
        weatherPluginEnabled = weatherPluginEnabled,
        mapSymbolsEnabled = mapSymbolsEnabled,
        appActive = appActive,
    )
}

fun clearWeatherMapSymbols(mapStyle: Style) {
    runCatching {
        mapStyle.getLayer(LAYER_ID)?.let { mapStyle.removeLayer(it) }
        mapStyle.getLayer(HALO_LAYER_ID)?.let { mapStyle.removeLayer(it) }
        mapStyle.getSource(SRC_ID)?.let { mapStyle.removeSource(it) }
    }
}

private data class ParsedWeatherMap(
    val result: WeatherMapTickResult,
    val features: List<Feature>,
)

private fun parseWeatherMapJson(raw: String): ParsedWeatherMap =
    runCatching {
        val o = JSONObject(raw)
        val arr = o.optJSONArray("symbols") ?: JSONArray()
        val features = ArrayList<Feature>(arr.length())
        for (i in 0 until arr.length()) {
            val s = arr.getJSONObject(i)
            val lat = s.getDouble("lat")
            val lon = s.getDouble("lon")
            val f = Feature.fromGeometry(Point.fromLngLat(lon, lat))
            val slug = s.optString("icon_slug", "not-available")
            val stale = s.optBoolean("stale", false)
            f.addStringProperty("icon_slug", slug)
            f.addStringProperty("icon_image", "wx-$slug")
            f.addStringProperty("name", s.optString("name", ""))
            f.addBooleanProperty("stale", stale)
            features.add(f)
        }
        ParsedWeatherMap(
            result =
                WeatherMapTickResult(
                    visibleCities = o.optInt("visible_cities", 0),
                    kept = o.optInt("kept_after_declutter", features.size),
                    fetches = o.optInt("fetches", 0),
                    symbolCount = features.size,
                    zoom = o.optDouble("zoom", 0.0),
                ),
            features = features,
        )
    }.getOrElse {
        ParsedWeatherMap(WeatherMapTickResult(), emptyList())
    }

private fun applyWeatherMapSymbols(
    mapStyle: Style,
    weatherIconsDir: String,
    features: List<Feature>,
) {
    if (features.isEmpty()) {
        clearWeatherMapSymbols(mapStyle)
        return
    }
    var imagesOk = 0
    val slugs = features.map { it.getStringProperty("icon_slug") }.distinct()
    for (slug in slugs) {
        val imageId = "wx-$slug"
        if (mapStyle.getImage(imageId) != null) {
            imagesOk++
            continue
        }
        val png =
            runCatching {
                rasterizeWeatherIconPng(slug, 72u, 72u, weatherIconsDir)
            }.getOrDefault(ByteArray(0))
        if (png.isEmpty()) {
            Log.w(TAG, "rasterize empty for slug=$slug dir=$weatherIconsDir")
            continue
        }
        val icon = BitmapFactory.decodeByteArray(png, 0, png.size)
        if (icon == null) {
            Log.w(TAG, "decode failed for slug=$slug bytes=${png.size}")
            continue
        }
        mapStyle.addImage(imageId, compositeHalo(icon))
        imagesOk++
    }
    val collection = FeatureCollection.fromFeatures(features)
    val existing = mapStyle.getSource(SRC_ID) as? GeoJsonSource
    if (existing != null) {
        existing.setGeoJson(collection)
    } else {
        mapStyle.addSource(GeoJsonSource(SRC_ID, collection))
    }
    // Halo circle under the icon (host renderer contrast — not a second icon pack).
    if (mapStyle.getLayer(HALO_LAYER_ID) == null) {
        mapStyle.addLayer(
            CircleLayer(HALO_LAYER_ID, SRC_ID).withProperties(
                PropertyFactory.circleRadius(18f),
                PropertyFactory.circleColor("#FFFFFF"),
                PropertyFactory.circleOpacity(
                    Expression.switchCase(
                        Expression.eq(Expression.get("stale"), Expression.literal(true)),
                        Expression.literal(0.35f),
                        Expression.literal(0.85f),
                    ),
                ),
                PropertyFactory.circleStrokeWidth(1.2f),
                PropertyFactory.circleStrokeColor("#90A4AE"),
            ),
        )
    }
    if (mapStyle.getLayer(LAYER_ID) == null) {
        mapStyle.addLayer(
            SymbolLayer(LAYER_ID, SRC_ID).withProperties(
                PropertyFactory.iconImage(Expression.get("icon_image")),
                PropertyFactory.iconSize(1.05f),
                PropertyFactory.iconAllowOverlap(true),
                PropertyFactory.iconIgnorePlacement(true),
                PropertyFactory.iconOpacity(
                    Expression.switchCase(
                        Expression.eq(Expression.get("stale"), Expression.literal(true)),
                        Expression.literal(0.45f),
                        Expression.literal(1.0f),
                    ),
                ),
            ),
        )
    }
    Log.i(TAG, "applied symbols=${features.size} imagesOk=$imagesOk/$slugs")
}

/** White circular halo behind the fill icon for basemap contrast. */
private fun compositeHalo(icon: Bitmap): Bitmap {
    val size = 80
    val out = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
    val canvas = Canvas(out)
    val paint =
        Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = Color.argb(210, 255, 255, 255)
            this.style = Paint.Style.FILL
        }
    canvas.drawCircle(size / 2f, size / 2f, size / 2f - 1f, paint)
    val left = (size - icon.width) / 2f
    val top = (size - icon.height) / 2f
    canvas.drawBitmap(icon, left, top, null)
    return out
}

/** Visible city count helper for declutter prototyping / diagnostics. */
fun weatherMapZoomMaxForUi(): Double = weatherMapZoomMax()

fun weatherMapSpacingForUi(): Double = weatherMapMinPixelSpacing()

fun weatherMapCapForUi(): Int = weatherMapMaxSymbols().toInt()
