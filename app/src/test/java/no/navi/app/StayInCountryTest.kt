package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class StayInCountryTest {
    @Test
    fun allowedCountries_off_isNull() {
        assertNull(StayInCountry.allowedCountriesForPlan(false, "no"))
        assertNull(StayInCountry.allowedCountriesForPlan(false, null))
    }

    @Test
    fun allowedCountries_on_drammenStyle_isNorway() {
        // Drammen→Kautokeino: origin resolves to NO via Natural Earth.
        assertEquals(listOf("no"), StayInCountry.allowedCountriesForPlan(true, "NO"))
        assertEquals(listOf("no"), StayInCountry.allowedCountriesForPlan(true, " no "))
    }

    @Test
    fun allowedCountries_on_withoutIso_isNull() {
        assertNull(StayInCountry.allowedCountriesForPlan(true, null))
        assertNull(StayInCountry.allowedCountriesForPlan(true, ""))
        assertNull(StayInCountry.allowedCountriesForPlan(true, "nor"))
    }

    @Test
    fun noRouteMessage_usesCountryName() {
        assertEquals(
            "No route found that stays within Norway with Stay in Country on. " +
                "Try turning it off, or add a via point.",
            StayInCountry.noRouteMessage("Norway"),
        )
    }

    @Test
    fun countryLabelFromIso_norway() {
        assertEquals("Norway", StayInCountry.countryLabelFromIso("no"))
        assertEquals("the starting country", StayInCountry.countryLabelFromIso(null))
    }

    @Test
    fun copy_matchesProductStrings() {
        assertEquals("Stay in Country", StayInCountry.LABEL)
        assertEquals(
            "Avoid crossing international borders, even if a foreign route is faster.",
            StayInCountry.SHORT_DESCRIPTION,
        )
        assertEquals(
            "When on, Navi only routes through roads inside your starting country.\n" +
                "The route may be longer or slower, but never crosses a border - useful\n" +
                "when carrying pets, plants, or goods that need customs paperwork or are\n" +
                "subject to quarantine rules in a neighboring country.\n" +
                "Example: Drammen to Kautokeino normally routes through Sweden and\n" +
                "Finland, which is faster. With Stay in Country on, the route stays\n" +
                "entirely within Norway.",
            StayInCountry.DETAILS,
        )
    }
}
