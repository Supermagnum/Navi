package no.navi.app

import android.util.Log
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.SymbolLayer

/**
 * House-number labels on OpenFreeMap Liberty (OpenMapTiles).
 *
 * Tile data already carries `source-layer: housenumber` with a `housenumber`
 * property from OpenFreeMap's z14 floor (overzoom for z15+), but upstream
 * Liberty ships **no** symbol layer for it. Offline Protomaps labels are baked
 * into `style.template.json` (`buildings_label_housenumber`).
 */
object BasemapHousenumberStyle {
    private const val TAG = "BasemapHousenumber"

    const val LAYER_ID = "housenumber_label"

    /** Matches OpenFreeMap's confirmed housenumber data floor (TileJSON maxzoom 14). */
    const val MIN_ZOOM = 14.0f

    /** Small muted digits — not styled like place-name labels. */
    private const val TEXT_COLOR = "#5a5a5a"

    fun apply(style: Style) {
        if (style.getLayer(LAYER_ID) != null) {
            return
        }
        // Prove the openmaptiles vector source is present (building fill / 3d).
        if (style.getLayer("building") == null &&
            style.getLayer("building-3d") == null
        ) {
            return
        }

        val labels =
            SymbolLayer(LAYER_ID, "openmaptiles").apply {
                sourceLayer = "housenumber"
                setMinZoom(MIN_ZOOM)
                setFilter(Expression.has("housenumber"))
                setProperties(
                    PropertyFactory.textField(Expression.get("housenumber")),
                    PropertyFactory.textFont(arrayOf("Noto Sans Regular")),
                    PropertyFactory.textSize(
                        Expression.interpolate(
                            Expression.linear(),
                            Expression.zoom(),
                            Expression.stop(14, 9.5f),
                            Expression.stop(17, 11f),
                        ),
                    ),
                    PropertyFactory.textColor(TEXT_COLOR),
                    PropertyFactory.textOptional(true),
                    PropertyFactory.textAllowOverlap(false),
                    PropertyFactory.textIgnorePlacement(false),
                    PropertyFactory.textPadding(1.5f),
                    PropertyFactory.symbolPlacement(Property.SYMBOL_PLACEMENT_POINT),
                    PropertyFactory.textAnchor(Property.TEXT_ANCHOR_CENTER),
                )
            }

        // Symbol block only — not above building fill (roads/lines would cover).
        BasemapLayerOrder.addSymbolLayer(style, labels)
        Log.i(TAG, "added $LAYER_ID minzoom=$MIN_ZOOM")
    }
}
