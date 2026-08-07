package no.navi.app

import android.util.Log
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.LineLayer
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.SymbolLayer

/**
 * Protected-area visibility on OpenFreeMap Liberty (OpenMapTiles).
 *
 * Tile data already carries `park.class` ∈ {national_park, protected_area,
 * nature_reserve} with names from ~z7, but upstream Liberty ships:
 * - a near-invisible dashed `park_outline` (`rgba(228,241,215)` on `#d8e8c8`)
 * - **no** symbol layer on `source-layer: park`, so names never render
 *
 * Offline Protomaps equivalents are baked into `style.template.json`.
 */
object BasemapProtectedAreaStyle {
    private const val TAG = "BasemapProtectedArea"

    /** Saturated green — readable on Liberty park fill and mixed landcover. */
    private const val OUTLINE_COLOR = "#2f7a32"

    /** Distinct from black town/city place labels. */
    private const val LABEL_COLOR = "#1e5a28"

    const val PARK_LABEL_LAYER_ID = "park_label_protected"

    /** Slightly above the z7 data floor to limit country-scale clutter. */
    const val LABEL_MIN_ZOOM = 8.0f

    fun apply(style: Style) {
        val outline = style.getLayer("park_outline") as? LineLayer
        if (outline != null) {
            outline.setProperties(
                PropertyFactory.lineColor(OUTLINE_COLOR),
                PropertyFactory.lineWidth(1.5f),
                PropertyFactory.lineDasharray(arrayOf(2f, 1.5f)),
                PropertyFactory.lineOpacity(0.9f),
            )
            Log.i(TAG, "patched park_outline contrast")
        }

        if (style.getLayer(PARK_LABEL_LAYER_ID) != null) {
            return
        }
        // Liberty park fill/outline prove the openmaptiles source is present.
        if (style.getLayer("park") == null && outline == null) {
            return
        }

        val labels =
            SymbolLayer(PARK_LABEL_LAYER_ID, "openmaptiles").apply {
                sourceLayer = "park"
                setMinZoom(LABEL_MIN_ZOOM)
                setFilter(
                    Expression.all(
                        Expression.has("name"),
                        Expression.match(
                            Expression.get("class"),
                            Expression.literal(false),
                            Expression.stop("national_park", true),
                            Expression.stop("protected_area", true),
                            Expression.stop("nature_reserve", true),
                        ),
                    ),
                )
                setProperties(
                    PropertyFactory.textField(Expression.get("name")),
                    PropertyFactory.textFont(arrayOf("Noto Sans Italic")),
                    PropertyFactory.textSize(
                        Expression.interpolate(
                            Expression.linear(),
                            Expression.zoom(),
                            Expression.stop(8, 11f),
                            Expression.stop(12, 13f),
                        ),
                    ),
                    PropertyFactory.textColor(LABEL_COLOR),
                    PropertyFactory.textHaloColor("#f8f4f0"),
                    PropertyFactory.textHaloWidth(1.4f),
                    PropertyFactory.textOptional(true),
                    PropertyFactory.textAllowOverlap(false),
                    PropertyFactory.textIgnorePlacement(false),
                    PropertyFactory.symbolPlacement(Property.SYMBOL_PLACEMENT_POINT),
                    PropertyFactory.textMaxWidth(10f),
                )
            }

        val above =
            when {
                style.getLayer("park_outline") != null -> "park_outline"
                style.getLayer("park") != null -> "park"
                else -> null
            }
        if (above != null) {
            style.addLayerAbove(labels, above)
        } else {
            style.addLayer(labels)
        }
        Log.i(TAG, "added $PARK_LABEL_LAYER_ID minzoom=$LABEL_MIN_ZOOM")
    }
}
