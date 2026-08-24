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
 * (`landcover_glacier_outline`, `landuse_glacier_outline`). This policy only
 * patches OpenFreeMap Liberty, which ships `landcover_ice` as fill with no
 * outline. Color is a cool teal rather than the nature-reserve green so ice
 * does not read as a park boundary.
 */
object BasemapGlacierOutlineStyle {
    private const val TAG = "BasemapGlacierOutline"

    const val LIBERTY_LAYER_ID = "landcover_ice_outline"

    /** Teal on pale ice (`#C8E9E9` / Liberty `rgba(224,236,236)`). */
    const val OUTLINE_COLOR = "#2a6e70"

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
        style.addLayerAbove(outline, "landcover_ice")
        Log.i(TAG, "added $LIBERTY_LAYER_ID")
    }
}
