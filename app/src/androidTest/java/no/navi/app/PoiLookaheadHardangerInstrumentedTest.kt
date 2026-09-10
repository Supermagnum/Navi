package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.poiLookaheadConeHalfWidthDeg
import uniffi.navi.poiLookaheadConeM
import uniffi.navi.poiLookaheadDefaultEnabled
import uniffi.navi.poiLookaheadIngestFromJson
import uniffi.navi.poiLookaheadQueryJson
import uniffi.navi.poiLookaheadStrictHoursUnknownDefault

/**
 * Hardanger / Ulvik-style look-ahead cone checks (no planned route).
 *
 * Uses real cider-route coordinates with tagged JSON ingest so the device test
 * does not require a Vestlandet PBF. Mirrors the route-independent shape of
 * [LiveHazardConeVallsetInstrumentedTest] without hazard urgency chrome.
 */
@RunWith(AndroidJUnit4::class)
class PoiLookaheadHardangerInstrumentedTest {
    private companion object {
        const val TAG = "PoiLookaheadHardanger"

        // Ulvik frukt & cideri (docs/cider-route.md stop 19).
        const val CIDER_LAT = 60.57525
        const val CIDER_LON = 6.93919
        const val ORIGIN_LAT = 60.57345
        const val ORIGIN_LON = 6.93919
        const val HEADING_NORTH = 0.0
    }

    private fun fixtureJson(): String =
        """
        [
          {
            "osm_id": 2412997030,
            "lat": $CIDER_LAT,
            "lon": $CIDER_LON,
            "tags": {
              "name": "Ulvik frukt & cideri",
              "brewery": "cider",
              "opening_hours": "Mo-Su 00:00-24:00"
            }
          },
          {
            "osm_id": 99,
            "lat": $ORIGIN_LAT,
            "lon": ${ORIGIN_LON + 0.007},
            "tags": {
              "name": "Off-cone viewpoint",
              "tourism": "viewpoint"
            }
          },
          {
            "osm_id": 100,
            "lat": 60.5745,
            "lon": $ORIGIN_LON,
            "tags": {
              "name": "Closed cafe",
              "amenity": "cafe",
              "opening_hours": "Mo-Su 00:00-00:01"
            }
          },
          {
            "osm_id": 101,
            "lat": 60.5748,
            "lon": $ORIGIN_LON,
            "tags": {
              "name": "Ulvik fishing",
              "leisure": "fishing"
            }
          }
        ]
        """.trimIndent()

    @Test
    fun defaults_off_and_cone_constants() {
        assertFalse(poiLookaheadDefaultEnabled())
        assertFalse(poiLookaheadStrictHoursUnknownDefault())
        assertEquals(850.0, poiLookaheadConeM(), 0.01)
        assertEquals(30.0, poiLookaheadConeHalfWidthDeg(), 0.01)
        assertFalse(MapHudPrefs.POI_LOOKAHEAD_DEFAULT_ENABLED)
    }

    @Test
    fun hardanger_cone_filters_categories_hours_and_toggle() {
        val stats = poiLookaheadIngestFromJson("hardanger:ulvik", fixtureJson())
        Log.i(TAG, "ingest records=${stats.records} cone_m=${stats.coneM}")
        assertTrue(stats.records >= 3)

        val off =
            poiLookaheadQueryJson(
                ORIGIN_LAT,
                ORIGIN_LON,
                HEADING_NORTH,
                false,
                false,
            )
        assertEquals(0, JSONObject(off).getJSONArray("hits").length())

        val on =
            poiLookaheadQueryJson(
                ORIGIN_LAT,
                ORIGIN_LON,
                HEADING_NORTH,
                true,
                false,
            )
        Log.i(TAG, "query=$on")
        val hits = JSONObject(on).getJSONArray("hits")
        assertTrue(hits.length() >= 1)
        var sawCidery = false
        var sawOffCone = false
        var sawClosed = false
        var sawHoursUnknown = false
        for (i in 0 until hits.length()) {
            val h = hits.getJSONObject(i)
            val name = h.optString("name")
            val icon = h.optString("icon_key")
            val open = h.optString("open_now")
            val label = h.optString("label")
            if (icon == "shop-alcohol" || name.contains("cideri", ignoreCase = true)) {
                sawCidery = true
            }
            if (name.contains("Off-cone")) sawOffCone = true
            if (name.contains("Closed")) sawClosed = true
            if (label.contains("hours unknown")) sawHoursUnknown = true
            assertFalse("closed must never reach HUD: $name", open == "false")
        }
        assertTrue("cidery with shop-alcohol expected in cone", sawCidery)
        assertFalse("viewpoint east of heading must be excluded", sawOffCone)
        assertFalse("closed cafe must be suppressed", sawClosed)
        assertTrue("hours-unknown fishing should show", sawHoursUnknown)

        val hud = poiLookaheadHudFromQueryJson(on)
        assertTrue(hud.active)
        assertTrue(hud.hits.any { it.iconKey == "shop-alcohol" })
        assertFalse(hud.hits.any { it.openNow == "false" })

        val strict =
            poiLookaheadQueryJson(
                ORIGIN_LAT,
                ORIGIN_LON,
                HEADING_NORTH,
                true,
                true,
            )
        val strictHits = JSONObject(strict).getJSONArray("hits")
        for (i in 0 until strictHits.length()) {
            assertFalse(
                JSONObject(strictHits.getJSONObject(i).toString())
                    .optString("label")
                    .contains("hours unknown"),
            )
        }
    }
}
