package no.navi.app

import android.util.Log
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.SymbolLayer

/**
 * Peak/hill height labels.
 *
 * Offline Protomaps `pois` already draws `kind=peak` / `hill` names; OSM `ele`
 * is tiled as numeric `elevation` (metres). The template defaults to metres;
 * this policy rewrites the text field for [UnitSystem.IMPERIAL_US] feet.
 *
 * OpenFreeMap Liberty tiles expose a `mountain_peak` source-layer with `ele` /
 * `ele_ft`, but upstream Liberty ships no symbol layer for it (same gap as
 * housenumbers). This adds one.
 */
object BasemapPeakElevationStyle {
    private const val TAG = "BasemapPeakElevation"

    const val LIBERTY_LAYER_ID = "mountain_peak_label"

    /**
     * Same floor as openstreetmap.org / OSM Carto peak names (~z13).
     * Offline Protomaps uses the same number in the `pois` kind match.
     */
    const val LIBERTY_MIN_ZOOM = 13.0f

    private const val LABEL_COLOR = "#303030"

    /** Same factor as [DisplayUnits] US altitude (feet). */
    private const val FEET_PER_METER = 3.28084

    fun apply(
        style: Style,
        unitSystem: UnitSystem,
    ) {
        val pois = style.getLayer("pois") as? SymbolLayer
        if (pois != null) {
            pois.setProperties(PropertyFactory.textField(protomapsTextField(unitSystem)))
            Log.i(TAG, "patched pois peak elevation units=${unitSystem.persistId}")
        }
        applyLiberty(style, unitSystem)
    }

    private fun applyLiberty(
        style: Style,
        unitSystem: UnitSystem,
    ) {
        val existing = style.getLayer(LIBERTY_LAYER_ID) as? SymbolLayer
        if (existing != null) {
            existing.setProperties(PropertyFactory.textField(libertyTextField(unitSystem)))
            return
        }
        if (style.getLayer("poi_r1") == null &&
            style.getLayer("landcover_ice") == null &&
            style.getLayer("park_outline") == null
        ) {
            return
        }

        val labels =
            SymbolLayer(LIBERTY_LAYER_ID, "openmaptiles").apply {
                sourceLayer = "mountain_peak"
                setMinZoom(LIBERTY_MIN_ZOOM)
                setFilter(
                    Expression.all(
                        Expression.has("name"),
                        Expression.match(
                            Expression.get("class"),
                            Expression.literal(false),
                            Expression.stop("peak", true),
                            Expression.stop("mountain_peak", true),
                        ),
                    ),
                )
                setProperties(
                    PropertyFactory.textField(libertyTextField(unitSystem)),
                    PropertyFactory.textFont(arrayOf("Noto Sans Regular")),
                    PropertyFactory.textSize(
                        Expression.interpolate(
                            Expression.linear(),
                            Expression.zoom(),
                            Expression.stop(12, 11f),
                            Expression.stop(16, 13f),
                        ),
                    ),
                    PropertyFactory.textColor(LABEL_COLOR),
                    PropertyFactory.textHaloColor("#f8f4f0"),
                    PropertyFactory.textHaloWidth(1.2f),
                    PropertyFactory.textOptional(true),
                    PropertyFactory.textAllowOverlap(false),
                    PropertyFactory.textIgnorePlacement(false),
                    PropertyFactory.textAnchor(Property.TEXT_ANCHOR_TOP),
                    PropertyFactory.textOffset(arrayOf(0f, 0.9f)),
                    PropertyFactory.textMaxWidth(10f),
                    PropertyFactory.symbolPlacement(Property.SYMBOL_PLACEMENT_POINT),
                )
            }

        // Prefer under Liberty POI symbols; never fall back onto a fill/line
        // (park_outline / landcover_ice) — that put peaks under later outlines.
        BasemapLayerOrder.addSymbolLayer(
            style,
            labels,
            preferBelow = "poi_r1",
        )
        Log.i(TAG, "added $LIBERTY_LAYER_ID minzoom=$LIBERTY_MIN_ZOOM units=${unitSystem.persistId}")
    }

    internal fun protomapsTextField(unitSystem: UnitSystem): Expression {
        val height =
            if (unitSystem == UnitSystem.IMPERIAL_US) {
                Expression.concat(
                    Expression.toString(
                        Expression.round(
                            Expression.product(
                                Expression.toNumber(Expression.get("elevation")),
                                Expression.literal(FEET_PER_METER),
                            ),
                        ),
                    ),
                    Expression.literal(" ft"),
                )
            } else {
                Expression.concat(
                    Expression.toString(
                        Expression.round(Expression.toNumber(Expression.get("elevation"))),
                    ),
                    Expression.literal(" m"),
                )
            }
        return Expression.switchCase(
            Expression.all(
                Expression.match(
                    Expression.get("kind"),
                    Expression.literal(false),
                    Expression.stop("peak", true),
                    Expression.stop("hill", true),
                ),
                Expression.has("elevation"),
            ),
            Expression.concat(
                Expression.coalesce(Expression.get("name"), Expression.literal("")),
                Expression.literal("\n"),
                height,
            ),
            Expression.get("name"),
        )
    }

    private fun libertyTextField(unitSystem: UnitSystem): Expression {
        val name = Expression.coalesce(Expression.get("name"), Expression.literal(""))
        return if (unitSystem == UnitSystem.IMPERIAL_US) {
            Expression.switchCase(
                Expression.has("ele_ft"),
                Expression.concat(
                    name,
                    Expression.literal("\n"),
                    Expression.toString(Expression.round(Expression.toNumber(Expression.get("ele_ft")))),
                    Expression.literal(" ft"),
                ),
                Expression.has("ele"),
                Expression.concat(
                    name,
                    Expression.literal("\n"),
                    Expression.toString(
                        Expression.round(
                            Expression.product(
                                Expression.toNumber(Expression.get("ele")),
                                Expression.literal(FEET_PER_METER),
                            ),
                        ),
                    ),
                    Expression.literal(" ft"),
                ),
                name,
            )
        } else {
            Expression.switchCase(
                Expression.has("ele"),
                Expression.concat(
                    name,
                    Expression.literal("\n"),
                    Expression.toString(Expression.round(Expression.toNumber(Expression.get("ele")))),
                    Expression.literal(" m"),
                ),
                name,
            )
        }
    }
}
