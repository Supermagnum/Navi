package no.navi.app

import org.maplibre.android.maps.Style
import org.maplibre.android.style.layers.Layer
import org.maplibre.android.style.layers.SymbolLayer

/**
 * Standing MapLibre layer-stacking rule for Navi basemaps:
 *
 * **All fill / line / background layers must precede all symbol (label/icon)
 * layers.** Relative order *within* fills/lines and *within* symbols is a
 * separate concern (e.g. glacier outline above ice fill but under roads;
 * major place labels colliding ahead of POIs via symbol-sort-key).
 *
 * Offline: baked into [style.template.json]. Online Liberty: every runtime
 * [SymbolLayer] / line Navi adds must use these helpers so a future fill/
 * outline tweak cannot cover labels again.
 */
object BasemapLayerOrder {
    fun firstSymbolLayerId(style: Style): String? = style.layers.firstOrNull { it is SymbolLayer }?.id

    /**
     * Insert [layer] in the symbol block (above every fill/line).
     * If [preferBelow] is already a symbol layer, insert just under it;
     * otherwise under the first existing symbol, or append.
     */
    fun addSymbolLayer(
        style: Style,
        layer: SymbolLayer,
        preferBelow: String? = null,
    ) {
        val below =
            preferBelow?.takeIf { id -> style.getLayer(id) is SymbolLayer }
                ?: firstSymbolLayerId(style)
        when {
            below != null -> style.addLayerBelow(layer, below)
            else -> style.addLayer(layer)
        }
    }

    /**
     * Insert a non-symbol layer below the first road-like anchor that exists,
     * else below the first symbol (so it stays out of the label stack), else
     * append.
     */
    fun addFillOrLineBelowAnchors(
        style: Style,
        layer: Layer,
        preferBelowAnchors: List<String>,
    ) {
        val road =
            preferBelowAnchors.firstOrNull { id ->
                val existing = style.getLayer(id) ?: return@firstOrNull false
                existing !is SymbolLayer
            }
        val firstSym = firstSymbolLayerId(style)
        when {
            road != null -> style.addLayerBelow(layer, road)
            firstSym != null -> style.addLayerBelow(layer, firstSym)
            else -> style.addLayer(layer)
        }
    }
}
