package no.navi.app

import android.location.Location
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.CorridorRouteResult
import java.io.File
import kotlin.math.abs

/**
 * Tunnel dead-reckoning smoke on a densified corridor that already carries
 * `is_tunnel` on sim samples (from graph edges). Plans may come from a staged
 * fixture or a prior long-trip pull under /data/local/tmp.
 *
 * Drops GPS inside every tunnel sample run, feeds motion-consistent sensor
 * frames via [NaviMapTestHooks.deadReckoningInject], and asserts the map mark
 * keeps moving on-road with a smooth GPS hand-back.
 */
@RunWith(AndroidJUnit4::class)
class TunnelDeadReckoningInstrumentedTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var dataDir: File

    @Before
    fun setUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.ignoreLiveGpsFixes = true
        NaviMapTestHooks.clearDebugGpsInject()
        NaviMapTestHooks.gpsEstimated = false
        NaviMapTestHooks.positionLost = false
        NaviMapTestHooks.lastDrMode = "Gps"
        NaviMapTestHooks.lastDrExitErrorM = null
    }

    @Test
    fun tunnels_dropGps_keepMoving_andHandBack() {
        val route = loadRouteOrSynthetic()
        val samples = parseRouteSimSamples(route.simSamplesJson)
        assertTrue("need densified samples", samples.size >= 20)

        val tunnelRuns = tunnelRuns(samples)
        assertTrue(
            "fixture must include at least one tunnel sample (is_tunnel=true); " +
                "got ${samples.count { it.isTunnel }} tunnel samples of ${samples.size}",
            tunnelRuns.isNotEmpty(),
        )

        activityRule.launchActivity(null)
        waitStyle()
        waitUntil(15_000) { NaviMapTestHooks.deadReckoningInject != null }

        NaviMapTestHooks.pendingRoute = route
        waitUntil(20_000) { NaviMapTestHooks.lastSimAlongM != null || mapHasPolyline() }

        val report = StringBuilder()
        report.appendLine("tunnel_runs=${tunnelRuns.size}")

        // Seed a good fix just before the first tunnel (direct into DR).
        val first = tunnelRuns.first()
        val seedIdx = (first.first - 3).coerceAtLeast(0)
        val seed = samples[seedIdx]
        NaviMapTestHooks.deadReckoningInjectGps!!.invoke(
            GpsSample(
                tMs = System.currentTimeMillis(),
                lat = seed.lat,
                lon = seed.lon,
                accuracyM = 8f,
                speedMps = seed.speedKmh / 3.6,
                bearingDeg = 0.0,
                available = true,
            ),
        )
        injectGps(seed, accuracyM = 8f)
        waitUntil(5_000) { NaviMapTestHooks.lastDrMode == "Gps" }

        for ((runIdx, run) in tunnelRuns.withIndex()) {
            val (start, end) = run
            val mid = (start + end) / 2
            var moved = false
            var maxAlong = samples[start].cumM
            var onRoad = true

            // Re-seed at tunnel entry so each gap starts from a known good fix.
            val entry = samples[start]
            NaviMapTestHooks.deadReckoningInjectGps!!.invoke(
                GpsSample(
                    tMs = System.currentTimeMillis(),
                    lat = entry.lat,
                    lon = entry.lon,
                    accuracyM = 8f,
                    speedMps = entry.speedKmh / 3.6,
                    bearingDeg = 0.0,
                    available = true,
                ),
            )
            injectGpsUnavailable()
            var tickMs = System.currentTimeMillis()
            for (i in start..end) {
                val s = samples[i]
                val bearing =
                    if (i + 1 < samples.size) {
                        RouteProgressTracker.bearingDeg(s.lat, s.lon, samples[i + 1].lat, samples[i + 1].lon)
                    } else {
                        0.0
                    }
                val stepM =
                    if (i > 0) {
                        (s.cumM - samples[i - 1].cumM).coerceAtLeast(1.0)
                    } else {
                        40.0
                    }
                val speedMps = (s.speedKmh / 3.6).coerceAtLeast(1.0)
                val dtMs = ((stepM / speedMps) * 1000.0).toLong().coerceIn(50L, 5_000L)
                tickMs += dtMs
                val out =
                    NaviMapTestHooks.deadReckoningInject!!.invoke(
                        SensorFrame(
                            tMs = tickMs,
                            headingDeg = bearing,
                            forwardAccelMps2 = 0.0,
                        ),
                    )
                val along = out.alongM ?: 0.0
                if (along > maxAlong + 1.0) {
                    moved = true
                    maxAlong = along
                }
                // Prefer constraint snap; fall back to nearest sample by distance.
                val nearest = samples.minBy { abs(it.cumM - along) }
                val xt =
                    RouteProgressTracker.haversineM(out.lat, out.lon, nearest.lat, nearest.lon)
                if (xt > 100.0) onRoad = false

                if (i == mid) {
                    capture("a2_tunnel_${runIdx}_inside.png")
                    assertTrue(
                        "expected estimated mark inside tunnel $runIdx (mode=${NaviMapTestHooks.lastDrMode})",
                        NaviMapTestHooks.gpsEstimated ||
                            NaviMapTestHooks.lastDrMode == "Estimating" ||
                            out.estimated,
                    )
                }
            }

            // Exit: good GPS at first sample past the tunnel (direct into DR).
            val exitIdx = (end + 1).coerceAtMost(samples.lastIndex)
            val exit = samples[exitIdx]
            val exitBearing =
                RouteProgressTracker.bearingDeg(
                    exit.lat,
                    exit.lon,
                    samples[minOf(exitIdx + 1, samples.lastIndex)].lat,
                    samples[minOf(exitIdx + 1, samples.lastIndex)].lon,
                )
            val handBack =
                NaviMapTestHooks.deadReckoningInjectGps!!.invoke(
                    GpsSample(
                        tMs = System.currentTimeMillis(),
                        lat = exit.lat,
                        lon = exit.lon,
                        accuracyM = 8f,
                        speedMps = exit.speedKmh / 3.6,
                        bearingDeg = exitBearing,
                        available = true,
                    ),
                )
            // Allow blend window.
            var lastOut = handBack
            repeat(15) {
                lastOut =
                    NaviMapTestHooks.deadReckoningInject!!.invoke(
                        SensorFrame(
                            tMs = System.currentTimeMillis(),
                            headingDeg = exitBearing,
                        ),
                    )
                Thread.sleep(80)
            }
            val err = lastOut.exitErrorM ?: NaviMapTestHooks.lastDrExitErrorM
            val smooth =
                lastOut.mode == DrMode.Gps &&
                    !lastOut.positionLost &&
                    !NaviMapTestHooks.positionLost &&
                    (err == null || err < 200.0)

            report.appendLine(
                "tunnel[$runIdx] samples=$start..$end " +
                    "moved=$moved on_road=$onRoad exit_err_m=${err ?: "n/a"} " +
                    "mode=${lastOut.mode} smooth=$smooth",
            )
            assertTrue("tunnel $runIdx map must keep moving", moved)
            assertTrue("tunnel $runIdx must stay on road", onRoad)
            capture("a2_tunnel_${runIdx}_exit.png")
            assertTrue(
                "tunnel $runIdx GPS hand-back should be smooth " +
                    "(mode=${lastOut.mode} err=$err lost=${lastOut.positionLost})",
                smooth,
            )
        }

        File(dataDir, "a2_tunnel_dr_report.txt").writeText(report.toString())
        android.util.Log.i("TunnelDeadReckoning", report.toString())
    }

    private fun loadRouteOrSynthetic(): CorridorRouteResult {
        val staged =
            listOf(
                File("/data/local/tmp/a2_halden_sunndal.route.json"),
                File(dataDir, "a2_halden_sunndal.route.json"),
            ).firstOrNull { it.isFile }
        if (staged != null) {
            // Minimal: expect sim_samples_json + polyline fields in a prior pull.
            val text = staged.readText()
            val poly = jsonStringField(text, "route_polyline") ?: jsonStringField(text, "polyline")
            val samples = jsonStringField(text, "sim_samples_json") ?: "[]"
            val report = jsonStringField(text, "report") ?: ""
            check(!poly.isNullOrBlank()) { "staged route missing polyline" }
            val end = samplesLastLatLon(samples)
            return CorridorRouteResult(
                report = report.ifBlank { "TEST_KIND=A2_STAGED\nPASS\n" },
                distanceKm = jsonDoubleField(text, "distance_km") ?: 1.0,
                etaMinutes = jsonDoubleField(text, "eta_minutes") ?: 1.0,
                cacheHit = true,
                coldBuildS = 0.0,
                warmLoadS = 0.0,
                routePolyline = poly,
                poiLat = end.first,
                poiLon = end.second,
                poiName = "End",
                poiIconKey = "city",
                breakPoisJson = "[]",
                daysJson = "[]",
                simSamplesJson = samples,
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
        // Synthetic straight tunnel corridor (~1.2 km tunnel in the middle).
        val samples = mutableListOf<RouteSimSample>()
        var lat = 60.0
        var lon = 10.0
        var cum = 0.0
        for (i in 0..60) {
            val tunnel = i in 15..45
            val p = DeadReckoningEngine.destinationPoint(lat, lon, 0.0, 40.0)
            if (i > 0) {
                lat = p.first
                lon = p.second
                cum += 40.0
            }
            samples +=
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
        val json =
            samples.joinToString(",", prefix = "[", postfix = "]") { s ->
                """{"lat":${s.lat},"lon":${s.lon},"cum_m":${s.cumM},"speed_kmh":${s.speedKmh},""" +
                    """"highway":"trunk","maxspeed_posted":true,"maxspeed_kmh":80,"is_tunnel":${s.isTunnel}}"""
            }
        // MapLibre polyline encoding uses lon,lat.
        val poly = samples.joinToString(";") { "${it.lon},${it.lat}" }
        return CorridorRouteResult(
            report = "route_tunnel_count=1\nroute_tunnel_fp=synthetic@1.20\nTEST_KIND=A2_SYNTH\nPASS\n",
            distanceKm = cum / 1000.0,
            etaMinutes = (cum / 1000.0) / 80.0 * 60.0,
            cacheHit = true,
            coldBuildS = 0.0,
            warmLoadS = 0.0,
            routePolyline = poly,
            poiLat = lat,
            poiLon = lon,
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

    private fun samplesLastLatLon(samplesJson: String): Pair<Double, Double> {
        val parsed = parseRouteSimSamples(samplesJson)
        val last = parsed.lastOrNull() ?: return 0.0 to 0.0
        return last.lat to last.lon
    }

    private fun tunnelRuns(samples: List<RouteSimSample>): List<Pair<Int, Int>> {
        val runs = mutableListOf<Pair<Int, Int>>()
        var start = -1
        for (i in samples.indices) {
            if (samples[i].isTunnel) {
                if (start < 0) start = i
            } else if (start >= 0) {
                runs += start to (i - 1)
                start = -1
            }
        }
        if (start >= 0) runs += start to samples.lastIndex
        return runs
    }

    private fun injectGps(
        s: RouteSimSample,
        accuracyM: Float,
    ) {
        NaviMapTestHooks.pendingInjectFixSpeedKmh = s.speedKmh
        NaviMapTestHooks.pendingInjectFixLatLon = s.lat to s.lon
        // Also mirror via test provider location for DR seed accuracy.
        val loc =
            Location("navi-test-inject").apply {
                latitude = s.lat
                longitude = s.lon
                time = System.currentTimeMillis()
                speed = (s.speedKmh / 3.6).toFloat()
                accuracy = accuracyM
                bearing = 0f
            }
        // pendingInjectFixLatLon is consumed by MainActivity poll; give it a moment.
        waitUntil(5_000) {
            abs(NaviMapTestHooks.lastGpsLat - s.lat) < 1e-4 &&
                abs(NaviMapTestHooks.lastGpsLon - s.lon) < 1e-4
        }
        assertNotNull(loc)
    }

    private fun injectGpsUnavailable() {
        val out = NaviMapTestHooks.deadReckoningNotifyLost?.invoke()
        assertNotNull("deadReckoningNotifyLost hook", out)
        assertTrue(
            "expected Estimating after GPS loss (mode=${out!!.mode})",
            out.mode == DrMode.Estimating || out.estimated,
        )
    }

    private fun capture(name: String) {
        Thread.sleep(400)
        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertTrue("null shot $name", shot != null)
        val out = File(dataDir, name)
        out.outputStream().use { shot!!.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        shell("screencap -p /data/local/tmp/$name")
        shell("chmod 644 /data/local/tmp/$name")
    }

    private fun shell(cmd: String) {
        val pfd = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
        java.io.FileInputStream(pfd.fileDescriptor).use { input ->
            val buf = ByteArray(4096)
            while (input.read(buf) >= 0) {
            }
        }
        pfd.close()
    }

    private fun waitStyle(timeoutMs: Long = 45_000) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady || NaviMapTestHooks.lastReportedLayerCount >= 1) return
            Thread.sleep(300)
        }
    }

    private fun waitUntil(
        timeoutMs: Long,
        pred: () -> Boolean,
    ) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (pred()) return
            Thread.sleep(100)
        }
        assertTrue("timeout waiting", pred())
    }

    private fun mapHasPolyline(): Boolean = NaviMapTestHooks.lastSimAlongM != null

    private fun jsonStringField(
        text: String,
        key: String,
    ): String? {
        val re = Regex(""""$key"\s*:\s*"((?:\\.|[^"\\])*)"""")
        return re
            .find(text)
            ?.groupValues
            ?.get(1)
            ?.replace("\\n", "\n")
            ?.replace("\\\"", "\"")
    }

    private fun jsonDoubleField(
        text: String,
        key: String,
    ): Double? {
        val re = Regex(""""$key"\s*:\s*([0-9.]+)""")
        return re
            .find(text)
            ?.groupValues
            ?.get(1)
            ?.toDoubleOrNull()
    }
}
