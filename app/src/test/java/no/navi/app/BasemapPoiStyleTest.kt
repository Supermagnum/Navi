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
    fun alcoholIsWhitelistedWithTownspotFallback() {
        val pois = layerJson("pois")
        val kinds = kindWhitelist(pois)
        assertTrue("alcohol must be in the pois kind whitelist", kinds.contains("alcohol"))

        val icon = iconForKind(pois, "alcohol")
        assertEquals(
            "bundled sprites have no alcohol icon; use townspot like fuel/pharmacy",
            "townspot",
            icon,
        )
        assertTrue(spriteKeys().contains("townspot"))
        assertFalse(spriteKeys().contains("alcohol"))
        for (kind in vehicleRepairKinds) {
            assertTrue("$kind must be in the pois kind whitelist", kinds.contains(kind))
            assertEquals("townspot", iconForKind(pois, kind))
            assertFalse(spriteKeys().contains(kind))
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
