package no.navi.app

import android.util.Log
import org.maplibre.android.maps.Style
import org.maplibre.android.style.layers.LineLayer
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory

/**
 * Ensures OpenFreeMap Liberty tunnel road / path / railway linework is dashed.
 *
 * Upstream Liberty paints many tunnel *fill* layers solid (only some casings and
 * rail hatching use `line-dasharray`), so tunnels are easy to miss on cream land.
 * Offline Protomaps dashes are baked into `style.template.json` (`*_tunnel`
 * layers filtered on `is_tunnel`); this policy only mutates Liberty at style load.
 */
object BasemapTunnelPaint {
    private const val TAG = "BasemapTunnelPaint"

    /** Dash units match Liberty tunnel casings / waterway tunnels (readable at road widths). */
    private val TUNNEL_DASH = arrayOf(2.0f, 1.5f)

    /** Fill (and casing) line layers that must read as tunnels. */
    private val TUNNEL_LINE_LAYER_IDS =
        listOf(
            "tunnel_motorway_link_casing",
            "tunnel_service_track_casing",
            "tunnel_link_casing",
            "tunnel_street_casing",
            "tunnel_secondary_tertiary_casing",
            "tunnel_trunk_primary_casing",
            "tunnel_motorway_casing",
            "tunnel_path_pedestrian",
            "tunnel_motorway_link",
            "tunnel_service_track",
            "tunnel_link",
            "tunnel_minor",
            "tunnel_secondary_tertiary",
            "tunnel_trunk_primary",
            "tunnel_motorway",
            "tunnel_major_rail",
            "tunnel_major_rail_hatching",
            "tunnel_transit_rail",
            "tunnel_transit_rail_hatching",
        )

    fun apply(style: Style) {
        var n = 0
        for (id in TUNNEL_LINE_LAYER_IDS) {
            val layer = style.getLayer(id) as? LineLayer ?: continue
            layer.setProperties(
                PropertyFactory.lineDasharray(TUNNEL_DASH),
                PropertyFactory.visibility(Property.VISIBLE),
            )
            n++
        }
        if (n > 0) {
            Log.i(TAG, "dashed $n Liberty tunnel line layer(s)")
        }
    }

    /** Layer ids patched for unit tests / docs. */
    internal fun layerIds(): List<String> = TUNNEL_LINE_LAYER_IDS
}
