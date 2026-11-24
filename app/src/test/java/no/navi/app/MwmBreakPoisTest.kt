package no.navi.app

import org.json.JSONArray
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.navi.FfiSearchHit
import uniffi.navi.FfiSearchHitKind

class MwmBreakPoisTest {
    @Test
    fun classifiesHikingHutShelterPicnicAndIgnoresHotel() {
        assertEquals("hut" to "cabin", MwmBreakPois.classifyRestFeature("Alpine hut", hiking = true))
        assertEquals("hut" to "cabin", MwmBreakPois.classifyRestFeature("tourism-wilderness_hut", hiking = true))
        assertEquals("shelter" to "shelter", MwmBreakPois.classifyRestFeature("Amenity Shelter", hiking = true))
        assertEquals("shelter" to "shelter", MwmBreakPois.classifyRestFeature("Gapahuk", hiking = true))
        assertEquals("tent" to "tourism-camp_site", MwmBreakPois.classifyRestFeature("Camp site", hiking = true))
        assertEquals("picnic" to "tourism-picnic_site", MwmBreakPois.classifyRestFeature("Picnic site", hiking = true))
        assertNull(MwmBreakPois.classifyRestFeature("Hotel", hiking = true))
        assertNull(MwmBreakPois.classifyRestFeature("Museum", hiking = true))
    }

    @Test
    fun classifiesMotorLodgingAndRestArea() {
        assertEquals("lodging" to "tourism-hotel", MwmBreakPois.classifyRestFeature("Hotel", hiking = false))
        assertEquals("rest_area" to "highway-rest_area", MwmBreakPois.classifyRestFeature("Rest area", hiking = false))
        assertEquals("rest_area" to "highway-rest_area", MwmBreakPois.classifyRestFeature("Parking", hiking = false))
        assertEquals("hut" to "cabin", MwmBreakPois.classifyRestFeature("Alpine hut", hiking = false))
    }

    @Test
    fun samplesHikingIntervalMarksAlongPolyline() {
        // ~45 km northbound line (0.001 deg lat ≈ 111 m → 0.4 deg ≈ 44.4 km).
        val pts =
            (0..40).joinToString(";") { i ->
                String.format(java.util.Locale.US, "11.0,%.5f", 60.0 + i * 0.01)
            }
        val marks = MwmBreakPois.sampleMarks(pts, MwmBreakPois.HIKING_INTERVAL_KM)
        assertTrue("expected several rast marks, got ${marks.size}", marks.size >= 3)
        assertEquals(MwmBreakPois.HIKING_INTERVAL_KM, marks.first().alongKm, 0.05)
        for (i in 1 until marks.size) {
            assertEquals(
                MwmBreakPois.HIKING_INTERVAL_KM,
                marks[i].alongKm - marks[i - 1].alongKm,
                0.05,
            )
        }
    }

    @Test
    fun assignNearestPicksWithinRadiusAndDedups() {
        val marks =
            listOf(
                MwmBreakPois.Mark(60.10, 11.0, 11.3),
                MwmBreakPois.Mark(60.20, 11.0, 22.6),
                MwmBreakPois.Mark(60.30, 11.0, 33.9),
            )
        val candidates =
            listOf(
                MwmBreakPois.Candidate("Hut A", 60.101, 11.001, "hut", "cabin"),
                MwmBreakPois.Candidate("Hut B", 60.201, 11.001, "hut", "cabin"),
                // Same place as Hut A (within dedup) — should not win a later mark alone after use.
                MwmBreakPois.Candidate("Hut A twin", 60.1015, 11.0012, "hut", "cabin"),
                MwmBreakPois.Candidate("Far", 61.0, 12.0, "hut", "cabin"),
            )
        val stops = MwmBreakPois.assignNearest(marks, candidates, searchRadiusM = 5_000.0)
        assertEquals(2, stops.size)
        assertEquals("Hut A", stops[0].name)
        assertEquals(11.3, stops[0].alongKm, 0.01)
        assertEquals("Hut B", stops[1].name)
        assertEquals(22.6, stops[1].alongKm, 0.01)
    }

    @Test
    fun breakPoisJsonFeedsRestPlaceNames() {
        val stops =
            listOf(
                MwmBreakPois.AlongStop("Gapahuk Langtjennet", 61.1, 10.5, "shelter", "shelter", 11.3),
                MwmBreakPois.AlongStop("Rondvassbu", 61.88, 9.79, "hut", "cabin", 120.0),
            )
        val json = MwmBreakPois.toBreakPoisJson(stops)
        val arr = JSONArray(json)
        assertEquals(2, arr.length())
        assertEquals("Gapahuk Langtjennet", arr.getJSONObject(0).getString("name"))
        assertEquals(61.1, arr.getJSONObject(0).getDouble("lat"), 1e-6)
        assertEquals(10.5, arr.getJSONObject(0).getDouble("lon"), 1e-6)
        val names = restPlaceNamesFromJson(json)
        assertEquals(listOf("Gapahuk Langtjennet", "Rondvassbu"), names)
        val stats = routePlanStatsFromPlan("PASS\n", json)
        assertEquals(2, stats.restPlaceCount)
        assertEquals(names, stats.restPlaceNames)
    }

    @Test
    fun reportTokensIncludeRestPlaceCount() {
        val sb = StringBuilder("PASS\n")
        MwmBreakPois.appendReportTokens(
            sb,
            MwmBreakPois.BuildResult(
                breakPoisJson = """[{"name":"Hut A"}]""",
                restPlaceCount = 3,
                candidateCount = 12,
                segmentQueries = 5,
            ),
        )
        val report = sb.toString()
        assertTrue(report.contains("rest_place_count=3"))
        assertTrue(report.contains("mwm_break_pois=true"))
        assertTrue(report.contains("mwm_break_candidates=12"))
        val stats = routePlanStatsFromPlan(report, """[{"name":"Hut A"},{"name":"Hut B"},{"name":"Hut C"}]""")
        assertEquals(3, stats.restPlaceCount)
    }

    @Test
    fun candidateFromHitFiltersNonRestTypes() {
        fun hit(
            name: String,
            featureType: String,
            id: UInt,
        ) = FfiSearchHit(
            name = name,
            address = "",
            featureType = featureType,
            lat = 61.88,
            lon = 9.79,
            distanceM = 0.0,
            kind = FfiSearchHitKind.FEATURE,
            source = "comaps",
            mergedSources = emptyList(),
            categoryKey = null,
            featureId = id,
            mwmName = "Norway_Oppland",
        )
        val c = MwmBreakPois.candidateFromHit(hit("Rondvassbu", "Alpine hut", 1u), hiking = true)
        assertEquals("Rondvassbu", c!!.name)
        assertEquals("hut", c.kind)
        assertNull(MwmBreakPois.candidateFromHit(hit("Scandic", "Hotel", 2u), hiking = true))
        val byName =
            MwmBreakPois.candidateFromHit(hit("Gapahuk Langtjennet", "Point", 3u), hiking = true)
        assertEquals("shelter", byName!!.kind)
    }
}
