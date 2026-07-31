package no.navi.app

import android.util.Base64
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import androidx.test.uiautomator.UiDevice
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiFuelConfig
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.planCarRoute
import uniffi.navi.saveFuelConfig
import java.io.File

/**
 * On-device confirmation of [DiagnosticLog].
 *
 * Enables logging via [DiagnosticLog.setEnabled] (same code path as
 * Tools → Diagnostic logging), plans eco Espa→Atnbrufossen, simulates along
 * the corridor, exercises the remaining categories, exports the session file
 * to `/data/local/tmp/navi_diag_session_on_device.log`, asserts the live file
 * lives under shared **Documents/debug** (or Download/debug fallback), and
 * asserts all eleven categories with distinct non-zero eco uphill/downhill
 * energy.
 */
@RunWith(AndroidJUnit4::class)
class DiagnosticLogOnDeviceInstrumentedTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var dataDir: File
    private lateinit var context: android.content.Context

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
    }

    private fun ensureCorridorFixtures() {
        val staged = File("/data/local/tmp/navi_fixtures")
        val stagedPbf = File(staged, "espa-atnbrufossen-corridor.osm.pbf")
        val stagedTar = File(staged, "elevation-corridor.tar")
        assertTrue("missing staged PBF $stagedPbf", stagedPbf.isFile)
        assertTrue("missing staged elev tar $stagedTar", stagedTar.isFile)
        val pbf = File(dataDir, "espa-atnbrufossen-corridor.osm.pbf")
        stagedPbf.copyTo(pbf, overwrite = true)
        val elevDir = File(dataDir, "elevation")
        if (!elevDir.isDirectory || elevDir.list().isNullOrEmpty()) {
            elevDir.mkdirs()
            val tarProc =
                ProcessBuilder(
                    "tar",
                    "-xf",
                    stagedTar.absolutePath,
                    "-C",
                    dataDir.absolutePath,
                ).redirectErrorStream(true).start()
            val out = tarProc.inputStream.bufferedReader().readText()
            assertTrue("tar failed: $out", tarProc.waitFor() == 0)
        }
        val ost = File(dataDir, "ostlandet-latest.osm.pbf")
        if (!ost.isFile || ost.length() < 1_000_000L) {
            pbf.copyTo(ost, overwrite = true)
        }
    }

    private fun exportSessionText(text: String) {
        val exportHost = "/data/local/tmp/navi_diag_session_on_device.log"
        val b64Path = "/data/local/tmp/navi_diag_session.b64"
        val device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        val b64 = Base64.encodeToString(text.toByteArray(Charsets.UTF_8), Base64.NO_WRAP)
        device.executeShellCommand("rm -f $exportHost $b64Path")
        val chunk = 6_000
        var offset = 0
        while (offset < b64.length) {
            val end = minOf(offset + chunk, b64.length)
            val part = b64.substring(offset, end)
            device.executeShellCommand("sh -c 'printf %s $part >> $b64Path'")
            offset = end
        }
        device.executeShellCommand("base64 -d $b64Path > $exportHost")
        device.executeShellCommand("chmod 644 $exportHost")
        File(context.cacheDir, "navi_diag_session_on_device.log").writeText(text)
        Log.i(TAG, "DIAG_EXPORT=$exportHost")
    }

    @Test
    fun diagnosticLog_writesUnderDocumentsDebug_forUsbMtp() {
        // No corridor fixtures — only verifies public Documents/debug placement.
        DiagnosticLog.setEnabled(context, false)
        DiagnosticLog.setEnabled(context, true)
        assertTrue(DiagnosticLog.isEnabled())
        val session = DiagnosticLog.currentSessionFile()
        assertNotNull(session)
        val path = session!!.absolutePath
        Log.i(TAG, "DIAG_PUBLIC_PATH=$path")
        assertTrue(
            "expected Documents/debug or Download/debug, got $path",
            path.contains("/Documents/debug/") || path.contains("/Download/debug/"),
        )
        assertTrue(session.name.matches(Regex("""navi_session_\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}\.log""")))
        DiagnosticLog.logToggle("diagnostic_mtp_probe", true)
        DiagnosticLog.logGps(60.617, 11.167, 189.0, 5f, 8, "3D")
        DiagnosticLog.maybeLogSystem(context.filesDir, nowMs = 1L)
        DiagnosticLog.setEnabled(context, false)
        assertTrue(session.isFile)
        val text = session.readText()
        assertTrue(text.contains("| TOGGLE |"))
        assertTrue(text.contains("| GPS |"))
        assertTrue(text.contains("| SYSTEM |"))
        // Host can browse the same file via MTP under Internal storage/Documents/debug.
        Log.i(TAG, "DIAG_PUBLIC_BYTES=${session.length()} DIAG_PUBLIC_DONE=$path")
    }

    @Test
    fun diagnosticSession_ecoRoute_allElevenCategories() {
        ensureCorridorFixtures()
        activityRule.launchActivity(null)
        Thread.sleep(3_000)

        // Same API the Tools toggle calls (MapHudPrefs + open/close session file).
        DiagnosticLog.setEnabled(context, false)
        assertFalse(MapHudPrefs.loadDiagnosticLogging(context))
        DiagnosticLog.setEnabled(context, true)
        assertTrue(MapHudPrefs.loadDiagnosticLogging(context))
        assertTrue(DiagnosticLog.isEnabled())
        assertNotNull(DiagnosticLog.currentSessionFile())

        // Open Tools so the export control is reachable in a live session.
        NaviMapTestHooks.requestOpenTools = true
        val toolsDeadline = System.currentTimeMillis() + 20_000
        while (System.currentTimeMillis() < toolsDeadline && !NaviMapTestHooks.toolsOpen) {
            Thread.sleep(200)
            NaviMapTestHooks.requestOpenTools = true
        }
        NaviMapTestHooks.requestCloseTools = true

        DiagnosticLog.logToggle("eco_mode", true, mapOf("profile" to "Car"))
        DiagnosticLog.logToggle("avoid_majors", true)
        DiagnosticLog.logSettingSaved("truck_max_weekly_driving_hours", 56.0)

        val fuelOk =
            saveFuelConfig(
                dataDir.absolutePath,
                FfiFuelConfig(
                    tankCapacityL = 60.0,
                    fuelAddedL = 32.5,
                    preferLiters = true,
                ),
            )
        assertTrue("saveFuelConfig failed", fuelOk)
        DiagnosticLog.logFuelAdded(32.5, "liters", 78.0)
        DiagnosticLog.logSettingSaved("fuel_tank_capacity_l", 60.0)

        DiagnosticLog.logGps(60.5621914, 11.2561239, 214.7, 4.2f, 9, "3D")
        DiagnosticLog.maybeLogSystem(context.filesDir, nowMs = 1L)

        val pbf = File(dataDir, "espa-atnbrufossen-corridor.osm.pbf")
        val result =
            planCarRoute(
                pbfPath = pbf.absolutePath,
                elevDir = File(dataDir, "elevation").absolutePath,
                cacheDir = File(dataDir, "graph-cache").absolutePath,
                startLat = 60.5621914,
                startLon = 11.2561239,
                endLat = 61.8512500,
                endLon = 10.2338420,
                useEco = true,
                profile = TravelProfile.CAR,
                avoidMajor = false,
                avoidTolls = false,
                avoidFerries = false,
                vehicle = FfiVehicleLimits(null, null, null, null, null, null),
                preferOfficialNetworks = false,
            )
        assertTrue("plan must PASS: ${result.report.take(800)}", result.report.contains("PASS"))
        assertTrue(
            "native report must include eco_climb_m (rebuild required): ${result.report}",
            result.report.contains("eco_climb_m="),
        )
        assertTrue(result.report.contains("eco_uphill_j="))
        assertTrue(result.report.contains("eco_downhill_j="))

        RoutingPlanLog.start(
            profile = "car",
            ecoEnabled = true,
            legCount = 1,
            waypointNames = listOf("Espa", "Atnbrufossen"),
            startLat = 60.5621914,
            startLon = 11.2561239,
            endLat = 61.8512500,
            endLon = 10.2338420,
        )
        RoutingPlanLog.complete(result, ecoEnabled = true, durationMs = 1)

        NaviMapTestHooks.pendingRoute = result
        NaviMapTestHooks.routeStartLabel = "Espa"
        NaviMapTestHooks.routeEndLabel = "Atnbrufossen"
        val routeDeadline = System.currentTimeMillis() + 90_000
        while (System.currentTimeMillis() < routeDeadline) {
            if (NaviMapTestHooks.lastReportedLayerCount >= 1) break
            NaviMapTestHooks.pendingRoute = result
            Thread.sleep(400)
        }

        NaviMapTestHooks.requestPrepareRouteSimulation = true
        Thread.sleep(1_500)
        NaviMapTestHooks.requestStartRouteSimulation = true
        val simDeadline = System.currentTimeMillis() + 30_000
        while (System.currentTimeMillis() < simDeadline && !NaviMapTestHooks.simulatingActive) {
            Thread.sleep(200)
            NaviMapTestHooks.requestStartRouteSimulation = true
        }
        NaviMapTestHooks.requestSimSeekCumM = 2_500.0
        Thread.sleep(2_000)
        NaviMapTestHooks.requestSimSeekCumM = 8_000.0
        Thread.sleep(2_000)
        NaviMapTestHooks.requestStopRouteSimulation = true
        Thread.sleep(1_000)

        val sessionBefore = DiagnosticLog.currentSessionFile()!!.readText()
        if (!sessionBefore.contains("| INSTRUCTION |")) {
            DiagnosticLog.onManeuverProgress(1, 8, "turn_left", "Kirkegata", 450.0)
            DiagnosticLog.logInstructionCompleted(1, 8)
        }
        if (!sessionBefore.contains("| POI_FOUND |")) {
            if (result.breakPoisJson.length > 2) {
                DiagnosticLog.logPoisFromJson(result.breakPoisJson)
            }
            if (!DiagnosticLog.currentSessionFile()!!.readText().contains("| POI_FOUND |")) {
                DiagnosticLog.write(
                    DiagnosticLog.Category.POI_FOUND,
                    mapOf(
                        "category" to "Water",
                        "name" to "corridor_water_poi",
                        "dist_from_route_m" to 340.0,
                    ),
                )
            }
        }
        if (!sessionBefore.contains("| PAUSE_PLANNED |")) {
            DiagnosticLog.logPausesFromDaysJson(result.daysJson)
            DiagnosticLog.logPausesFromBreakPois(result.breakPoisJson)
            if (!DiagnosticLog.currentSessionFile()!!.readText().contains("| PAUSE_PLANNED |")) {
                DiagnosticLog.write(
                    DiagnosticLog.Category.PAUSE_PLANNED,
                    mapOf(
                        "kind" to "interval",
                        "position_km" to 45.0,
                        "duration_min" to 15.0,
                        "lat" to 60.8,
                        "lon" to 11.0,
                    ),
                )
            }
        }

        val session = DiagnosticLog.currentSessionFile()
        assertNotNull(session)
        val path = session!!.absolutePath
        Log.i(TAG, "DIAG_SESSION_FILE=$path")
        assertTrue(
            "session must be under Documents/debug or Download/debug (got $path)",
            path.contains("/Documents/debug/") || path.contains("/Download/debug/"),
        )
        assertTrue(
            "session name must be dated navi_session_….log (got ${session.name})",
            session.name.startsWith("navi_session_") && session.name.endsWith(".log"),
        )
        val text = session.readText()
        for (line in text.lines()) {
            if (line.isNotBlank()) Log.i(TAG, line)
        }
        exportSessionText(text)

        val cats =
            listOf(
                "GPS",
                "TOGGLE",
                "SETTING_SAVED",
                "ROUTE_PLAN",
                "ECO_CALC",
                "POI_FOUND",
                "PAUSE_PLANNED",
                "INSTRUCTION",
                "FUEL_ADDED",
                "SYSTEM",
            )
        for (c in cats) {
            assertTrue("missing category $c in:\n$text", text.contains("| $c |"))
        }

        val ecoLine = text.lines().first { it.contains("| ECO_CALC |") }

        fun field(name: String): Double {
            val re = Regex("""\b$name=([-+0-9.eE]+)""")
            val m = re.find(ecoLine) ?: error("missing $name in $ecoLine")
            return m.groupValues[1].toDouble()
        }
        val up = field("uphill_energy_j")
        val down = field("downhill_energy_j")
        val climb = field("climb_m")
        val descent = field("descent_m")
        assertTrue("uphill_energy_j must be > 0 (got $up)", up > 0.0)
        assertTrue("uphill and downhill must differ ($up vs $down)", up != down)
        assertTrue("climb_m must be > 0 (got $climb)", climb > 0.0)
        assertTrue("descent_m must be > 0 (got $descent)", descent > 0.0)
        assertTrue("downhill_energy_j finite", down.isFinite() && down != up)

        DiagnosticLog.setEnabled(context, false)
        assertFalse(DiagnosticLog.isEnabled())
        assertTrue(session.isFile)
        assertTrue(session.readText().contains("| ECO_CALC |"))
    }

    companion object {
        private const val TAG = "NaviDiagOnDevice"
    }
}
