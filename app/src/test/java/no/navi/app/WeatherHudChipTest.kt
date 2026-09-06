package no.navi.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Locale

/**
 * Timing labels for the GPS weather pill. Avoids `org.json` (Android unit-test
 * stubs throw "not mocked") — JSON wiring is covered on device / instrumented.
 */
class WeatherHudChipTest {
    @Test
    fun timingPrefersNextUpdateWhenInFuture() {
        val label =
            weatherHudTimingLabel(
                fetchedAtUnix = 1_700_000_000L,
                nextFetchUnix = 1_700_003_600L,
                nowUnix = 1_700_001_000L,
                locale = Locale.US,
            )
        assertTrue(label!!.startsWith("Next update "))
        assertFalse(label.contains("stale", ignoreCase = true))
        assertFalse(label.contains("throttled", ignoreCase = true))
        assertFalse(label.contains("offline", ignoreCase = true))
    }

    @Test
    fun timingFallsBackToUpdatedWhenNextIsPast() {
        val label =
            weatherHudTimingLabel(
                fetchedAtUnix = 1_700_000_000L,
                nextFetchUnix = 1_700_000_500L,
                nowUnix = 1_700_001_000L,
                locale = Locale.US,
            )
        assertTrue(label!!.startsWith("Updated "))
        assertFalse(label.contains("stale", ignoreCase = true))
        assertFalse(label.contains("throttled", ignoreCase = true))
        assertFalse(label.contains("offline", ignoreCase = true))
    }

    @Test
    fun timingNullWhenNoTimestamps() {
        assertNull(
            weatherHudTimingLabel(
                fetchedAtUnix = null,
                nextFetchUnix = null,
                nowUnix = 1_700_000_000L,
            ),
        )
    }
}
