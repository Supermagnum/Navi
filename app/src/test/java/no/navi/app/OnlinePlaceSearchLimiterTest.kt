package no.navi.app

import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class OnlinePlaceSearchDefaultOffTest {
    @Test
    fun onlinePlaceSearchDefaultEnabledIsFalse() {
        assertTrue(
            "ONLINE_PLACE_SEARCH_DEFAULT_ENABLED must remain false (opt-in)",
            !MapHudPrefs.ONLINE_PLACE_SEARCH_DEFAULT_ENABLED,
        )
        assertEquals(
            "https://nominatim.openstreetmap.org",
            MapHudPrefs.NOMINATIM_DEFAULT_BASE_URL,
        )
    }
}

class OnlinePlaceSearchGateTest {
    @Test
    fun settingOffSendsNothing() {
        assertTrue(
            !shouldSendOnlinePlaceSearch(
                OnlinePlaceSearchConfig(
                    enabled = false,
                    baseUrl = MapHudPrefs.NOMINATIM_DEFAULT_BASE_URL,
                    userAgent = naviOnlinePlaceSearchUserAgent("0.3.19-beta"),
                    explicitSubmit = true,
                ),
            ),
        )
    }

    @Test
    fun typingIsNotExplicitSubmit() {
        assertTrue(
            !shouldSendOnlinePlaceSearch(
                OnlinePlaceSearchConfig(
                    enabled = true,
                    baseUrl = MapHudPrefs.NOMINATIM_DEFAULT_BASE_URL,
                    userAgent = naviOnlinePlaceSearchUserAgent("0.3.19-beta"),
                    explicitSubmit = false,
                ),
            ),
        )
    }

    @Test
    fun emptyBaseUrlSendsNothing() {
        assertTrue(
            !shouldSendOnlinePlaceSearch(
                OnlinePlaceSearchConfig(
                    enabled = true,
                    baseUrl = "",
                    userAgent = naviOnlinePlaceSearchUserAgent("0.3.19-beta"),
                    explicitSubmit = true,
                ),
            ),
        )
        assertTrue(
            !shouldSendOnlinePlaceSearch(
                OnlinePlaceSearchConfig(
                    enabled = true,
                    baseUrl = "   ",
                    userAgent = naviOnlinePlaceSearchUserAgent("0.3.19-beta"),
                    explicitSubmit = true,
                ),
            ),
        )
    }

    @Test
    fun userAgentIdentifiesAppVersionAndProjectPage() {
        val ua = naviOnlinePlaceSearchUserAgent("0.3.19-beta")
        assertEquals("Navi/0.3.19-beta (https://github.com/Supermagnum/Navi)", ua)
        assertTrue(!ua.contains("okhttp", ignoreCase = true))
        assertTrue(!ua.contains("Java/", ignoreCase = true))
    }
}

class OnlinePlaceSearchLimiterTest {
    private val sampleJson =
        """
        [{"place_id":1,"osm_id":1,"lat":"60.79","lon":"11.07",
          "display_name":"Hamar, Innlandet, Norway",
          "class":"place","type":"town",
          "address":{"municipality":"Hamar","country_code":"no"}}]
        """.trimIndent()

    @Before
    fun reset() {
        OnlinePlaceSearch.resetOverridesForTests()
    }

    private fun config(
        enabled: Boolean = true,
        baseUrl: String = MapHudPrefs.NOMINATIM_DEFAULT_BASE_URL,
        explicitSubmit: Boolean = true,
        version: String = "0.3.19-beta",
    ) = OnlinePlaceSearchConfig(
        enabled = enabled,
        baseUrl = baseUrl,
        userAgent = naviOnlinePlaceSearchUserAgent(version),
        explicitSubmit = explicitSubmit,
    )

    private fun recordingClient(
        body: String = sampleJson,
        code: Int = 200,
        delayMs: Long = 0L,
    ): Pair<MutableList<OnlineHttpRequest>, OnlineHttpClient> {
        val calls = mutableListOf<OnlineHttpRequest>()
        val client =
            OnlineHttpClient { req ->
                synchronized(calls) { calls.add(req) }
                if (delayMs > 0L) delay(delayMs)
                if (code in 200..299) {
                    OnlineHttpResult.Ok(body)
                } else {
                    OnlineHttpResult.Failure(code, "http $code")
                }
            }
        return calls to client
    }

    @Test
    fun settingOffSendsNothing() =
        runBlocking {
            val (calls, client) = recordingClient()
            val hits =
                OnlinePlaceSearch.searchWithConfig(
                    "Hamar",
                    5,
                    config(enabled = false),
                    client,
                )
            assertTrue(hits.isEmpty())
            assertEquals(0, calls.size)
        }

    @Test
    fun typingSendsNothing() =
        runBlocking {
            val (calls, client) = recordingClient()
            val hits =
                OnlinePlaceSearch.searchWithConfig(
                    "Ha",
                    5,
                    config(explicitSubmit = false),
                    client,
                )
            assertTrue(hits.isEmpty())
            assertEquals(0, calls.size)
        }

    @Test
    fun explicitSearchSendsOneRequest() =
        runBlocking {
            val (calls, client) = recordingClient()
            val hits =
                OnlinePlaceSearch.searchWithConfig(
                    "Hamar",
                    5,
                    config(),
                    client,
                )
            assertEquals(1, hits.size)
            assertEquals("Hamar", hits[0].name)
            assertEquals(1, calls.size)
            assertTrue(calls[0].url.contains("nominatim.openstreetmap.org/search"))
            assertTrue(calls[0].url.contains("q=Hamar"))
        }

    @Test
    fun twoQuickSearchesSendAtMostOnePerSecond() =
        runBlocking {
            val clock =
                java.util.concurrent.atomic
                    .AtomicLong(10_000L)
            val waited = mutableListOf<Long>()
            val limiter =
                OnlinePlaceSearchLimiter(
                    minIntervalMs = 1_000L,
                    clockMs = { clock.get() },
                    wait = { ms ->
                        waited.add(ms)
                        clock.addAndGet(ms)
                    },
                )
            val (calls, client) = recordingClient()
            OnlinePlaceSearch.searchWithConfig("Hamar", 5, config(), client, limiter)
            OnlinePlaceSearch.searchWithConfig("Oslo", 5, config(), client, limiter)
            assertEquals(2, calls.size)
            assertTrue("second submit must wait the 1s gap, waits=$waited", waited.any { it >= 1_000L })
        }

    @Test
    fun overlappingSubmitReplacesPendingAndStaysAtOnePerSecond() =
        runBlocking {
            val limiter = OnlinePlaceSearchLimiter(minIntervalMs = 1_000L)
            val (seedCalls, seedClient) = recordingClient()
            OnlinePlaceSearch.searchWithConfig("Seed", 5, config(), seedClient, limiter)
            assertEquals(1, seedCalls.size)

            val calls = mutableListOf<OnlineHttpRequest>()
            val client =
                OnlineHttpClient { req ->
                    synchronized(calls) { calls.add(req) }
                    delay(30)
                    OnlineHttpResult.Ok("[]")
                }
            val a =
                async {
                    OnlinePlaceSearch.searchWithConfig("Alpha", 5, config(), client, limiter)
                }
            delay(5)
            val b =
                async {
                    OnlinePlaceSearch.searchWithConfig("Beta", 5, config(), client, limiter)
                }
            a.await()
            b.await()
            assertEquals(
                "replaced pending plus 1/s cap: only the later query should fire",
                1,
                calls.size,
            )
            assertTrue(calls[0].url.contains("q=Beta"))
        }

    @Test
    fun repeatedQueryIsServedFromCache() =
        runBlocking {
            val limiter = OnlinePlaceSearchLimiter()
            val (calls, client) = recordingClient()
            val first =
                OnlinePlaceSearch.searchWithConfig("Hamar", 5, config(), client, limiter)
            val second =
                OnlinePlaceSearch.searchWithConfig("Hamar", 5, config(), client, limiter)
            assertEquals(1, first.size)
            assertEquals(first[0].name, second[0].name)
            assertEquals(1, calls.size)
        }

    @Test
    fun userAgentIsTheAppsOwn() =
        runBlocking {
            val (calls, client) = recordingClient()
            OnlinePlaceSearch.searchWithConfig("Hamar", 5, config(), client)
            assertEquals(1, calls.size)
            assertEquals(
                "Navi/0.3.19-beta (https://github.com/Supermagnum/Navi)",
                calls[0].headers["User-Agent"],
            )
        }

    @Test
    fun emptyBaseUrlSendsNothing() =
        runBlocking {
            val (calls, client) = recordingClient()
            val hits =
                OnlinePlaceSearch.searchWithConfig(
                    "Hamar",
                    5,
                    config(baseUrl = ""),
                    client,
                )
            assertTrue(hits.isEmpty())
            assertEquals(0, calls.size)
        }

    @Test
    fun errorOrRateLimitReturnsEmptyWithoutRetry() =
        runBlocking {
            val (calls, client) = recordingClient(code = 429)
            val hits =
                OnlinePlaceSearch.searchWithConfig("Hamar", 5, config(), client)
            assertTrue(hits.isEmpty())
            assertEquals(1, calls.size)
        }
}
