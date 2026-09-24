package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class OnlinePlaceReverseTest {
    @Test
    fun formatReverseAddressPrefersRoadAndHouseNumber() {
        val addr =
            org.json.JSONObject(
                """{"road":"Welhavens gate","house_number":"11A","city":"Hamar"}""",
            )
        assertEquals(
            "Welhavens gate 11A",
            OnlinePlaceSearch.formatReverseAddress(addr, "Welhavens gate, Hamar, Norway"),
        )
    }

    @Test
    fun formatReverseAddressFallsBackToShortDisplayName() {
        assertEquals(
            "Kalmar",
            OnlinePlaceSearch.formatReverseAddress(null, "Kalmar, Kalmar County, Sweden"),
        )
    }

    @Test
    fun parseNominatimReverseJsonBuildsHit() {
        val body =
            """
            {
              "lat": "60.79448",
              "lon": "11.06799",
              "display_name": "Welhavens gate 11A, Hamar, Norway",
              "osm_id": 12345,
              "category": "place",
              "type": "house",
              "address": {
                "road": "Welhavens gate",
                "house_number": "11A",
                "city": "Hamar",
                "suburb": "Sentrum"
              }
            }
            """.trimIndent()
        val hit = OnlinePlaceSearch.parseNominatimReverseJson(body, 60.0, 11.0)
        assertTrue(hit != null)
        assertEquals("Welhavens gate 11A", hit!!.name)
        assertEquals(60.79448, hit.lat, 1e-5)
        assertEquals(11.06799, hit.lon, 1e-5)
        assertEquals("Hamar", hit.municipality)
        assertEquals("Sentrum", hit.subArea)
        assertTrue(hit.kind.contains("online"))
    }

    @Test
    fun parseNominatimReverseJsonRejectsErrorPayload() {
        assertNull(
            OnlinePlaceSearch.parseNominatimReverseJson(
                """{"error":"Unable to geocode"}""",
                60.0,
                11.0,
            ),
        )
    }

    @Test
    fun nominatimQueryFallbacksShortensLongAddressCsv() {
        val q =
            "Kanzlers Weide, Uferstraße, Rechtes Weserufer, Minden, " +
                "Kreis Minden-Lübbecke, North Rhine-Westphalia, 32423, Germany"
        val fb = OnlinePlaceSearch.nominatimQueryFallbacks(q)
        assertTrue(fb.first() == q.trim() || fb.contains(q.trim()))
        assertTrue(fb.any { it == "Kanzlers Weide, Germany" })
        assertTrue(fb.any { it.contains("Minden") && it.contains("Germany") })
    }

    @Test
    fun parseNominatimJsonUsesShortAddressLabel() {
        val body =
            """
            [{
              "lat":"60.792205","lon":"11.085951",
              "display_name":"11A, Welhavens gate, Espern, Hamar, Norway",
              "osm_id":1,"class":"place","type":"house",
              "address":{"road":"Welhavens gate","house_number":"11A","city":"Hamar"}
            }]
            """.trimIndent()
        val hits = OnlinePlaceSearch.parseNominatimJson(body, 5)
        assertEquals(1, hits.size)
        assertEquals("Welhavens gate 11A", hits[0].name)
    }
}
