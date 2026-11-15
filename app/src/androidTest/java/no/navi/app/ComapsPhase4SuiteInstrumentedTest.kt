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
import uniffi.navi.FfiComapsProfile
import uniffi.navi.FfiComapsRouteCode
import uniffi.navi.comapsNativeAvailable
import uniffi.navi.comapsSessionClose
import uniffi.navi.comapsSessionOpen
import uniffi.navi.comapsSessionRoute
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

    private fun profileOf(s: String): FfiComapsProfile =
        when (s.lowercase()) {
            "car" -> FfiComapsProfile.CAR
            "foot", "hike", "pedestrian" -> FfiComapsProfile.PEDESTRIAN
            "bike", "bicycle" -> FfiComapsProfile.BICYCLE
            else -> FfiComapsProfile.CAR
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

        val open =
            comapsSessionOpen(
                storagePath = root.absolutePath,
                writablePath = root.absolutePath,
                sidecarDir = File(root, "sidecars").absolutePath,
                loadWetlandSidecars = true,
            )
        assertTrue("session: ${open.errorMessage}", open.ok)
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
            val route =
                comapsSessionRoute(
                    profile = profileOf(r.getString("profile")),
                    fromLat = from.getDouble("lat"),
                    fromLon = from.getDouble("lon"),
                    toLat = to.getDouble("lat"),
                    toLon = to.getDouble("lon"),
                    wetlandHook = wetland || r.getString("profile") in setOf("foot", "hike"),
                )
            // Always enable wetland hook for foot in parity (Navi policy); car only when tagged.
            val expect = r.optString("expect", "ok")
            if (expect == "ok") {
                if (route.code == FfiComapsRouteCode.OK) okCount++
            } else {
                // Off-road endpoints must come back as the business code, never auto-snapped.
                val want =
                    when (expect) {
                        "end_not_on_road" -> FfiComapsRouteCode.END_NOT_ON_ROAD
                        "start_not_on_road" -> FfiComapsRouteCode.START_NOT_ON_ROAD
                        else -> error("$id: unknown expect '$expect'")
                    }
                assertEquals("$id: expected $expect", want, route.code)
                assertTrue("$id: no polyline for a failed route", route.points.isEmpty())
                expectedFailures++
            }

            // GeoJSON for every route
            val coords = JSONArray()
            for (p in route.points) {
                coords.put(JSONArray().put(p.lon).put(p.lat))
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
                            .put("router", "mwm")
                            .put("code", route.code.name)
                            .put("length_m", route.lengthM)
                            .put("time_s", route.timeS),
                    )
            File(outDir, "$id.mwm.geojson").writeText(gj.toString())

            results.put(
                JSONObject()
                    .put("id", id)
                    .put("profile", r.getString("profile"))
                    .put("tags", r.optJSONArray("tags"))
                    .put("code", route.code.name)
                    .put("length_m", route.lengthM)
                    .put("time_s", route.timeS)
                    .put("build_s", route.buildTimeS)
                    .put("point_count", route.points.size)
                    .put("turn_count", JSONArray(route.turnsJson).length())
                    .put("geojson", File(outDir, "$id.mwm.geojson").absolutePath)
                    .put("error", route.errorMessage),
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
