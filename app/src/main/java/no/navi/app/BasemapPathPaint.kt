package no.navi.app

import android.util.Log
import org.maplibre.android.maps.Style
import org.maplibre.android.style.layers.LineLayer
import org.maplibre.android.style.layers.PropertyFactory

/**
 * Fixes Liberty path/track paint that ships as pure white (`hsl(0,0%,100%)` /
 * `#fff`) and is effectively invisible on Liberty's cream land (`#f8f4f0`).
 *
 * Offline Protomaps path paint is baked into `style.template.json` (`roads_other`);
 * this policy only mutates remote OpenFreeMap Liberty layers at style load.
 */
object BasemapPathPaint {
    private const val TAG = "BasemapPathPaint"

    /** Reddish-brown dashed paths — readable on `#f8f4f0` land and green landcover. */
    private const val PATH_COLOR = "#7a4a28"

    /** Slightly lighter fill for service/track lanes that sit inside a casing. */
    private const val TRACK_FILL_COLOR = "#8b5e3c"

    private val PATH_LAYER_IDS =
        listOf(
            "road_path_pedestrian",
            "tunnel_path_pedestrian",
            "bridge_path_pedestrian",
        )

    private val TRACK_LAYER_IDS =
        listOf(
            "road_service_track",
            "tunnel_service_track",
            "bridge_service_track",
        )

    fun apply(style: Style) {
        var n = 0
        for (id in PATH_LAYER_IDS) {
            val layer = style.getLayer(id) as? LineLayer ?: continue
            layer.setProperties(
                PropertyFactory.lineColor(PATH_COLOR),
                // Slightly denser dashes than upstream so thin paths remain legible.
                PropertyFactory.lineDasharray(arrayOf(1.2f, 0.6f)),
            )
            n++
        }
        for (id in TRACK_LAYER_IDS) {
            val layer = style.getLayer(id) as? LineLayer ?: continue
            layer.setProperties(PropertyFactory.lineColor(TRACK_FILL_COLOR))
            n++
        }
        if (n > 0) {
            Log.i(TAG, "patched $n Liberty path/track line layer(s)")
        }
    }
}
