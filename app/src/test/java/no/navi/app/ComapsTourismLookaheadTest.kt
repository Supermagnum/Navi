package no.navi.app

import org.json.JSONArray
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.navi.FfiSearchHit
import uniffi.navi.FfiSearchHitKind

class ComapsTourismLookaheadTest {
    @Test
    fun tourismTagMapsMuseumAndViewpoint() {
        assertEquals("museum", ComapsSearchBridge.tourismTagFromFeatureType("Museum"))
        assertEquals("viewpoint", ComapsSearchBridge.tourismTagFromFeatureType("View point"))
        assertEquals("artwork", ComapsSearchBridge.tourismTagFromFeatureType("Artwork"))
        assertEquals("attraction", ComapsSearchBridge.tourismTagFromFeatureType("Castle"))
        assertEquals("attraction", ComapsSearchBridge.tourismTagFromFeatureType(""))
    }

    @Test
    fun tourismHitsBecomeTaggedJsonWithGeneralClassTags() {
        val hits =
            listOf(
                FfiSearchHit(
                    name = "Old Town",
                    address = "",
                    featureType = "Attraction",
                    lat = 60.39,
                    lon = 5.32,
                    distanceM = 120.0,
                    kind = FfiSearchHitKind.FEATURE,
                    source = "comaps",
                    mergedSources = emptyList(),
                    categoryKey = "category_tourism",
                    featureId = 42u,
                    mwmName = "Norway_Hordaland",
                ),
                FfiSearchHit(
                    name = "Harbour Museum",
                    address = "",
                    featureType = "Museum",
                    lat = 60.391,
                    lon = 5.321,
                    distanceM = 200.0,
                    kind = FfiSearchHitKind.FEATURE,
                    source = "comaps",
                    mergedSources = emptyList(),
                    categoryKey = "category_tourism",
                    featureId = null,
                    mwmName = null,
                ),
            )
        val raw = ComapsSearchBridge.tourismHitsToTaggedPoiJson(hits)
        val arr = JSONArray(raw)
        assertEquals(2, arr.length())
        assertEquals("attraction", arr.getJSONObject(0).getJSONObject("tags").getString("tourism"))
        assertEquals("museum", arr.getJSONObject(1).getJSONObject("tags").getString("tourism"))
        assertEquals(42L, arr.getJSONObject(0).getLong("osm_id"))
        assertTrue(arr.getJSONObject(1).getLong("osm_id") < 0L)
    }
}
