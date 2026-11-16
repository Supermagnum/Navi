package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiComapsOptimize
import uniffi.navi.FfiComapsRouteCode
import uniffi.navi.TravelProfile
import uniffi.navi.comapsNativeAvailable
import uniffi.navi.comapsSessionClose
import uniffi.navi.comapsSessionOpen
import uniffi.navi.comapsSessionSetAvoidOptions
import uniffi.navi.comapsSessionSetBikeWetland
import uniffi.navi.comapsSessionSetOptimize
import uniffi.navi.comapsSessionSetTurnsLocale
import java.io.File
import java.io.FileOutputStream

/**
 * Phase 4 parity suite (~40 routes) on MWM / CoMaps (flag ON, wetland hook on).
 * Fixture: androidTest/assets/parity-suite-ostlandet.json
 */
@RunWith(AndroidJUnit4::class)
class ComapsPhase4SuiteInstrumentedTest {
    private lateinit var root: File
    private lateinit var outDir: File

    @Before
    fun setUp() {
        MwmClientBootstrap.ensureInitialized()
        check(comapsNativeAvailable())
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        root = File(ctx.getExternalFilesDir(null), "navi-mwm-test")
        outDir = File(root, "out-parity-mwm")
        outDir.mkdirs()
        File(root, "261105").mkdirs()

        fun copyAssetDir(
            assetPath: String,
            destDir: File,
        ) {
            val children = ctx.assets.list(assetPath).orEmpty()
            if (children.isEmpty()) {
                destDir.parentFile?.mkdirs()
                if (destDir.isFile && destDir.length() > 0L) return
                ctx.assets.open(assetPath).use { input ->
                    FileOutputStream(destDir).use { output -> input.copyTo(output) }
                }
                return
            }
            destDir.mkdirs()
            for (child in children) {
                if (child == "MANIFEST.txt") continue
                copyAssetDir("$assetPath/$child", File(destDir, child))
            }
        }
        copyAssetDir("comaps-data", root)
    }

    private fun travelProfileOf(s: String): TravelProfile =
        when (s.lowercase()) {
            "car" -> TravelProfile.CAR
            "foot", "hike", "pedestrian" -> TravelProfile.HIKING
            "bike", "bicycle" -> TravelProfile.BICYCLE
            else -> TravelProfile.CAR
        }

    private fun codeFromReport(report: String): FfiComapsRouteCode {
        val token =
            Regex("""comaps_route_code=([A-Za-z_]+)""")
                .find(report)
                ?.groupValues
                ?.getOrNull(1)
                ?: return if (report.contains("PASS")) FfiComapsRouteCode.OK else FfiComapsRouteCode.INTERNAL
        return runCatching { FfiComapsRouteCode.valueOf(token) }.getOrDefault(FfiComapsRouteCode.INTERNAL)
    }

    @Test
    fun runParitySuiteMwm() {
        val instr = InstrumentationRegistry.getInstrumentation()
        val suiteText =
            instr.context.assets
                .open("parity-suite-ostlandet.json")
                .bufferedReader()
                .readText()
        val suite = JSONObject(suiteText)
        val routes = suite.getJSONArray("routes")

        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val open =
            comapsSessionOpen(
                storagePath = root.absolutePath,
                writablePath = root.absolutePath,
                sidecarDir = File(root, "sidecars").absolutePath,
                loadWetlandSidecars = true,
            )
        assertTrue("session: ${open.errorMessage}", open.ok)
        assertTrue("production planner should see open session", MwmProductionPlanner.shouldUse(ctx))
        comapsSessionSetTurnsLocale("nb")
        comapsSessionSetBikeWetland(false)
        comapsSessionSetOptimize(FfiComapsOptimize.TIME)
        comapsSessionSetAvoidOptions(false, false, false, false)

        val results = JSONArray()
        var okCount = 0
        var expectedFailures = 0
        for (i in 0 until routes.length()) {
            val r = routes.getJSONObject(i)
            val id = r.getString("id")
            // PreferNonMotorway for the product "shortest" demo id
            if (id == "lillehammer_drammen_shortest") {
                comapsSessionSetOptimize(FfiComapsOptimize.PREFER_NON_MOTORWAY)
            } else {
                comapsSessionSetOptimize(FfiComapsOptimize.TIME)
            }
            val wetland =
                r.optJSONArray("tags")?.let { tags ->
                    (0 until tags.length()).any {
                        tags.getString(it).contains("wetland") ||
                            tags.getString(it) == "soft_hard" ||
                            tags.getString(it) == "boardwalk"
                    }
                } == true
            val from = r.getJSONObject("from")
            val to = r.getJSONObject("to")
            // Production UI path: same adapter MainActivity uses (CorridorRouteResult + PLANNER=mwm).
            val corridor =
                MwmProductionPlanner.plan(
                    context = ctx,
                    profile = travelProfileOf(r.getString("profile")),
                    startLat = from.getDouble("lat"),
                    startLon = from.getDouble("lon"),
                    endLat = to.getDouble("lat"),
                    endLon = to.getDouble("lon"),
                    vias = emptyList(),
                    avoidMotorways = id == "lillehammer_drammen_shortest",
                    avoidTolls = false,
                    avoidFerries = false,
                    turnsLocale = "nb",
                    bikeWetland = wetland,
                )
            assertTrue("$id: missing PLANNER=mwm", corridor.report.contains("PLANNER=mwm"))
            val code = codeFromReport(corridor.report)
            val expect = r.optString("expect", "ok")
            if (expect == "ok") {
                if (code == FfiComapsRouteCode.OK && corridor.report.contains("PASS")) okCount++
            } else {
                val want =
                    when (expect) {
                        "end_not_on_road" -> FfiComapsRouteCode.END_NOT_ON_ROAD
                        "start_not_on_road" -> FfiComapsRouteCode.START_NOT_ON_ROAD
                        else -> error("$id: unknown expect '$expect'")
                    }
                assertEquals("$id: expected $expect", want, code)
                assertTrue("$id: no polyline for a failed route", corridor.routePolyline.isEmpty())
                expectedFailures++
            }

            val coords = JSONArray()
            for (pair in corridor.routePolyline.split(';').filter { it.isNotBlank() }) {
                val xy = pair.split(',')
                if (xy.size >= 2) {
                    coords.put(JSONArray().put(xy[0].toDouble()).put(xy[1].toDouble()))
                }
            }
            val gj =
                JSONObject()
                    .put("type", "Feature")
                    .put(
                        "geometry",
                        JSONObject().put("type", "LineString").put("coordinates", coords),
                    ).put(
                        "properties",
                        JSONObject()
                            .put("id", id)
                            .put("router", "mwm_production")
                            .put("code", code.name)
                            .put("length_m", corridor.distanceKm * 1000.0)
                            .put("eta_minutes", corridor.etaMinutes),
                    )
            File(outDir, "$id.mwm.geojson").writeText(gj.toString())

            results.put(
                JSONObject()
                    .put("id", id)
                    .put("profile", r.getString("profile"))
                    .put("tags", r.optJSONArray("tags"))
                    .put("code", code.name)
                    .put("length_m", corridor.distanceKm * 1000.0)
                    .put("eta_minutes", corridor.etaMinutes)
                    .put("build_s", corridor.coldBuildS)
                    .put("point_count", coords.length())
                    .put("planner", "mwm")
                    .put("geojson", File(outDir, "$id.mwm.geojson").absolutePath)
                    .put(
                        "report_head",
                        corridor.report
                            .lineSequence()
                            .take(6)
                            .joinToString(" | "),
                    ),
            )
        }
        comapsSessionClose()

        val summary =
            JSONObject()
                .put("router", "mwm")
                .put("device", "R52TB0JQEDE")
                .put("route_count", routes.length())
                .put("ok_count", okCount)
                .put("expected_failure_count", expectedFailures)
                .put("results", results)
        File(outDir, "parity-mwm-summary.json").writeText(summary.toString(2))
        // Soft gate: at least half the suite must OK (mountain leaves may be missing).
        assertTrue(
            "expected >=20 OK routes, got $okCount / ${routes.length()}",
            okCount >= 20,
        )
    }
}
