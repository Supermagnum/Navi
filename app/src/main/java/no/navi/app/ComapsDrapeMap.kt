package no.navi.app

import android.util.Log
import android.view.View
import android.view.ViewGroup
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.compose.LocalLifecycleOwner
import org.maplibre.android.maps.Style
import org.maplibre.android.style.layers.BackgroundLayer
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.FillExtrusionLayer
import org.maplibre.android.style.layers.FillLayer
import org.maplibre.android.style.layers.HeatmapLayer
import org.maplibre.android.style.layers.HillshadeLayer
import org.maplibre.android.style.layers.Layer
import org.maplibre.android.style.layers.LineLayer
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.RasterLayer
import org.maplibre.android.style.layers.SymbolLayer
import org.maplibre.android.style.sources.GeoJsonSource
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.log2

/** Compose host for the CoMaps drape surface (an `app.organicmaps.sdk.MapView`). */
@Composable
internal fun ComapsDrapeMap(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val owner = LocalLifecycleOwner.current
    val backend = ComapsDrape.backend ?: return
    val view = remember { backend.createMapView(context) }
    DisposableEffect(owner) {
        backend.attachLifecycle(owner)
        onDispose { backend.detachLifecycle(owner) }
    }
    AndroidView(
        factory = {
            // One process-wide surface: detach from a previous composition's parent.
            (view.parent as? ViewGroup)?.removeView(view)
            view.visibility = View.VISIBLE
            view
        },
        modifier = modifier,
    )
}

/**
 * Hides every MapLibre layer that is not a Navi overlay while the drape is the
 * visible basemap, and restores them (with their original visibility) otherwise.
 *
 * Navi overlays (route, waypoints, GPS puck, tracks, DATEX, weather, camping ...)
 * are all drawn from [GeoJsonSource]s that Navi adds at runtime; basemap layers use
 * vector / raster / raster-dem sources or no source (background).
 */
internal class DrapeBasemapMask {
    private val savedVisibility = HashMap<String, String>()

    var masked: Boolean = false
        private set

    /** Idempotent; call again after style changes so late-added basemap layers are caught. */
    fun apply(
        style: Style,
        hide: Boolean,
    ) {
        if (hide) {
            for (layer in style.layers) {
                if (layer.id in savedVisibility) continue
                if (!isBasemapLayer(style, layer)) continue
                savedVisibility[layer.id] = layer.visibility.value
                layer.setProperties(PropertyFactory.visibility(Property.NONE))
            }
            masked = true
        } else if (savedVisibility.isNotEmpty()) {
            for ((id, original) in savedVisibility) {
                style.getLayer(id)?.setProperties(PropertyFactory.visibility(original))
            }
            savedVisibility.clear()
            masked = false
        } else {
            masked = false
        }
    }

    /** The style was replaced (setStyle); saved ids refer to layers that no longer exist. */
    fun reset() {
        savedVisibility.clear()
        masked = false
    }

    private fun isBasemapLayer(
        style: Style,
        layer: Layer,
    ): Boolean {
        if (layer is BackgroundLayer) return true
        val sourceId =
            when (layer) {
                is FillLayer -> layer.sourceId
                is LineLayer -> layer.sourceId
                is SymbolLayer -> layer.sourceId
                is CircleLayer -> layer.sourceId
                is FillExtrusionLayer -> layer.sourceId
                is RasterLayer -> layer.sourceId
                is HillshadeLayer -> layer.sourceId
                is HeatmapLayer -> layer.sourceId
                else -> return false
            }
        return style.getSource(sourceId) !is GeoJsonSource
    }
}

/**
 * Keeps the drape camera on MapLibre's camera (MapLibre owns gestures and the
 * overlays). The CoMaps API has no bearing / pitch setter, so the drape is only
 * shown while MapLibre is north-up and flat; otherwise [aligned] is false and
 * MapLibre draws its own basemap.
 */
internal class DrapeCameraSync {
    private var lastPushMs = 0L
    private var lastLat = Double.NaN
    private var lastLon = Double.NaN
    private var lastLevel = Double.NaN

    /** Last observed [ComapsDrape.Backend.renderingActive]; a change forces a re-push. */
    var wasRendering = false

    /** True when the drape can reproduce this MapLibre camera (north-up and flat). */
    fun aligned(
        bearing: Double,
        tilt: Double,
    ): Boolean {
        val b = ((bearing % 360.0) + 360.0) % 360.0
        val north = b < MAX_BEARING_DEG || b > 360.0 - MAX_BEARING_DEG
        return north && tilt < MAX_TILT_DEG
    }

    fun push(
        backend: ComapsDrape.Backend,
        lat: Double,
        lon: Double,
        zoom: Double,
        force: Boolean = false,
    ) {
        val offset = NaviMapTestHooks.drapeZoomOffset ?: backend.zoomOffset() ?: DEFAULT_ZOOM_OFFSET
        val level = (zoom + offset).coerceIn(1.0, 19.0)
        val now = System.currentTimeMillis()
        val moved = abs(lat - lastLat) > 1e-7 || abs(lon - lastLon) > 1e-7 || abs(level - lastLevel) > 1e-3
        if (!force && !moved) return
        if (!force && now - lastPushMs < MIN_INTERVAL_MS) return
        lastPushMs = now
        lastLat = lat
        lastLon = lon
        lastLevel = level
        runCatching { backend.setViewport(lat, lon, level) }
            .onFailure { Log.w("ComapsDrape", "setViewport failed", it) }
        NaviMapTestHooks.drapeViewportSyncs += 1
    }

    companion object {
        /** Used before the drape surface has a size (the real value is [zoomOffset]). */
        const val DEFAULT_ZOOM_OFFSET = 0.585
        private const val MAX_BEARING_DEG = 0.5
        private const val MAX_TILT_DEG = 0.5
        private const val MIN_INTERVAL_MS = 120L

        /**
         * CoMaps scale level that draws the same metres-per-pixel as MapLibre zoom `z`:
         * `level = z + zoomOffset(...)`.
         *
         * MapLibre's world is `512 * density * 2^z` px wide. Drape's is
         * `tile * 2^(level - inc)` px wide (`GetScreenScale`), with
         * `tile = CalculateTileSize(surface)` (256..1024, power of two near the larger
         * surface side) and `inc = trunc(log2(tile / 256 / visualScale))`,
         * `visualScale = max(1.35, densityDpi / 160)` (`dp::VisualScale`).
         * Solving for the level gives `z + inc + log2(512 * density / tile)`.
         */
        fun zoomOffset(
            densityDpi: Int,
            density: Float,
            surfaceMaxPx: Int,
        ): Double {
            val visualScale = maxOf(1.35, densityDpi / 160.0)
            val tile = tileSize(surfaceMaxPx)
            val inc = log2(tile / 256.0 / visualScale).toInt()
            return inc + log2(512.0 * density / tile)
        }

        /** `df::CalculateTileSize` (non-desktop): power of two near [maxSz], halved, clamped. */
        fun tileSize(maxSz: Int): Int {
            val ceiled = 1 shl ceil(log2(maxSz + 1.0)).toInt()
            val res =
                if (maxSz < 1024) {
                    ceiled
                } else {
                    val floored = ceiled / 2
                    if (ceiled - maxSz < maxSz - floored) ceiled else floored
                }
            return (res / 2).coerceIn(256, 1024)
        }
    }
}
