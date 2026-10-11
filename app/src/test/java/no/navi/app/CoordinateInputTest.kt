package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.navi.PlaceHit

class CoordinateInputTest {
    @Test
    fun parsesCommaSeparatedLatLon() {
        val parsed = parseLatLonQuery("60.562480, 11.256282")
        assertEquals(60.562480, parsed!!.first, 1e-9)
        assertEquals(11.256282, parsed.second, 1e-9)
    }

    @Test
    fun parsesSpaceSeparatedLatLon() {
        val parsed = parseLatLonQuery("67.2804 14.4050")
        assertEquals(67.2804, parsed!!.first, 1e-9)
        assertEquals(14.4050, parsed.second, 1e-9)
    }

    @Test
    fun rejectsPlaceNames() {
        assertNull(parseLatLonQuery("Bodø"))
        assertNull(parseLatLonQuery("Oslo Sentralstasjon"))
    }

    @Test
    fun rejectsIntegersWithoutDecimal() {
        assertNull(parseLatLonQuery("60 11"))
    }

    @Test
    fun rejectsOutOfRange() {
        assertNull(parseLatLonQuery("91.0, 10.0"))
        assertNull(parseLatLonQuery("60.0, 181.0"))
        assertTrue(looksLikeCoordinatePair("1280060.393,5.324"))
        assertFalse(looksLikeCoordinatePair("Dombås"))
    }

    @Test
    fun foldSearchKeyCoversRequiredSpellings() {
        assertEquals(foldSearchKey("Dombås"), foldSearchKey("Dombas"))
        assertEquals(foldSearchKey("dombås"), foldSearchKey("DOMBÅS"))
        assertEquals(foldSearchKey("Åre"), foldSearchKey("Are"))
        assertEquals(foldSearchKey("Øra"), foldSearchKey("Ora"))
        assertEquals(foldSearchKey("København"), foldSearchKey("Kobenhavn"))
        assertEquals(foldSearchKey("Århus"), foldSearchKey("Arhus"))
        assertEquals(foldSearchKey("Łódź"), foldSearchKey("Lodz"))
        assertEquals(foldSearchKey("München"), foldSearchKey("Munchen"))
        assertEquals(foldSearchKey("Köln"), foldSearchKey("Koln"))
        assertEquals(foldSearchKey("Görlitz"), foldSearchKey("Gorlitz"))
        assertEquals(foldSearchKey("Świnoujście"), foldSearchKey("Swinoujscie"))
        assertEquals(foldSearchKey("Dombås"), foldSearchKey("Domba\u030as"))
        assertTrue(foldSearchKey("Little A'Le'Inn").contains("little"))
    }

    @Test
    fun selectingAResultIsMapOnly() {
        val hit =
            PlaceHit(
                1L,
                "Dombås",
                "place:village",
                62.07,
                9.12,
                "",
                "Dovre",
                "europe/norway/ostlandet",
            )
        val move = searchHitMapMove(hit)
        assertEquals(62.07, move.lat, 1e-6)
        assertEquals(9.12, move.lon, 1e-6)
        assertTrue(move.name.contains("Dombås"))
    }

    @Test
    fun noFillerWhenNameHasNoQueryTokens() {
        assertFalse(placeNameContainsQueryTokens("farm Gjøvik", "København"))
        assertTrue(placeNameContainsQueryTokens("København", "Kobenhavn"))
    }

    @Test
    fun onlineDanishTownsBeatLocalMinorPlaces() {
        fun farm(
            name: String,
            lat: Double,
            lon: Double,
            region: String,
            muni: String,
        ) = PlaceHit(1L, name, "place:farm", lat, lon, "", muni, region)

        fun onlineCity(
            name: String,
            lat: Double,
            lon: Double,
            region: String,
        ) = PlaceHit(2L, name, "online/place/city|dk", lat, lon, "", "", region)

        val kobenhavn =
            mergeOnlineAndOfflinePlaceHits(
                "København",
                listOf(onlineCity("København", 55.6867, 12.5701, "europe/denmark")),
                listOf(farm("København", 60.9603, 10.5479, "europe/norway/ostlandet", "Gjøvik")),
                63.43,
                10.40,
                "europe/norway/trondelag",
            )
        assertEquals("København", kobenhavn[0].name)
        assertTrue(kobenhavn[0].kind.startsWith("online/"))
        assertTrue(kobenhavn.any { it.kind == "place:farm" })

        val aarhus =
            mergeOnlineAndOfflinePlaceHits(
                "Århus",
                listOf(onlineCity("Aarhus", 56.1629, 10.2039, "europe/denmark")),
                listOf(farm("Århus", 59.67, 8.0, "europe/norway/ostlandet", "Vinje")),
                63.43,
                10.40,
                "europe/norway/trondelag",
            )
        assertTrue(aarhus[0].kind.startsWith("online/"))

        val rodby =
            mergeOnlineAndOfflinePlaceHits(
                "Rødby",
                listOf(PlaceHit(3L, "Rødby", "online/place/town|dk", 54.69, 11.39, "", "Lolland", "europe/denmark")),
                listOf(PlaceHit(4L, "Rødby", "place:hamlet", 59.83, 10.44, "", "Asker", "europe/norway/ostlandet")),
                63.43,
                10.40,
                "europe/norway/trondelag",
            )
        assertTrue(rodby[0].kind.startsWith("online/"))
        assertEquals("europe/denmark", rodby[0].regionId)
    }

    @Test
    fun localDombasAndAgaStayFirstWhenOnline() {
        val dombas =
            mergeOnlineAndOfflinePlaceHits(
                "Dombås",
                listOf(PlaceHit(10L, "Dombås", "online/place/village|no", 62.07, 9.12, "", "Dovre", "europe/norway/ostlandet")),
                listOf(PlaceHit(11L, "Dombås", "place:village", 62.075, 9.128, "", "Dovre", "europe/norway/ostlandet")),
                62.07,
                9.12,
                "europe/norway/ostlandet",
            )
        assertEquals("place:village", dombas[0].kind)

        val aga =
            mergeOnlineAndOfflinePlaceHits(
                "Aga",
                listOf(PlaceHit(12L, "Aga", "online/place/island|no", 59.03, 5.98, "", "", "europe/norway/vestlandet")),
                listOf(PlaceHit(13L, "Aga", "place:hamlet", 60.29870, 6.60322, "", "Ullensvang", "europe/norway/vestlandet")),
                60.39,
                6.50,
                "europe/norway/vestlandet",
            )
        assertEquals("europe/norway/vestlandet", aga[0].regionId)
        assertEquals("place:hamlet", aga[0].kind)
    }

    @Test
    fun areDoesNotRankFarmAboveOnlineAre() {
        val merged =
            mergeOnlineAndOfflinePlaceHits(
                "Are",
                listOf(PlaceHit(20L, "Åre", "online/place/town|se", 63.398, 13.081, "", "Åre", "europe/sweden/jamtland")),
                listOf(PlaceHit(21L, "Are", "place:farm", 61.0, 7.0, "", "", "europe/norway/vestlandet")),
                63.43,
                10.40,
                "europe/norway/trondelag",
            )
        assertEquals("Åre", merged[0].name)
        assertTrue(merged[0].kind.startsWith("online/"))
    }

    private val areFarm = PlaceHit(30L, "Åre", "place:farm", 59.37, 10.18, "", "Våje", "europe/norway/ostlandet")
    private val areHamlet = PlaceHit(31L, "Are", "place:hamlet", 59.36, 5.36, "", "Bokn", "europe/norway/vestlandet")

    private fun names(hits: List<PlaceHit>) = hits.map { it.name }

    @Test
    fun exactAsTypedNameComesBeforeFoldedName() {
        val ranked = rankPlaceHits("Åre", listOf(areHamlet, areFarm), 63.43, 10.40, "europe/norway/trondelag")
        assertEquals(listOf("Åre", "Are"), names(ranked))
        assertEquals(listOf("Åre", "Are"), names(rankPlaceHits("åre", listOf(areHamlet, areFarm))))
        assertEquals(0, placeMatchClass("Åre", "Åre"))
        assertEquals(1, placeMatchClass("Are", "Åre"))
        assertEquals(1, placeMatchClass("Åre", "Are"))
        assertEquals(1, placeMatchClass("Are", "Are"))
        assertEquals(2, placeMatchClass("Årestua", "Åre"))
        assertEquals(2, placeMatchClass("Kolnesholmane", "Köln"))
        assertEquals(3, placeMatchClass("Nedre Åre", "Åre"))
    }

    @Test
    fun plainQueryLetsKindAndDistanceDecideBetweenSpellings() {
        val ranked = rankPlaceHits("Are", listOf(areFarm, areHamlet), 63.43, 10.40, "europe/norway/trondelag")
        assertEquals(listOf("Are", "Åre"), names(ranked))
        val town = PlaceHit(32L, "Åre", "place:town", 63.398, 13.081, "", "Åre", "europe/sweden/jamtland")
        assertEquals("Åre", rankPlaceHits("Are", listOf(areHamlet, areFarm, town))[0].name)
        val nearFarm = areFarm.copy(lat = 63.40, lon = 10.30)
        val farFarm = areHamlet.copy(kind = "place:farm")
        assertEquals(
            "Våje",
            rankPlaceHits("Are", listOf(farFarm, nearFarm), 63.43, 10.40, "")[0].municipality,
        )
    }

    @Test
    fun prefixMatchNeverBeatsWholeNameMatch() {
        val city = PlaceHit(33L, "Årestua", "place:city", 63.0, 10.0, "", "", "europe/norway/trondelag")
        val ranked = rankPlaceHits("Åre", listOf(city, areHamlet, areFarm), 63.0, 10.0, "europe/norway/trondelag")
        assertEquals(listOf("Åre", "Are", "Årestua"), names(ranked))
    }

    @Test
    fun onlineSettlementBeatsLocalMinorFeatureEvenInTheVisibleRegion() {
        val rodby =
            mergeOnlineAndOfflinePlaceHits(
                "Rødby",
                listOf(PlaceHit(40L, "Rødby", "online/place/village|dk", 54.69, 11.39, "", "Lolland", "europe/denmark")),
                listOf(PlaceHit(41L, "Rødby", "highway:bus_stop", 59.83, 10.44, "", "Asker", "europe/norway/ostlandet")),
                59.83,
                10.44,
                "europe/norway/ostlandet",
            )
        assertEquals("europe/denmark", rodby[0].regionId)

        val are =
            mergeOnlineAndOfflinePlaceHits(
                "Åre",
                listOf(PlaceHit(42L, "Åre", "online/place/village|se", 63.398, 13.081, "", "Åre", "europe/sweden/jamtland")),
                listOf(areFarm, areHamlet),
                59.37,
                10.18,
                "europe/norway/ostlandet",
            )
        assertEquals(listOf("online/place/village|se", "place:farm", "place:hamlet"), are.map { it.kind })

        for (minor in listOf("route:ferry", "route:hiking:rwn", "shop:supermarket", "amenity:cafe", "building", "place:locality")) {
            val merged =
                mergeOnlineAndOfflinePlaceHits(
                    "Rødby",
                    listOf(PlaceHit(43L, "Rødby", "online/place/village|dk", 54.69, 11.39, "", "", "europe/denmark")),
                    listOf(PlaceHit(44L, "Rødby", minor, 59.83, 10.44, "", "Asker", "europe/norway/ostlandet")),
                    59.83,
                    10.44,
                    "europe/norway/ostlandet",
                )
            assertTrue("local $minor", merged[0].kind.startsWith("online/"))
        }
    }

    @Test
    fun onlineCopyOfALocalPlaceIsDroppedSoTheLocalHitRanks() {
        val localHamlet = PlaceHit(45L, "Rødby", "place:hamlet", 59.8301, 10.4402, "", "Asker", "europe/norway/ostlandet")
        val rodby =
            mergeOnlineAndOfflinePlaceHits(
                "Rødby",
                listOf(
                    PlaceHit(47L, "Rødby", "online/place/village|dk", 54.69, 11.39, "", "Lolland", "europe/denmark"),
                    PlaceHit(46L, "Rødby", "online/place/hamlet|no", 59.8303, 10.4405, "", "Asker", "europe/norway/ostlandet"),
                ),
                listOf(localHamlet),
                63.43,
                10.40,
                "europe/norway/trondelag",
            )
        assertEquals(listOf("online/place/village|dk", "place:hamlet"), rodby.map { it.kind })

        val dombas =
            mergeOnlineAndOfflinePlaceHits(
                "Dombås",
                listOf(PlaceHit(48L, "Dombås", "online/place/village|no", 62.0751, 9.1282, "", "Dovre", "europe/norway/ostlandet")),
                listOf(
                    PlaceHit(49L, "Dombås", "named", 62.08, 9.13, "", "Dovre", "europe/norway/ostlandet"),
                    PlaceHit(50L, "Dombås", "place:village", 62.075, 9.128, "", "Dovre", "europe/norway/ostlandet"),
                ),
                63.43,
                10.40,
                "europe/norway/trondelag",
            )
        assertEquals(listOf("place:village", "named"), dombas.map { it.kind })
    }

    @Test
    fun localHamletStaysFirstAgainstVillagesOfThatNameOnOtherContinents() {
        val ullensvang = PlaceHit(70L, "Aga", "place:hamlet", 60.2987, 6.6032, "", "Ullensvang", "europe/norway/vestlandet")
        val busStop = PlaceHit(71L, "Aga", "highway:bus_stop", 60.31, 6.61, "", "Ullensvang", "europe/norway/vestlandet")
        val merged =
            mergeOnlineAndOfflinePlaceHits(
                "Aga",
                listOf(
                    PlaceHit(74L, "Aga", "online/place/village|bj", 7.1, 2.2, "", "Zou", "africa/benin"),
                    PlaceHit(75L, "Aga", "online/place/village|ph", 18.0, 121.4, "", "Pudtol", ""),
                    PlaceHit(72L, "Aga", "online/place/hamlet|no", 60.2988, 6.6034, "", "Ullensvang", "europe/norway/vestlandet"),
                    PlaceHit(73L, "Aga", "online/place/island|no", 59.03, 5.98, "", "Bømlo", "europe/norway/vestlandet"),
                ),
                listOf(busStop, ullensvang),
                63.43,
                10.40,
                "europe/norway/trondelag",
            )
        assertEquals(ullensvang, merged[0])
        assertEquals(busStop, merged[1])
        assertTrue(merged.drop(2).all { it.kind.startsWith("online/") })
    }

    @Test
    fun localSettlementOfTheSameNameStaysFirstAgainstOnlineCity() {
        for (localKind in listOf("place:village", "place:town", "place:city")) {
            val merged =
                mergeOnlineAndOfflinePlaceHits(
                    "Rødby",
                    listOf(PlaceHit(50L, "Rødby", "online/place/city|dk", 54.69, 11.39, "", "", "europe/denmark")),
                    listOf(PlaceHit(51L, "Rødby", localKind, 59.83, 10.44, "", "Asker", "europe/norway/ostlandet")),
                    63.43,
                    10.40,
                    "europe/norway/trondelag",
                )
            assertEquals("local $localKind", localKind, merged[0].kind)
        }
    }

    @Test
    fun wholeQueryMatchIsFoldedAndWordBased() {
        val koln = PlaceHit(60L, "Kolnesholmane", "place:islet", 63.5, 10.0, "", "Frøya", "europe/norway/trondelag")
        assertFalse(placeHitMatchesWholeQuery(koln, "Köln"))
        val kbh = PlaceHit(61L, "København", "place:farm", 60.96, 10.55, "", "Gjøvik", "europe/norway/ostlandet")
        assertTrue(placeHitMatchesWholeQuery(kbh, "Kobenhavn"))
        assertTrue(placeHitMatchesWholeQuery(kbh, "København, Gjøvik"))
        val muenchen = PlaceHit(62L, "München", "place:city", 48.14, 11.58, "", "", "europe/germany/bayern")
        assertTrue(placeHitMatchesWholeQuery(muenchen, "Muenchen"))
    }
}
