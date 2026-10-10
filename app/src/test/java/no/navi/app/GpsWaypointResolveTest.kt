package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.navi.PlaceHit

class GpsWaypointResolveTest {
    @Test
    fun prefersAddressWithinHits() {
        val hits =
            listOf(
                PlaceHit(1L, "Finstad", "highway:bus_stop", 60.0, 11.0, "", "", ""),
                PlaceHit(2L, "Ådalsbrukvegen 134", "addr:housenumber", 60.0, 11.0, "", "", ""),
            )
        assertEquals(
            "Ådalsbrukvegen 134",
            pickNearbyPlaceNameForGpsWaypoint(hits),
        )
    }

    @Test
    fun usesNearestNameWhenNoAddress() {
        val hits =
            listOf(
                PlaceHit(1L, "Finstad", "highway:bus_stop", 60.80573, 11.32984, "", "", ""),
            )
        assertEquals("Finstad", pickNearbyPlaceNameForGpsWaypoint(hits))
    }

    @Test
    fun nullWhenEmptyOrBlank() {
        assertNull(pickNearbyPlaceNameForGpsWaypoint(emptyList()))
        assertNull(
            pickNearbyPlaceNameForGpsWaypoint(
                listOf(PlaceHit(1L, "  ", "named", 60.0, 11.0, "", "", "")),
            ),
        )
    }

    @Test
    fun placeHitDisplayLabelJoinsContextAndSkipsDuplicates() {
        assertEquals(
            "Båberg, Brattberg, Gjøvik",
            placeHitDisplayLabel(
                PlaceHit(1L, "Båberg", "place:farm", 60.97, 10.55, "Brattberg", "Gjøvik", ""),
            ),
        )
        assertEquals(
            "Espa, Stange",
            placeHitDisplayLabel(
                PlaceHit(2L, "Espa", "place:village", 60.58, 11.27, "", "Stange", ""),
            ),
        )
        assertEquals(
            "Gjøvik",
            placeHitDisplayLabel(
                PlaceHit(3L, "Gjøvik", "place:town", 60.80, 10.69, "", "Gjøvik", ""),
            ),
        )
    }

    @Test
    fun fallbackFormat() {
        assertEquals(
            "GPS (60.80573, 11.32984)",
            formatGpsWaypointFallback(60.80573, 11.32984),
        )
    }

    @Test
    fun snapsPinOntoSegmentWithinTwelveMetres() {
        val pts =
            listOf(
                60.0 to 10.0,
                60.0 to 10.001,
            )
        val midLon = 10.0005
        val placeLat = 60.0 + (5.0 / 111_320.0)
        val snap = snapWaypointToRoutePolyline(pts, placeLat, midLon)
        assertNotNull(snap)
        assertTrue(snap!!.distM <= WAYPOINT_ROUTE_PIN_MAX_M)
        assertEquals(60.0, snap.lat, 1e-6)
    }

    @Test
    fun reportsDistanceWhenPlaceFarFromCorridor() {
        val pts = listOf(60.0 to 10.0, 60.0 to 10.001)
        val farLat = 60.0 + (40.0 / 111_320.0)
        val snap = snapWaypointToRoutePolyline(pts, farLat, 10.0005)
        assertNotNull(snap)
        assertTrue(snap!!.distM > WAYPOINT_ROUTE_PIN_MAX_M)
    }
}

class PlaceSearchMergeTest {
    @Test
    fun kalmarTokensRejectKalmargatenPrefix() {
        assertTrue(placeNameContainsQueryTokens("Kalmar, Kalmar kommun", "Kalmar"))
        assertTrue(!placeNameContainsQueryTokens("Kalmargaten barnehage", "Kalmar"))
        assertTrue(!placeNameContainsQueryTokens("Kalmargaten 2, Engen, Bergen", "Kalmar"))
    }

    @Test
    fun mergeDropsBergenFtsWhenOnlineHasKalmarSweden() {
        val online =
            listOf(
                PlaceHit(
                    1L,
                    "Kalmar",
                    "online/place/city",
                    56.6628826,
                    16.3662382,
                    "",
                    "Kalmar kommun",
                    "europe/sweden/kalmar",
                ),
            )
        val offline =
            listOf(
                PlaceHit(
                    2L,
                    "Kalmargaten barnehage",
                    "amenity:kindergarten",
                    60.39,
                    5.32,
                    "Engen",
                    "Bergen",
                    "europe/norway/vestlandet",
                ),
                PlaceHit(
                    3L,
                    "Kalmarhuset",
                    "building:yes",
                    60.39,
                    5.33,
                    "Jonsvollen",
                    "Bergen",
                    "europe/norway/vestlandet",
                ),
            )
        val merged = mergeOnlineAndOfflinePlaceHits("Kalmar", online, offline)
        assertEquals(1, merged.size)
        assertEquals("Kalmar", merged[0].name)
        assertTrue(merged.none { it.municipality.contains("Bergen", ignoreCase = true) })
        assertTrue(merged.none { it.name.contains("Kalmargaten", ignoreCase = true) })
    }

    @Test
    fun mergeKeepsOfflineWholeTokenMatch() {
        val online =
            listOf(
                PlaceHit(1L, "Hamar", "online/place/city", 60.79, 11.07, "", "Hamar", ""),
            )
        val offline =
            listOf(
                PlaceHit(2L, "Hamar stasjon", "railway:station", 60.79, 11.08, "", "Hamar", ""),
            )
        // "Hamar" is a whole token in "Hamar stasjon"
        val merged = mergeOnlineAndOfflinePlaceHits("Hamar", online, offline)
        assertEquals(2, merged.size)
    }

    @Test
    fun agaRanksUllensvangAheadOfHallandWhenMapIsOverVestlandet() {
        val hits =
            listOf(
                PlaceHit(
                    2L,
                    "Agardh",
                    "place:hamlet",
                    56.67,
                    12.86,
                    "",
                    "",
                    "europe/sweden/halland",
                ),
                PlaceHit(
                    1L,
                    "Aga",
                    "place:hamlet",
                    60.30,
                    6.60,
                    "",
                    "Ullensvang",
                    "europe/norway/vestlandet",
                ),
            )
        val ranked =
            rankPlaceHits(
                "Aga",
                hits,
                60.39,
                6.50,
                "europe/norway/vestlandet",
            )
        assertEquals("Aga", ranked[0].name)
        assertEquals("europe/norway/vestlandet", ranked[0].regionId)
    }

    @Test
    fun localExactBeatsOnlineExactWhenMapIsOverHardanger() {
        val online =
            PlaceHit(
                10L,
                "Aga",
                "online/place/village",
                57.04,
                12.54,
                "",
                "",
                "europe/sweden/halland",
            )
        val local =
            PlaceHit(
                11L,
                "Aga",
                "place:hamlet",
                60.29870,
                6.60322,
                "",
                "Ullensvang",
                "europe/norway/vestlandet",
            )
        val merged =
            mergeOnlineAndOfflinePlaceHits(
                "Aga",
                listOf(online),
                listOf(local),
                60.39,
                6.50,
                "europe/norway/vestlandet",
            )
        assertEquals("Aga", merged[0].name)
        assertEquals("europe/norway/vestlandet", merged[0].regionId)
        assertEquals("Ullensvang", merged[0].municipality)
        assertTrue(merged[0].kind.startsWith("place:"))
    }

    @Test
    fun expectedPlacesRankFirst() {
        fun hit(
            name: String,
            kind: String,
            lat: Double,
            lon: Double,
            region: String,
            muni: String = "",
        ) = PlaceHit(name.hashCode().toLong(), name, kind, lat, lon, "", muni, region)

        val cases =
            listOf(
                "Hamar" to
                    listOf(
                        hit("Hamar", "place:town", 60.79, 11.07, "europe/norway/ostlandet", "Hamar"),
                        hit("Hamarvegen", "highway:residential", 60.80, 11.10, "europe/norway/ostlandet"),
                    ),
                "Bergen" to
                    listOf(
                        hit("Bergen", "place:city", 60.39, 5.32, "europe/norway/vestlandet", "Bergen"),
                        hit("Bergenhus", "place:suburb", 60.40, 5.32, "europe/norway/vestlandet"),
                    ),
                "Raufoss" to
                    listOf(
                        hit("Raufoss", "place:town", 60.73, 10.61, "europe/norway/ostlandet"),
                        hit("Raufossvegen", "highway:tertiary", 60.73, 10.62, "europe/norway/ostlandet"),
                    ),
                "Utne" to
                    listOf(
                        hit("Utne", "place:village", 60.42, 6.62, "europe/norway/vestlandet"),
                        hit("Utne kyrkje", "amenity:place_of_worship", 60.42, 6.63, "europe/norway/vestlandet"),
                    ),
            )
        for ((q, hits) in cases) {
            assertEquals(q, rankPlaceHits(q, hits, hits[0].lat, hits[0].lon, hits[0].regionId)[0].name)
        }
    }

    @Test
    fun prefixOnlyQueryStillFindsResults() {
        val hits =
            listOf(
                PlaceHit(1L, "Raufoss", "place:town", 60.73, 10.61, "", "", "europe/norway/ostlandet"),
            )
        val ranked = rankPlaceHits("Raufo", hits, 60.73, 10.61, "europe/norway/ostlandet")
        assertEquals("Raufoss", ranked[0].name)
    }
}
