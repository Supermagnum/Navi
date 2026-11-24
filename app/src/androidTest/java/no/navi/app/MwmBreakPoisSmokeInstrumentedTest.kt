package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.TravelProfile
import uniffi.navi.comapsNativeAvailable
import uniffi.navi.comapsSearchEnabled
import uniffi.navi.comapsSessionClose
import uniffi.navi.comapsSessionIsOpen
import uniffi.navi.comapsSessionOpen
import java.io.File

/**
 * Smoke: MWM production planner fills break_pois / rest_place_count from
 * installed Ostlandet leaves (Hamar → Lillehammer hiking).
 */
@RunWith(AndroidJUnit4::class)
class MwmBreakPoisSmokeInstrumentedTest {
    private lateinit var root: File

    @Before
    fun setUp() {
        MwmClientBootstrap.ensureInitialized()
        assumeTrue("CoMaps native core not linked", comapsNativeAvailable())
        assumeTrue("MWM client flag off", comapsSearchEnabled())
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dataMwm = File(NaviAppData.resolve(ctx), "mwm")
        val productionLeaves =
            File(dataMwm, "releases").listFiles().orEmpty().flatMap { rel ->
                File(rel, "leaves").listFiles { f -> f.isFile && f.name.endsWith(".mwm") }.orEmpty().toList()
            }
        assumeTrue("no production MWM leaves", productionLeaves.isNotEmpty())
        assumeTrue("World.mwm missing", File(dataMwm, "World.mwm").isFile)
        root = dataMwm
        ComapsHeadlessPlatform.requireContext(ctx)
        if (comapsSessionIsOpen()) comapsSessionClose()
        val ver =
            productionLeaves
                .first()
                .parentFile
                ?.parentFile
                ?.name
        val sidecars =
            ver
                ?.let { File(dataMwm, "releases/$it/sidecars") }
                ?.takeIf { it.isDirectory }
        comapsSessionOpen(
            storagePath = dataMwm.absolutePath,
            writablePath = dataMwm.absolutePath,
            sidecarDir = sidecars?.absolutePath,
            loadWetlandSidecars = false,
        )
        assumeTrue("session not open", comapsSessionIsOpen())
    }

    @After
    fun tearDown() {
        runCatching { if (comapsSessionIsOpen()) comapsSessionClose() }
    }

    @Test
    fun hikingPlanPopulatesRestPlacesFromMwm() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        // Hamar → Lillehammer stays on Ostlandet leaves (~60 km).
        val planned =
            MwmProductionPlanner.plan(
                context = ctx,
                profile = TravelProfile.HIKING,
                startLat = 60.7945,
                startLon = 11.0680,
                endLat = 61.1153,
                endLon = 10.4662,
                vias = emptyList(),
                avoidMotorways = false,
                avoidTolls = false,
                avoidFerries = false,
                turnsLocale = "nb",
            )
        assertTrue("plan failed: ${planned.report.take(400)}", planned.report.contains("PASS"))
        assertTrue("distance too short: ${planned.distanceKm}", planned.distanceKm > 20.0)
        val breaks = JSONArray(planned.breakPoisJson.ifBlank { "[]" })
        android.util.Log.i(
            "NaviPlan",
            "mwm_break_smoke rest_place_count token=" +
                Regex("rest_place_count=(\\d+)").find(planned.report)?.groupValues?.get(1) +
                " json_n=${breaks.length()} sample=${planned.breakPoisJson.take(400)}",
        )
        assertTrue(
            "expected rest places from MWM; report=${planned.report.take(800)} " +
                "breaks=${planned.breakPoisJson.take(500)}",
            breaks.length() > 0,
        )
        val first = breaks.getJSONObject(0)
        assertTrue(first.optString("name").isNotBlank())
        assertTrue(first.optDouble("lat").isFinite())
        assertTrue(first.optDouble("lon").isFinite())
    }
}
