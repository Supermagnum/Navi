package no.navi.app

import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.util.concurrent.atomic.AtomicInteger

class DatexSessionRefreshTest {
    @Before
    fun setUp() {
        DatexSessionRefresh.resetForTests()
    }

    @After
    fun tearDown() {
        DatexSessionRefresh.resetForTests()
    }

    @Test
    fun pluginOffSkipsWithoutRefresh() =
        runBlocking {
            val calls = AtomicInteger(0)
            DatexSessionRefresh.refreshOverride = {
                calls.incrementAndGet()
                """{"overlay_enabled":false,"warning":"should_not_run"}"""
            }
            val outcome =
                DatexSessionRefresh.ensureBeforeFirstPlan(
                    pluginEnabled = false,
                    appliesToProfile = true,
                    host = "example.test",
                    port = 80u,
                    routeLatLonJson = "[[60.5,11.2],[61.0,10.9]]",
                    wifiOnly = false,
                    onWifi = true,
                    cacheDir = null,
                )
            assertEquals(DatexSessionRefresh.PrePlanOutcome.SkippedPluginOff, outcome)
            assertEquals(0, calls.get())
            assertFalse(DatexSessionRefresh.firstPlanWarmupCompleted())
        }

    @Test
    fun hikingProfileSkipsWithoutRefresh() =
        runBlocking {
            val calls = AtomicInteger(0)
            DatexSessionRefresh.refreshOverride = {
                calls.incrementAndGet()
                "{}"
            }
            val outcome =
                DatexSessionRefresh.ensureBeforeFirstPlan(
                    pluginEnabled = true,
                    appliesToProfile = false,
                    host = "example.test",
                    port = 80u,
                    routeLatLonJson = "[[60.5,11.2],[61.0,10.9]]",
                    wifiOnly = false,
                    onWifi = true,
                    cacheDir = null,
                )
            assertEquals(DatexSessionRefresh.PrePlanOutcome.SkippedNotApplicable, outcome)
            assertEquals(0, calls.get())
            assertFalse(DatexSessionRefresh.firstPlanWarmupCompleted())
        }

    @Test
    fun firstPlanWaitsForRefreshWithinTimeout() =
        runBlocking {
            val calls = AtomicInteger(0)
            DatexSessionRefresh.refreshOverride = {
                calls.incrementAndGet()
                delay(50)
                """{"overlay_enabled":true,"data_source":"server-duckdns","active":[],"inactive":[]}"""
            }
            var waiting = false
            val outcome =
                DatexSessionRefresh.ensureBeforeFirstPlan(
                    pluginEnabled = true,
                    appliesToProfile = true,
                    host = "example.test",
                    port = 80u,
                    routeLatLonJson = "[[60.5,11.2],[61.0,10.9]]",
                    wifiOnly = false,
                    onWifi = true,
                    cacheDir = null,
                    onWaiting = { waiting = true },
                )
            assertTrue(waiting)
            assertTrue(outcome is DatexSessionRefresh.PrePlanOutcome.Refreshed)
            assertEquals(1, calls.get())
            assertTrue(DatexSessionRefresh.firstPlanWarmupCompleted())

            val second =
                DatexSessionRefresh.ensureBeforeFirstPlan(
                    pluginEnabled = true,
                    appliesToProfile = true,
                    host = "example.test",
                    port = 80u,
                    routeLatLonJson = "[[60.5,11.2],[61.0,10.9]]",
                    wifiOnly = false,
                    onWifi = true,
                    cacheDir = null,
                )
            assertEquals(DatexSessionRefresh.PrePlanOutcome.SkippedAlreadyDone, second)
            assertEquals(1, calls.get())
        }

    @Test
    fun slowNetworkRespectsTimeoutAndDoesNotHang() =
        runBlocking {
            val calls = AtomicInteger(0)
            DatexSessionRefresh.refreshOverride = {
                calls.incrementAndGet()
                delay(DatexSessionRefresh.PRE_PLAN_TIMEOUT_MS + 2_000L)
                """{"overlay_enabled":true,"data_source":"late"}"""
            }
            val t0 = System.currentTimeMillis()
            val outcome =
                DatexSessionRefresh.ensureBeforeFirstPlan(
                    pluginEnabled = true,
                    appliesToProfile = true,
                    host = "example.test",
                    port = 80u,
                    routeLatLonJson = "[[60.5,11.2],[61.0,10.9]]",
                    wifiOnly = false,
                    onWifi = true,
                    cacheDir = null,
                )
            val elapsed = System.currentTimeMillis() - t0
            assertEquals(DatexSessionRefresh.PrePlanOutcome.TimedOut, outcome)
            assertTrue(
                "timeout should fire near ${DatexSessionRefresh.PRE_PLAN_TIMEOUT_MS}ms, got ${elapsed}ms",
                elapsed < DatexSessionRefresh.PRE_PLAN_TIMEOUT_MS + 1_500L,
            )
            assertTrue(DatexSessionRefresh.firstPlanWarmupCompleted())
            assertEquals(1, calls.get())
        }

    @Test
    fun concurrentRefreshSharedIsSingleFlight() =
        runBlocking {
            val calls = AtomicInteger(0)
            DatexSessionRefresh.refreshOverride = {
                calls.incrementAndGet()
                delay(200)
                """{"overlay_enabled":true,"seq":${calls.get()}}"""
            }
            val a =
                async {
                    DatexSessionRefresh.refreshShared(
                        enabled = true,
                        host = "example.test",
                        port = 80u,
                        routeLatLonJson = "[[1.0,2.0],[3.0,4.0]]",
                        wifiOnly = false,
                        onWifi = true,
                        cacheDir = null,
                    )
                }
            val b =
                async {
                    DatexSessionRefresh.refreshShared(
                        enabled = true,
                        host = "example.test",
                        port = 80u,
                        routeLatLonJson = "[[1.0,2.0],[3.0,4.0]]",
                        wifiOnly = false,
                        onWifi = true,
                        cacheDir = null,
                    )
                }
            val ra = a.await()
            val rb = b.await()
            assertEquals(ra, rb)
            assertEquals(1, calls.get())
        }
}
