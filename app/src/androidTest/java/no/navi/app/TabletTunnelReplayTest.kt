package no.navi.app

import android.content.Context
import android.location.Location
import android.location.LocationManager
import android.location.provider.ProviderProperties
import android.os.Build
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import org.junit.After
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.CorridorRouteResult
import kotlin.math.sin

/**
 * On-device replay of the simulated tunnel cases. Geometry and the trace are
 * generated here and are not stored. GNSS is injected as samples (and, when
 * the platform allows, as a mock location). IMU is injected as sensor frames.
 * There is no real drive.
 */
@RunWith(AndroidJUnit4::class)
class TabletTunnelReplayTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    @Before
    fun resetHookObservation() {
        NaviMapTestHooks.lastRoutePolyline = ""
    }

    @After
    fun removeReplayProvider() {
        NaviMapTestHooks.ignoreLiveGpsFixes = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.deadReckoningSuspendHardware?.invoke(false)
        val lm =
            InstrumentationRegistry
                .getInstrumentation()
                .targetContext
                .getSystemService(LocationManager::class.java)
                ?: return
        runCatching { lm.removeTestProvider(PROVIDER) }
    }

    @Test
    fun clearReplayTestProvider() {
        val lm =
            InstrumentationRegistry
                .getInstrumentation()
                .targetContext
                .getSystemService(LocationManager::class.java)
        assertNotNull(lm)
        runCatching { lm!!.removeTestProvider(PROVIDER) }
        assertFalse(lm!!.allProviders.contains(PROVIDER))
    }

    @Test
    fun steady50_steady80_andCrawl_stayWithin50m() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        context
            .getSharedPreferences("navi_map_hud", Context.MODE_PRIVATE)
            .edit()
            .putBoolean("location_diagnostics", true)
            .commit()
        NaviMapTestHooks.ignoreLiveGpsFixes = true
        NaviMapTestHooks.hideSearchChrome = true
        val samples = straightTunnel()
        val route = routeOf(samples)
        activityRule.launchActivity(null)
        // onCreate clears this flag. Set it again once the activity is up so
        // the tablet's live fix cannot snap the estimate onto the route.
        NaviMapTestHooks.ignoreLiveGpsFixes = true
        waitUntil(20_000) { NaviMapTestHooks.deadReckoningInject != null }
        NaviMapTestHooks.pendingRoute = route
        waitUntil(20_000) { NaviMapTestHooks.lastRoutePolyline.isNotEmpty() }
        NaviMapTestHooks.deadReckoningSuspendHardware?.invoke(true)
        val mock = tryMockProvider()
        val report = StringBuilder()
        report.appendLine("mock_location=$mock")
        for (case in listOf("steady50", "steady80", "crawl")) {
            val truth = profile(samples, case)
            val result = replay(samples, truth, pushMock = mock)
            report.appendLine(
                "$case along=${fmt(result.alongErr)} cross=${fmt(result.crossErr)} " +
                    "exit=${fmt(result.exitM)} mode=${result.mode} speed=${fmt(result.speedMps)}",
            )
            android.util.Log.i("TabletTunnelReplay", report.toString())
            println(report.toString())
        }
        android.util.Log.i("TabletTunnelReplay", report.toString())
        println(report.toString())
        for (line in report.lines()) {
            if (!line.contains("exit=")) continue
            val exit = line.substringAfter("exit=").substringBefore(" ").toDoubleOrNull()
            assertTrue(line, exit != null && exit <= 50.0)
        }
    }

    private data class Step(
        val tMs: Long,
        val alongM: Double,
        val speedMps: Double,
        val accel: Double,
        val inTunnel: Boolean,
        val lat: Double,
        val lon: Double,
    )

    private data class Replay(
        val alongErr: Double?,
        val crossErr: Double?,
        val exitM: Double?,
        val mode: String,
        val speedMps: Double,
    )

    private fun replay(
        samples: List<RouteSimSample>,
        truth: List<Step>,
        pushMock: Boolean,
    ): Replay {
        NaviMapTestHooks.deadReckoningReset?.invoke()
        val injectGps = NaviMapTestHooks.deadReckoningInjectGps
        val injectImu = NaviMapTestHooks.deadReckoningInject
        assertNotNull(injectGps)
        assertNotNull(injectImu)
        val tunnelStart = samples.first { it.isTunnel }.cumM
        val tunnelEnd = samples.last { it.isTunnel }.cumM
        val seed = truth.first { it.alongM < tunnelStart - 40.0 }
        // Stay ahead of the live sensor tick, which uses wall-clock time.
        // A tick behind this clock has a negative dt and is ignored.
        var clock = System.currentTimeMillis() + 5_000L
        repeat(8) {
            injectGps!!.invoke(
                GpsSample(
                    tMs = clock,
                    lat = seed.lat,
                    lon = seed.lon,
                    accuracyM = 5f,
                    speedMps = seed.speedMps,
                    bearingDeg = 0.0,
                ),
            )
            injectImu!!.invoke(
                SensorFrame(
                    tMs = clock + 50L,
                    headingDeg = 0.0,
                    yawRateDegPerS = 0.1,
                    forwardAccelMps2 = 0.0,
                ),
            )
            clock += 200L
        }
        var exitM: Double? = null
        var alongErr: Double? = null
        var crossErr: Double? = null
        var mode = "Gps"
        var exitSpeed = 0.0
        for ((i, s) in truth.withIndex()) {
            val t = clock + s.tMs
            val portal =
                kotlin.math.abs(s.alongM - tunnelStart) < 40.0 ||
                    kotlin.math.abs(s.alongM - tunnelEnd) < 40.0
            val vib = 0.15 * (0.85 * noise(1, i) + 0.15 * sin(i * 0.37))
            when {
                s.inTunnel -> {
                    injectGps!!.invoke(GpsSample(tMs = t, lat = 0.0, lon = 0.0, available = false))
                }
                portal -> {
                    // Same as the unit suite: a portal fix is not good enough to
                    // end dead reckoning, and it is not a loss either.
                    val portalSpeed = if (s.speedMps <= 0.05) 0.0 else s.speedMps
                    injectGps!!.invoke(
                        GpsSample(
                            tMs = t,
                            lat = s.lat,
                            lon = s.lon,
                            accuracyM = 35f,
                            speedMps = portalSpeed,
                            bearingDeg = 0.0,
                        ),
                    )
                    if (pushMock) pushMock(s, portalSpeed)
                }
                s.tMs % 1_000L < 100L -> {
                    val reported = (s.speedMps + 0.2 * noise(8, i)).coerceAtLeast(0.0)
                    injectGps!!.invoke(
                        GpsSample(
                            tMs = t,
                            lat = s.lat,
                            lon = s.lon,
                            accuracyM = 8f,
                            speedMps = reported,
                            bearingDeg = 0.0,
                        ),
                    )
                    if (pushMock) pushMock(s, reported)
                }
            }
            val out =
                injectImu!!.invoke(
                    SensorFrame(
                        tMs = t,
                        headingDeg = 0.0,
                        yawRateDegPerS = 0.0,
                        forwardAccelMps2 = s.accel + vib,
                    ),
                )
            if (exitM == null && s.alongM >= tunnelEnd + 20.0 && !s.inTunnel) {
                exitM = RouteProgressTracker.haversineM(out.lat, out.lon, s.lat, s.lon)
                val along = (out.alongM ?: s.alongM) - s.alongM
                alongErr = along
                val cross2 = exitM * exitM - along * along
                crossErr = if (cross2 > 0.0) kotlin.math.sqrt(cross2) else 0.0
                mode = out.mode.name
                exitSpeed = out.speedMps
            }
        }
        return Replay(alongErr, crossErr, exitM, mode, exitSpeed)
    }

    private fun pushMock(
        s: Step,
        speed: Double,
    ) {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val lm = context.getSystemService(LocationManager::class.java) ?: return
        val loc =
            Location(PROVIDER).apply {
                latitude = s.lat
                longitude = s.lon
                accuracy = 8f
                this.speed = speed.toFloat()
                time = System.currentTimeMillis()
                elapsedRealtimeNanos = System.nanoTime()
            }
        runCatching { lm.setTestProviderLocation(PROVIDER, loc) }
    }

    private fun tryMockProvider(): Boolean {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val lm = context.getSystemService(LocationManager::class.java) ?: return false
        return runCatching {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                lm.addTestProvider(
                    PROVIDER,
                    false,
                    false,
                    false,
                    false,
                    true,
                    true,
                    true,
                    ProviderProperties.POWER_USAGE_LOW,
                    ProviderProperties.ACCURACY_FINE,
                )
            } else {
                @Suppress("DEPRECATION")
                lm.addTestProvider(PROVIDER, false, false, false, false, true, true, true, 1, 1)
            }
            lm.setTestProviderEnabled(PROVIDER, true)
            true
        }.getOrDefault(false)
    }

    private fun straightTunnel(): List<RouteSimSample> {
        val out = mutableListOf<RouteSimSample>()
        var lat = 60.0
        var lon = 10.0
        var cum = 0.0
        val total = (5_200.0 / 25.0).toInt()
        for (i in 0..total) {
            if (i > 0) {
                val p = DeadReckoningEngine.destinationPoint(lat, lon, 0.0, 25.0)
                lat = p.first
                lon = p.second
                cum += 25.0
            }
            val tunnel = cum in 1_000.0..4_000.0
            out +=
                RouteSimSample(
                    lat = lat,
                    lon = lon,
                    cumM = cum,
                    speedKmh = 80.0,
                    highway = "trunk",
                    maxspeedPosted = true,
                    maxspeedKmh = 80.0,
                    isTunnel = tunnel,
                )
        }
        return out
    }

    private fun profile(
        samples: List<RouteSimSample>,
        case: String,
    ): List<Step> {
        val road = DrConstraint(samples)
        val cruise = if (case == "steady50") 50.0 / 3.6 else 80.0 / 3.6
        val crawl = 4.0 / 3.6
        val out = mutableListOf<Step>()
        var t = 0L
        var along = 0.0
        var speed = cruise
        var crawled = 0L
        val end = 4_200.0
        while (along < end && t < 900_000L) {
            val accel =
                if (case != "crawl" || along < 1_050.0) {
                    speed = cruise
                    0.0
                } else if (crawled < 60_000L) {
                    val a = if (speed > crawl + 0.2) ((crawl - speed) / 0.8).coerceIn(-2.5, 0.0) else 0.0
                    speed = (speed + a * 0.1).coerceAtLeast(crawl)
                    crawled += 100L
                    a
                } else if (speed < cruise - 0.2) {
                    speed = (speed + 2.5 * 0.1).coerceAtMost(cruise)
                    2.5
                } else {
                    speed = cruise
                    0.0
                }
            along += speed * 0.1
            val p = DeadReckoningEngine.pointAtAlong(road, along.coerceAtMost(samples.last().cumM))
            val inTunnel = samples.minByOrNull { kotlin.math.abs(it.cumM - along) }?.isTunnel == true
            out += Step(t, along, speed, accel, inTunnel, p.lat, p.lon)
            t += 100L
        }
        return out
    }

    private fun routeOf(samples: List<RouteSimSample>): CorridorRouteResult {
        val json =
            samples.joinToString(",", prefix = "[", postfix = "]") { s ->
                """{"lat":${s.lat},"lon":${s.lon},"cum_m":${s.cumM},"speed_kmh":80,""" +
                    """"highway":"trunk","maxspeed_posted":true,"maxspeed_kmh":80,"is_tunnel":${s.isTunnel}}"""
            }
        val poly = samples.joinToString(";") { "${it.lon},${it.lat}" }
        val end = samples.last()
        return CorridorRouteResult(
            report = "TEST_KIND=TABLET_REPLAY\nPASS\n",
            distanceKm = end.cumM / 1000.0,
            etaMinutes = 10.0,
            cacheHit = true,
            coldBuildS = 0.0,
            warmLoadS = 0.0,
            routePolyline = poly,
            poiLat = end.lat,
            poiLon = end.lon,
            poiName = "Synthetic end",
            poiIconKey = "city",
            breakPoisJson = "[]",
            daysJson = "[]",
            simSamplesJson = json,
            maneuversJson = "[]",
            priorityPathSharePct = 100.0,
            routeSegmentsJson = "[]",
            offTrailAdvisory = "",
            tollPolicy = "allow",
            padAttemptsJson = "[]",
            searchExpansions = 0u,
            searchTerminateReason = "ok",
            tollAvoidanceIncomplete = false,
            routeUsesTolls = false,
        )
    }

    private fun noise(
        seed: Long,
        i: Int,
    ): Double {
        val x = (seed * 1103515245L + i * 12345L) and 0x7fffffffL
        return (x.toDouble() / 0x7fffffff.toDouble()) * 2.0 - 1.0
    }

    private fun fmt(v: Double?): String = if (v == null) "n/a" else String.format(java.util.Locale.US, "%.1f", v)

    private fun waitUntil(
        timeoutMs: Long,
        pred: () -> Boolean,
    ) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (pred()) return
            Thread.sleep(100)
        }
        assertTrue(pred())
    }

    companion object {
        private const val PROVIDER = "navi-replay"
    }
}
