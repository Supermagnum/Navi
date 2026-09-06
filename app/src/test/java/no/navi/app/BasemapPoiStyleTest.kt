package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Offline Protomaps `pois` layer: kind whitelist, per-kind zoom floor, and
 * sprite fallback. Guards the Vinmonopolet / `shop=alcohol` render miss.
 *
 * Avoids `org.json` (Android unit-test stubs throw "not mocked").
 */
class BasemapPoiStyleTest {
    private val vehicleRepairKinds =
        listOf(
            "car_repair",
            "motorcycle_repair",
            "motorcycle",
            "bicycle_repair",
            "bicycle_repair_station",
            "bicycle",
        )

    @Test
    fun railwayStationIsWhitelistedWithDedicatedSprite() {
        val pois = layerJson("pois")
        val kinds = kindWhitelist(pois)
        assertTrue("station must be in the pois kind whitelist", kinds.contains("station"))
        assertEquals("station", iconForKind(pois, "station"))
        assertTrue(spriteKeys().contains("station"))
        val floors = kindZoomFloors(pois)
        assertEquals(12.0, floors["station"] ?: error("station zoom floor missing"), 0.0)
        assertFalse(
            passesPoisFilter(kinds, floors, kind = "station", minZoom = 10.0, zoom = 11.0),
        )
        assertTrue(
            passesPoisFilter(kinds, floors, kind = "station", minZoom = 10.0, zoom = 12.0),
        )
    }

    @Test
    fun alcoholHasDedicatedSprite() {
        val pois = layerJson("pois")
        val kinds = kindWhitelist(pois)
        assertTrue("alcohol must be in the pois kind whitelist", kinds.contains("alcohol"))

        val icon = iconForKind(pois, "alcohol")
        assertEquals("alcohol", icon)
        assertTrue(spriteKeys().contains("alcohol"))
        for (kind in vehicleRepairKinds) {
            assertTrue("$kind must be in the pois kind whitelist", kinds.contains(kind))
            assertEquals(kind, iconForKind(pois, kind))
            assertTrue(spriteKeys().contains(kind))
        }
    }

    @Test
    fun alcoholVisibleFromZ16NotZ15() {
        val pois = layerJson("pois")
        val kinds = kindWhitelist(pois)
        val floors = kindZoomFloors(pois)

        assertEquals(16.0, floors["alcohol"] ?: floors.getValue("__default"), 0.0)
        assertFalse(passesPoisFilter(kinds, floors, kind = "alcohol", minZoom = 16.0, zoom = 15.0))
        assertTrue(passesPoisFilter(kinds, floors, kind = "alcohol", minZoom = 16.0, zoom = 16.0))
        assertTrue(passesPoisFilter(kinds, floors, kind = "alcohol", minZoom = 16.0, zoom = 17.0))
    }

    @Test
    fun supermarketStillVisibleAtZ16() {
        val pois = layerJson("pois")
        val kinds = kindWhitelist(pois)
        val floors = kindZoomFloors(pois)
        assertTrue(kinds.contains("supermarket"))
        assertTrue(
            passesPoisFilter(kinds, floors, kind = "supermarket", minZoom = 15.0, zoom = 16.0),
        )
    }

    @Test
    fun peakLabelUsesElevationProperty() {
        val pois = layerJson("pois")
        val layoutAt = pois.indexOf("\"text-field\"")
        assertTrue("pois must have text-field", layoutAt >= 0)
        val field = arraySlice(pois, pois.indexOf('[', layoutAt))
        assertTrue("peak labels must read tile elevation, not only name", field.contains("\"elevation\""))
        assertTrue("peak kinds must be gated in the text-field", field.contains("\"peak\""))
        assertFalse("must not use OSM ele key; Protomaps tiles use elevation", field.contains("\"ele\""))
        val floors = kindZoomFloors(pois)
        assertEquals(13.0, floors["peak"] ?: error("peak zoom floor missing"), 0.0)
        assertEquals(13.0, floors["hill"] ?: error("hill zoom floor missing"), 0.0)
        assertEquals(
            13.0f,
            BasemapPeakElevationStyle.LIBERTY_MIN_ZOOM,
            0.0f,
        )
    }

    @Test
    fun glacierLabelUsesOutlineTeal() {
        val pois = layerJson("pois")
        val paintAt = pois.indexOf("\"text-color\"")
        assertTrue("pois must have text-color", paintAt >= 0)
        val colorExpr = arraySlice(pois, pois.indexOf('[', paintAt))
        assertTrue(
            "glacier labels must use outline teal on ice fill",
            colorExpr.contains("\"glacier\"") && colorExpr.contains("#2a6e70"),
        )
        assertFalse("must not keep muddy grey-teal", colorExpr.contains("#406060"))
    }

    @Test
    fun glacierHasDashedOutlineLayers() {
        for (id in listOf("landcover_glacier_outline", "landuse_glacier_outline")) {
            val layer = layerJson(id)
            assertTrue("$id must be a line layer", layer.contains("\"type\": \"line\""))
            assertTrue("$id must dash", layer.contains("\"line-dasharray\""))
            assertTrue("$id must key on glacier", layer.contains("\"glacier\""))
            assertTrue("$id uses teal not reserve green", layer.contains("#2a6e70"))
        }
        val reserve = layerJson("landuse_protected_outline")
        assertTrue(reserve.contains("#2f7a32"))
    }

    @Test
    fun glacierOutlinesStackAboveLandFills() {
        val text = templateFile().readText()

        fun layerIndex(id: String): Int {
            val at = text.indexOf("\"id\": \"$id\"")
            assertTrue("missing layer $id", at >= 0)
            return at
        }

        val landcover = layerIndex("landcover")
        val landuse = layerIndex("landuse")
        val protectedOutline = layerIndex("landuse_protected_outline")
        val landcoverOutline = layerIndex("landcover_glacier_outline")
        val landuseOutline = layerIndex("landuse_glacier_outline")
        val water = layerIndex("water")
        assertTrue("landcover fill before landuse fill", landcover < landuse)
        assertTrue(
            "landcover glacier outline after land fills",
            landcoverOutline > landuse && landcoverOutline > protectedOutline,
        )
        assertTrue(
            "landuse glacier outline after land fills",
            landuseOutline > landuse && landuseOutline > protectedOutline,
        )
        assertTrue(
            "glacier outlines before water/roads",
            landcoverOutline < water && landuseOutline < water,
        )
    }

    @Test
    fun whitelistedKindsDoNotFallBackToTownspot() {
        val pois = layerJson("pois")
        val kinds = kindWhitelist(pois)
        val sprites = spriteKeys()
        for (kind in kinds) {
            val icon = iconForKind(pois, kind)
            assertTrue(
                "$kind must not use townspot fallback (got $icon)",
                icon != "townspot",
            )
            assertTrue(
                "$kind icon $icon must exist in light.json",
                sprites.contains(icon),
            )
        }
    }

    private fun passesPoisFilter(
        kinds: Set<String>,
        floors: Map<String, Double>,
        kind: String,
        minZoom: Double,
        zoom: Double,
    ): Boolean {
        if (zoom < minZoom) return false
        val floor = floors[kind] ?: floors.getValue("__default")
        if (zoom < floor) return false
        return kinds.contains(kind)
    }

    private fun layerJson(id: String): String {
        val text = templateFile().readText()
        val marker = "\"id\": \"$id\""
        val markerAt = text.indexOf(marker)
        require(markerAt >= 0) { "missing layer $id" }
        var start = markerAt
        while (start > 0 && text[start] != '{') start--
        return braceSlice(text, start)
    }

    private fun kindWhitelist(pois: String): Set<String> {
        val filterAt = pois.indexOf("\"filter\"")
        require(filterAt >= 0)
        val filter = arraySlice(pois, pois.indexOf('[', filterAt))
        // Last match in the all-filter is the kind allow-list: [ "match", ["get","kind"], [ kinds... ], true, false ]
        val lastMatch = filter.lastIndexOf("\"match\"")
        val kindsArrayAt = filter.indexOf('[', filter.indexOf('[', lastMatch) + 1)
        val kindsJson = arraySlice(filter, kindsArrayAt)
        return quotedStrings(kindsJson).toSet()
    }

    private fun kindZoomFloors(pois: String): Map<String, Double> {
        val filterAt = pois.indexOf("\"filter\"")
        val filter = arraySlice(pois, pois.indexOf('[', filterAt))
        val matchAt = filter.indexOf("\"match\"")
        val match = arraySlice(filter, filter.lastIndexOf('[', matchAt))
        // ["match", ["get","kind"], "glacier", 12, "wetland", 12, "townhall", 15, 16]
        val tokens = matchTokens(match).drop(1)
        require(tokens.first().startsWith("[")) { "expected get-kind, got ${tokens.first()}" }
        val rest = tokens.drop(1)
        val out = mutableMapOf<String, Double>()
        var i = 0
        while (i + 1 < rest.size) {
            out[unquote(rest[i])] = rest[i + 1].toDouble()
            i += 2
        }
        out["__default"] = rest.last().toDouble()
        return out
    }

    private fun iconForKind(
        pois: String,
        kind: String,
    ): String {
        val layoutAt = pois.indexOf("\"icon-image\"")
        val match = arraySlice(pois, pois.indexOf('[', layoutAt))
        val tokens = matchTokens(match).drop(1)
        require(tokens.first().startsWith("["))
        val rest = tokens.drop(1)
        var i = 0
        while (i + 1 < rest.size) {
            if (unquote(rest[i]) == kind) return unquote(rest[i + 1])
            i += 2
        }
        return unquote(rest.last())
    }

    private fun spriteKeys(): Set<String> {
        val text = spriteFile().readText()
        return quotedStrings(text).toSet()
    }

    private fun quotedStrings(json: String): List<String> {
        val out = mutableListOf<String>()
        val re = Regex("\"((?:\\\\.|[^\"])*)\"")
        for (m in re.findAll(json)) {
            out.add(m.groupValues[1])
        }
        return out
    }

    private fun unquote(token: String): String = token.removeSurrounding("\"")

    private fun matchTokens(arrayJson: String): List<String> {
        val inner = arrayJson.trim().removePrefix("[").removeSuffix("]")
        val tokens = mutableListOf<String>()
        var i = 0
        while (i < inner.length) {
            when (val c = inner[i]) {
                ' ', '\n', '\r', '\t', ',' -> i++
                '"' -> {
                    val end = inner.indexOf('"', i + 1)
                    tokens.add(inner.substring(i, end + 1))
                    i = end + 1
                }
                '[' -> {
                    val slice = arraySlice(inner, i)
                    tokens.add(slice)
                    i += slice.length
                }
                else -> {
                    val end = inner.indexOfFirstFrom(i) { ch -> ch == ',' || ch.isWhitespace() || ch == ']' }
                    val tok = inner.substring(i, if (end < 0) inner.length else end).trim()
                    if (tok.isNotEmpty()) tokens.add(tok)
                    i = if (end < 0) inner.length else end
                }
            }
        }
        return tokens
    }

    private fun arraySlice(
        text: String,
        start: Int,
    ): String {
        require(text[start] == '[')
        var depth = 0
        for (i in start until text.length) {
            when (text[i]) {
                '[' -> depth++
                ']' -> {
                    depth--
                    if (depth == 0) return text.substring(start, i + 1)
                }
            }
        }
        error("unbalanced array")
    }

    private fun braceSlice(
        text: String,
        start: Int,
    ): String {
        require(text[start] == '{')
        var depth = 0
        for (i in start until text.length) {
            when (text[i]) {
                '{' -> depth++
                '}' -> {
                    depth--
                    if (depth == 0) return text.substring(start, i + 1)
                }
            }
        }
        error("unbalanced object")
    }

    private fun String.indexOfFirstFrom(
        start: Int,
        pred: (Char) -> Boolean,
    ): Int {
        for (i in start until length) {
            if (pred(this[i])) return i
        }
        return -1
    }

    private fun templateFile(): File =
        firstExisting(
            "src/main/assets/map-styles/protomaps-light/style.template.json",
            "app/src/main/assets/map-styles/protomaps-light/style.template.json",
        )

    private fun spriteFile(): File =
        firstExisting(
            "src/main/assets/map-styles/protomaps-light/sprites/light.json",
            "app/src/main/assets/map-styles/protomaps-light/sprites/light.json",
        )

    private fun firstExisting(vararg rel: String): File =
        rel.map(::File).firstOrNull { it.isFile }
            ?: error("missing ${rel.joinToString()}")
}
