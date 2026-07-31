package no.navi.app

import android.util.Log
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.SymbolLayer

/**
 * Street-name and basemap-POI visibility by camera zoom (MapLibre).
 *
 * Product ladder (higher zoom = more zoomed in):
 * - zoom ≥ 13: motorway labels/shields
 * - zoom ≥ 14: + secondary road names
 * - zoom ≥ 15: + other major (primary/tertiary/trunk) and minor street names
 * - zoom ≥ 16: basemap POI icons (schools, fuel, shops, …)
 *
 * Distinct from Navi [docs/poi.md] rest/overnight [PoiIndex].
 */
object BasemapLabelPolicy {
    private const val TAG = "BasemapLabelPolicy"

    const val MOTORWAY_MIN_ZOOM = 13.0
    const val SECONDARY_MIN_ZOOM = 14.0
    const val MAJOR_MINOR_MIN_ZOOM = 15.0
    const val POI_MIN_ZOOM = 16.0

    fun apply(style: Style) {
        val applied =
            runCatching { applyLiberty(style) }.getOrDefault(false) ||
                runCatching { applyProtomapsRuntime(style) }.getOrDefault(false)
        Log.i(TAG, "apply labels/pois zoom policy ok=$applied")
    }

    /** OpenFreeMap Liberty (OpenMapTiles) layers. */
    private fun applyLiberty(style: Style): Boolean {
        val major = style.getLayer("highway-name-major") as? SymbolLayer ?: return false

        // Majors: secondary from z14; primary/tertiary/trunk from z15.
        major.setMinZoom(SECONDARY_MIN_ZOOM.toFloat())
        major.setFilter(
            Expression.match(
                Expression.get("class"),
                Expression.literal(false),
                Expression.stop("primary", true),
                Expression.stop("secondary", true),
                Expression.stop("tertiary", true),
                Expression.stop("trunk", true),
            ),
        )
        major.setProperties(
            PropertyFactory.textOpacity(
                Expression.step(
                    Expression.zoom(),
                    Expression.literal(0f),
                    Expression.stop(
                        SECONDARY_MIN_ZOOM,
                        Expression.match(
                            Expression.get("class"),
                            Expression.literal(0f),
                            Expression.stop("secondary", 1f),
                        ),
                    ),
                    Expression.stop(MAJOR_MINOR_MIN_ZOOM, Expression.literal(1f)),
                ),
            ),
        )

        (style.getLayer("highway-name-minor") as? SymbolLayer)?.let { layer ->
            layer.setMinZoom(MAJOR_MINOR_MIN_ZOOM.toFloat())
            layer.setProperties(PropertyFactory.textOpacity(Expression.literal(1f)))
        }
        (style.getLayer("highway-name-path") as? SymbolLayer)?.let { layer ->
            layer.setMinZoom(MAJOR_MINOR_MIN_ZOOM.toFloat())
        }

        // Motorways: Liberty uses shields (ref), not name layers.
        for (id in listOf(
            "highway-shield-non-us",
            "highway-shield-us-interstate",
            "road_shield_us",
        )) {
            style.getLayer(id)?.setMinZoom(MOTORWAY_MIN_ZOOM.toFloat())
        }

        for (id in listOf("poi_r1", "poi_r7", "poi_r20", "poi_transit")) {
            style.getLayer(id)?.setMinZoom(POI_MIN_ZOOM.toFloat())
        }
        return true
    }

    /**
     * Offline Protomaps layers are baked in [style.template.json]. This only
     * re-asserts minzooms if the prepared style already contains them (e.g.
     * after a 3D augment reload).
     */
    private fun applyProtomapsRuntime(style: Style): Boolean {
        val motorway = style.getLayer("roads_label_motorway") ?: return false
        motorway.setMinZoom(MOTORWAY_MIN_ZOOM.toFloat())
        style.getLayer("roads_label_secondary")?.setMinZoom(SECONDARY_MIN_ZOOM.toFloat())
        style.getLayer("roads_label_major")?.setMinZoom(MAJOR_MINOR_MIN_ZOOM.toFloat())
        style.getLayer("roads_label_minor")?.setMinZoom(MAJOR_MINOR_MIN_ZOOM.toFloat())
        style.getLayer("pois")?.setMinZoom(POI_MIN_ZOOM.toFloat())
        return true
    }
}
