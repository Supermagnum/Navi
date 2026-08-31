package no.navi.app

import android.util.Log
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.SymbolLayer

/**
 * Index-contour elevation labels for [MapterhornContours].
 *
 * Reads the `ele` / `major` properties set by [ContourGenerator]. Only major
 * (index) contours are labelled — minor lines stay unlabelled. Labels use
 * `symbol-placement: line` on the shared [MapterhornContours.SOURCE_ID].
 *
 * Stacked at the **bottom** of the basemap symbol block (below road/place/POI
 * text) via [BasemapLayerOrder.addSymbolLayer].
 */
object BasemapContourLabelStyle {
    private const val TAG = "BasemapContourLabel"

    const val LAYER_ID = "navi-contours-label"

    /** First zoom where index elevation text is drawn (contour lines from z9). */
    const val LABEL_MIN_ZOOM = 11f

    private const val LABEL_COLOR = "#2a2018"
    private const val LABEL_HALO = "#f5f0ea"

    /** Same factor as [BasemapPeakElevationStyle] / [DisplayUnits] US altitude. */
    private const val FEET_PER_METER = 3.28084

    fun attach(
        style: Style,
        unitSystem: UnitSystem,
    ) {
        if (style.getLayer(LAYER_ID) != null) {
            apply(style, unitSystem)
            return
        }
        if (style.getSource(MapterhornContours.SOURCE_ID) == null) return

        val labels = buildLayer(unitSystem)
        BasemapLayerOrder.addSymbolLayer(style, labels)
        NaviMapTestHooks.lastContourLabelsAttached = true
        Log.i(TAG, "added $LAYER_ID minZoom=$LABEL_MIN_ZOOM units=${unitSystem.persistId}")
    }

    fun apply(
        style: Style,
        unitSystem: UnitSystem,
    ) {
        val existing = style.getLayer(LAYER_ID) as? SymbolLayer ?: return
        existing.setProperties(PropertyFactory.textField(elevationTextField(unitSystem)))
    }

    fun detach(style: Style) {
        NaviMapTestHooks.lastContourLabelsAttached = false
        runCatching { style.removeLayer(LAYER_ID) }
    }

    private fun buildLayer(unitSystem: UnitSystem): SymbolLayer =
        SymbolLayer(LAYER_ID, MapterhornContours.SOURCE_ID).apply {
            setMinZoom(LABEL_MIN_ZOOM)
            setMaxZoom(MapterhornContours.MAX_ZOOM)
            setFilter(Expression.eq(Expression.get(ContourGenerator.PROP_MAJOR), true))
            setProperties(
                PropertyFactory.textField(elevationTextField(unitSystem)),
                PropertyFactory.textFont(arrayOf("Noto Sans Regular")),
                PropertyFactory.textSize(
                    Expression.interpolate(
                        Expression.linear(),
                        Expression.zoom(),
                        Expression.stop(11, 9f),
                        Expression.stop(13, 10f),
                        Expression.stop(15, 11f),
                    ),
                ),
                PropertyFactory.textColor(LABEL_COLOR),
                PropertyFactory.textHaloColor(LABEL_HALO),
                PropertyFactory.textHaloWidth(1.4f),
                PropertyFactory.symbolPlacement(Property.SYMBOL_PLACEMENT_LINE),
                PropertyFactory.symbolSpacing(
                    Expression.interpolate(
                        Expression.linear(),
                        Expression.zoom(),
                        Expression.stop(11, 280f),
                        Expression.stop(13, 180f),
                        Expression.stop(15, 120f),
                    ),
                ),
                PropertyFactory.textRotationAlignment(Property.TEXT_ROTATION_ALIGNMENT_MAP),
                PropertyFactory.textPitchAlignment(Property.TEXT_PITCH_ALIGNMENT_VIEWPORT),
                PropertyFactory.textKeepUpright(true),
                PropertyFactory.textMaxAngle(30f),
                PropertyFactory.textOptional(true),
                PropertyFactory.textAllowOverlap(false),
                PropertyFactory.textIgnorePlacement(false),
                PropertyFactory.textPadding(2f),
            )
        }

    /** Elevation label from feature `ele` (metres). Feet for US imperial only. */
    internal fun elevationTextField(unitSystem: UnitSystem): Expression =
        if (unitSystem == UnitSystem.IMPERIAL_US) {
            Expression.concat(
                Expression.toString(
                    Expression.round(
                        Expression.product(
                            Expression.toNumber(Expression.get(ContourGenerator.PROP_ELEV)),
                            Expression.literal(FEET_PER_METER),
                        ),
                    ),
                ),
                Expression.literal(" ft"),
            )
        } else {
            Expression.concat(
                Expression.toString(
                    Expression.round(Expression.toNumber(Expression.get(ContourGenerator.PROP_ELEV))),
                ),
                Expression.literal(" m"),
            )
        }
}
