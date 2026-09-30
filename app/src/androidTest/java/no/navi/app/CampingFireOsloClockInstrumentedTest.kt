package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.ZonedDateTime

/**
 * Item 4: fire-ban window uses Europe/Oslo local calendar (clock_read contract).
 *
 * HostApi `clock_read` is not yet wired through UniFFI (wasmtime gate closed);
 * this test verifies the same local-date contract the native embedder uses:
 * device timezone Europe/Oslo + local Y-M-D for the 15 Apr–15 Sep rule.
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
class CampingFireOsloClockInstrumentedTest {
    private val oslo: ZoneId = ZoneId.of("Europe/Oslo")

    private fun inFireBan(d: LocalDate): Boolean {
        val m = d.monthValue
        val day = d.dayOfMonth
        return when {
            m == 4 && day >= 15 -> true
            m in 5..8 -> true
            m == 9 && day <= 15 -> true
            else -> false
        }
    }

    @Test
    fun deviceTimezoneIsEuropeOslo() {
        val zid = ZoneId.systemDefault().id
        assertEquals(
            "emulator must run with Europe/Oslo for camping clock_read parity",
            "Europe/Oslo",
            zid,
        )
    }

    @Test
    fun utcLateApr14IsApr15LocalBan() {
        // 2026-04-14T23:30Z → 2026-04-15 01:30 in Europe/Oslo
        val utc = Instant.parse("2026-04-14T23:30:00Z")
        val local = ZonedDateTime.ofInstant(utc, oslo).toLocalDate()
        assertEquals(LocalDate.of(2026, 4, 15), local)
        assertTrue(inFireBan(local))
    }

    @Test
    fun utcLateSep15IsSep16LocalOutsideBan() {
        val utc = Instant.parse("2026-09-15T22:30:00Z")
        val local = ZonedDateTime.ofInstant(utc, oslo).toLocalDate()
        assertEquals(LocalDate.of(2026, 9, 16), local)
        assertFalse(inFireBan(local))
    }

    @Test
    fun deviceLocalClockMatchesOsloZone() {
        // Real device clock (after adb date set in the test runner script).
        val deviceLocal = LocalDate.now(ZoneId.systemDefault())
        val osloLocal = LocalDate.now(oslo)
        assertEquals(
            "system default zone must agree with Europe/Oslo on calendar date",
            osloLocal,
            deviceLocal,
        )
        // Log fire status for whatever date the emulator currently shows.
        val ban = inFireBan(deviceLocal)
        println(
            "clock_read contract: device_local=$deviceLocal in_fire_ban=$ban " +
                "(timezone=${ZoneId.systemDefault()})",
        )
    }
}
