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
                listOf(PlaceHit(12L, "Aga", "online/place/village", 57.04, 12.54, "", "", "europe/sweden/halland")),
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
}
