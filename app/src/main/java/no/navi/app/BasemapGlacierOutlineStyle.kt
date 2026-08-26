package no.navi.app

import android.util.Log
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.FillLayer
import org.maplibre.android.style.layers.LineLayer
import org.maplibre.android.style.layers.PropertyFactory

/**
 * Dashed glacier outlines for visibility against pale ice fill.
 *
 * Offline Protomaps outlines are baked into `style.template.json`
 * (`landcover_glacier_outline`, `landuse_glacier_outline`) after land fills
 * and before water/roads, and still before every symbol layer (see
 * [BasemapLayerOrder]). This policy only patches OpenFreeMap Liberty, which
 * ships `landcover_ice` as fill with no outline. Color is a cool teal rather
 * than the nature-reserve green so ice does not read as a park boundary.
 *
 * Liberty inserts the outline below the first road/tunnel casing (still in
 * the fill/line block, never after labels) so wetland/landuse/water/sand
 * fills drawn after `landcover_ice` cannot hide the dashed edge, while
 * park/place/POI labels stay on top.
 */
object BasemapGlacierOutlineStyle {
    private const val TAG = "BasemapGlacierOutline"

    const val LIBERTY_LAYER_ID = "landcover_ice_outline"

    /** Teal on pale ice (`#C8E9E9` / Liberty `rgba(224,236,236)`). */
    const val OUTLINE_COLOR = "#2a6e70"

    /**
     * Prefer inserting below these so outline sits above land fills but under
     * roads (and under all symbols via [BasemapLayerOrder]). First match wins.
     */
    private val BELOW_ROAD_ANCHORS =
        listOf(
            "tunnel_motorway_link_casing",
            "tunnel_service_track_casing",
            "road_motorway_casing",
            "highway_motorway_casing",
            "road_path",
            "bridge_motorway_link_casing",
        )

    fun apply(style: Style) {
        if (style.getLayer(LIBERTY_LAYER_ID) != null) {
            return
        }
        val ice = style.getLayer("landcover_ice") as? FillLayer
        if (ice == null) {
            return
        }

        val outline =
            LineLayer(LIBERTY_LAYER_ID, "openmaptiles").apply {
                sourceLayer = "landcover"
                setMinZoom(5f)
                setFilter(Expression.eq(Expression.get("class"), "ice"))
                setProperties(
                    PropertyFactory.lineColor(OUTLINE_COLOR),
                    PropertyFactory.lineWidth(1.5f),
                    PropertyFactory.lineDasharray(arrayOf(2f, 1.5f)),
                    PropertyFactory.lineOpacity(0.9f),
                )
            }
        BasemapLayerOrder.addFillOrLineBelowAnchors(style, outline, BELOW_ROAD_ANCHORS)
        Log.i(TAG, "added $LIBERTY_LAYER_ID")
    }
}
