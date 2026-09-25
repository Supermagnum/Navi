package no.navi.app

import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import kotlin.system.measureTimeMillis

/**
 * Typing cancels in-flight place search. Nominatim polite-use wait must use
 * cancellable [delay], not [Thread.sleep], or cancelled jobs stack ~1s each on
 * the IO pool and the search field feels frozen (double-letter typos).
 */
class OnlinePlaceSearchThrottleTest {
    @Before
    fun resetThrottle() {
        OnlinePlaceSearch.resetThrottleForTests()
        // Seed a recent request so the next throttle waits the full gap.
        runBlocking { OnlinePlaceSearch.throttleNominatim() }
    }

    @Test
    fun throttleNominatim_isCancelledQuickly() =
        runBlocking {
            val job =
                async {
                    OnlinePlaceSearch.throttleNominatim()
                }
            delay(50)
            val elapsed =
                measureTimeMillis {
                    job.cancelAndJoin()
                }
            assertTrue(
                "cancelled Nominatim throttle should abort in well under 1s, was ${elapsed}ms",
                elapsed < 400,
            )
        }
}
