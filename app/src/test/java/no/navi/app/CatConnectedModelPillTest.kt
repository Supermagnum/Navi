package no.navi.app

import no.navi.app.cat.CatRigModelPreset
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CatConnectedModelPillTest {
    @Test
    fun matchesDummyByHamlibModelName() {
        val dummy = CatRigModelPreset(1, "Dummy (CI only)")
        assertTrue(catPresetMatchesConnectedModel(dummy, "Dummy"))
        assertFalse(catPresetMatchesConnectedModel(dummy, "Yaesu FT-891"))
    }

    @Test
    fun matchesYaesuByTokenInPresetName() {
        val ft891 = CatRigModelPreset(1036, "Yaesu FT-891")
        assertTrue(catPresetMatchesConnectedModel(ft891, "FT-891"))
        assertTrue(catPresetMatchesConnectedModel(ft891, "Yaesu FT-891"))
        assertFalse(catPresetMatchesConnectedModel(ft891, "Dummy"))
    }

    @Test
    fun blankModelNameNeverMatches() {
        val dummy = CatRigModelPreset(1, "Dummy (CI only)")
        assertFalse(catPresetMatchesConnectedModel(dummy, ""))
        assertFalse(catPresetMatchesConnectedModel(dummy, "   "))
    }

    @Test
    fun benshiRfcommLiveOnlyWhenConnectedRfcomm() {
        assertTrue(catBenshiRfcommLive(connected = true, link = CAT_LINK_BENSHI_RFCOMM))
        assertFalse(catBenshiRfcommLive(connected = false, link = CAT_LINK_BENSHI_RFCOMM))
        assertFalse(catBenshiRfcommLive(connected = true, link = CAT_LINK_BENSHI_LAB))
        assertFalse(catBenshiRfcommLive(connected = true, link = "tcp"))
        assertFalse(catBenshiRfcommLive(connected = true, link = "offline"))
        assertFalse(catBenshiRfcommLive(connected = false, link = ""))
    }
}
