package no.navi.app

import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.system.measureTimeMillis

/**
 * A replaced / cancelled limiter wait must abort instead of blocking the
 * caller for the full one-second gap.
 */
class OnlinePlaceSearchThrottleTest {
    @Test
    fun limiterWaitIsCancelledQuickly() =
        runBlocking {
            val limiter =
                OnlinePlaceSearchLimiter(
                    minIntervalMs = 1_000L,
                    clockMs = { 10_000L },
                    wait = { delay(it) },
                )
            val client =
                OnlineHttpClient {
                    OnlineHttpResult.Ok("[]")
                }
            // Seed lastStartMs so the next get() waits the full gap.
            limiter.get(
                "seed",
                OnlineHttpRequest("https://example.test/seed", emptyMap()),
                client,
            )
            val job =
                async {
                    limiter.get(
                        "next",
                        OnlineHttpRequest("https://example.test/next", emptyMap()),
                        client,
                    )
                }
            delay(40)
            val elapsed =
                measureTimeMillis {
                    job.cancelAndJoin()
                }
            assertTrue(
                "cancelled limiter wait should abort in well under 1s, was ${elapsed}ms",
                elapsed < 400,
            )
        }
}
