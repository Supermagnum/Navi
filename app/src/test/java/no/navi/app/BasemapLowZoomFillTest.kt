package no.navi.app

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Issue 152: at z5 and below the neighbouring country showed as one flat land
 * colour. With the world overview mounted, regional archives were hidden below
 * [BasemapStyleResolver.REGIONAL_HANDOVER_MINZOOM], leaving the overview as the
 * only source of land and water fills. On device the overview renders some z5
 * tiles flat, while the regional archive carries the same tile with landcover,
 * landuse and lakes. Fills now stay visible from z0; only roads, boundaries and
 * labels keep the z6 handover. Nothing changes at z6 and above.
 */
class BasemapLowZoomFillTest {
    private val handover = BasemapStyleResolver.REGIONAL_HANDOVER_MINZOOM

    @Test
    fun handover_never_holds_back_land_or_water_fills() {
        assertNull(BasemapStyleResolver.handoverMinZoom(BasemapStyleResolver.LayerKind.EarthLand, handover))
        assertNull(BasemapStyleResolver.handoverMinZoom(BasemapStyleResolver.LayerKind.Water, handover))
    }

    @Test
    fun handover_still_applies_to_roads_and_labels() {
        assertEquals(handover, BasemapStyleResolver.handoverMinZoom(BasemapStyleResolver.LayerKind.Roads, handover))
        assertEquals(handover, BasemapStyleResolver.handoverMinZoom(BasemapStyleResolver.LayerKind.Labels, handover))
        assertNull(BasemapStyleResolver.handoverMinZoom(BasemapStyleResolver.LayerKind.Roads, null))
    }

    @Test
    fun shipped_template_fill_layers_have_no_zoom_window() {
        val fills = fillLayers(templateLayers())
        assertTrue("template must carry earth, landcover, landuse and water fills", fills.size >= 4)
        for (layer in fills) {
            assertFalse("${layer.optString("id")} has minzoom", layer.has("minzoom"))
            assertFalse("${layer.optString("id")} has maxzoom", layer.has("maxzoom"))
        }
    }

    @Test
    fun regional_fills_draw_from_z0_with_overview_mounted() {
        val out = JSONArray()
        val template = templateLayers()
        for (kind in listOf(
            BasemapStyleResolver.LayerKind.EarthLand,
            BasemapStyleResolver.LayerKind.Water,
            BasemapStyleResolver.LayerKind.Roads,
            BasemapStyleResolver.LayerKind.Labels,
        )) {
            val group = JSONArray()
            for (i in 0 until template.length()) {
                val layer = template.getJSONObject(i)
                if (BasemapStyleResolver.layerKind(layer) == kind) group.put(layer)
            }
            BasemapStyleResolver.appendClonedLayers(
                out,
                group,
                "region0",
                BasemapStyleResolver.handoverMinZoom(kind, handover),
                null,
            )
        }
        var fillCount = 0
        for (i in 0 until out.length()) {
            val layer = out.getJSONObject(i)
            val id = layer.getString("id")
            if (layer.getString("source-layer") in FILL_SOURCE_LAYERS && layer.getString("type") == "fill") {
                fillCount += 1
                assertFalse("$id must not be hidden below z6", layer.has("minzoom"))
            }
            if (layer.getString("type") == "symbol") {
                assertTrue("$id label handover", layer.getDouble("minzoom") >= handover)
            }
        }
        assertTrue(fillCount >= 4)
    }

    @Test
    fun overview_keeps_its_cap_and_label_handover() {
        val template = templateLayers()
        val out = JSONArray()
        BasemapStyleResolver.appendClonedLayers(
            out,
            template,
            "overview",
            null,
            BasemapStyleResolver.OVERVIEW_HANDOVER_MAXZOOM,
        )
        for (i in 0 until out.length()) {
            val layer = out.getJSONObject(i)
            val maxZoom = layer.getDouble("maxzoom")
            if (layer.getString("type") == "symbol") {
                assertEquals(handover, maxZoom, 0.0)
            } else {
                assertEquals(BasemapStyleResolver.OVERVIEW_HANDOVER_MAXZOOM, maxZoom, 0.0)
            }
        }
    }

    @Test
    fun z6_and_above_window_is_unchanged() {
        // Before and after this change regional layers were visible at z6+ and the
        // overview was capped above z7, so the z6..z7 overlap is the same.
        assertEquals(6.0, handover, 0.0)
        assertEquals(7.0, BasemapStyleResolver.OVERVIEW_HANDOVER_MAXZOOM, 0.0)
        assertEquals(handover, BasemapStyleResolver.handoverMinZoom(BasemapStyleResolver.LayerKind.Roads, handover))
    }

    private fun fillLayers(layers: JSONArray): List<JSONObject> {
        val out = ArrayList<JSONObject>()
        for (i in 0 until layers.length()) {
            val layer = layers.getJSONObject(i)
            if (layer.optString("type") == "fill" &&
                layer.optString("source-layer") in FILL_SOURCE_LAYERS
            ) {
                out.add(layer)
            }
        }
        return out
    }

    private fun templateLayers(): JSONArray {
        val file = File("src/main/assets/map-styles/protomaps-light/style.template.json")
        assertTrue("template not found at ${file.absolutePath}", file.isFile)
        return JSONObject(file.readText()).getJSONArray("layers")
    }

    private companion object {
        val FILL_SOURCE_LAYERS = setOf("earth", "landcover", "landuse", "water")
    }
}
