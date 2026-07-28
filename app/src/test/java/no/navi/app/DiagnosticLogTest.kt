package no.navi.app

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.io.File

/**
 * JVM unit tests for structured diagnostic session logs.
 *
 * Covers format, per-category lines, toggle no-op when off, mid-session close,
 * GPS rate-limiting, and session-file retention.
 */
class DiagnosticLogTest {
    private lateinit var root: File
    private lateinit var filesDir: File

    @Before
    fun setUp() {
        root = File.createTempFile("navi_diag_", "").also {
            it.delete()
            it.mkdirs()
        }
        filesDir = File(root, "files").also { it.mkdirs() }
        DiagnosticLog.setLogsDirForTest(File(filesDir, "diagnostic_logs"))
        DiagnosticLog.applyEnabled(filesDir, false)
    }

    @After
    fun tearDown() {
        DiagnosticLog.applyEnabled(filesDir, false)
        DiagnosticLog.setLogsDirForTest(null)
        root.deleteRecursively()
    }

    @Test
    fun formatLine_isPipeDelimitedUtc() {
        val line =
            DiagnosticLog.formatLine(
                DiagnosticLog.Category.TOGGLE,
                mapOf("name" to "eco_mode", "value" to true, "profile" to "Car"),
                epochMs = 1_753_711_805_123L,
            )
        assertTrue(line.contains(" | TOGGLE | "))
        assertTrue(line.contains("name=eco_mode"))
        assertTrue(line.contains("value=true"))
        assertTrue(line.startsWith("20"))
        assertTrue(line.contains("Z | "))
    }

    @Test
    fun disabled_isGenuineNoOp_noFileCreated() {
        DiagnosticLog.applyEnabled(filesDir, false)
        DiagnosticLog.logToggle("eco_mode", true)
        DiagnosticLog.logSettingSaved("k", 1)
        DiagnosticLog.logGps(60.0, 11.0, 100.0, 4f, 8, "3D")
        assertNull(DiagnosticLog.currentSessionFile())
        assertTrue(DiagnosticLog.listSessionFiles(filesDir).isEmpty())
        assertFalse(DiagnosticLog.isEnabled())
    }

    @Test
    fun enable_createsSessionFile_disable_closesCleanly() {
        DiagnosticLog.applyEnabled(filesDir, true)
        assertTrue(DiagnosticLog.isEnabled())
        val session = DiagnosticLog.currentSessionFile()
        assertNotNull(session)
        assertTrue(session!!.name.startsWith("navi_session_"))
        assertTrue(session.name.endsWith(".log"))

        DiagnosticLog.logToggle("eco_mode", true, mapOf("profile" to "Car"))
        DiagnosticLog.applyEnabled(filesDir, false)
        assertFalse(DiagnosticLog.isEnabled())
        assertNull(DiagnosticLog.currentSessionFile())
        assertTrue(session.isFile)
        val text = session.readText()
        assertTrue(text.contains("| TOGGLE |"))
        assertTrue(text.contains("name=eco_mode"))
        // Closed cleanly: no half-open writer left; file remains readable.
        assertTrue(text.lines().filter { it.isNotBlank() }.all { it.contains(" | ") })
    }

    @Test
    fun eachCategory_writesExpectedLine() {
        DiagnosticLog.applyEnabled(filesDir, true)
        DiagnosticLog.logGps(60.5621914, 11.2561239, 214.7, 4.2f, 9, "3D")
        DiagnosticLog.logToggle("eco_mode", true, mapOf("profile" to "Car"))
        DiagnosticLog.logSettingSaved("truck_max_weekly_driving_hours", 56.0)
        DiagnosticLog.logRoutePlanStart("Car", 60.562, 11.256, 61.851, 10.234)
        DiagnosticLog.write(
            DiagnosticLog.Category.ROUTE_PLAN,
            mapOf(
                "status" to "complete",
                "distance_km" to 190.73,
                "eta_min" to 127.0,
                "breaks" to 0,
            ),
        )
        DiagnosticLog.logEcoCalc(
            profile = "Car",
            ecoMode = true,
            climbM = 2011.0,
            descentM = 1476.0,
            uphillJ = 87_200_000.0,
            downhillJ = 7_738_589.0,
            regenCreditJ = 0.0,
            netJ = 94_938_589.0,
        )
        DiagnosticLog.write(
            DiagnosticLog.Category.POI_FOUND,
            mapOf(
                "category" to "Water",
                "name" to "Dolkjoylla spring",
                "dist_from_route_m" to 340.0,
            ),
        )
        DiagnosticLog.write(
            DiagnosticLog.Category.PAUSE_PLANNED,
            mapOf(
                "kind" to "daily_rest",
                "position_km" to 289.3,
                "duration_min" to 45.0,
                "lat" to 62.344,
                "lon" to 11.467,
            ),
        )
        DiagnosticLog.onManeuverProgress(3, 8, "turn_left", "Kirkegata", 450.0)
        DiagnosticLog.logInstructionCompleted(3, 8)
        DiagnosticLog.logFuelAdded(32.5, "liters", 78.0)
        DiagnosticLog.maybeLogSystem(filesDir, nowMs = 1L)

        val text = DiagnosticLog.currentSessionFile()!!.readText()
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
            assertTrue("missing $c in:\n$text", text.contains("| $c |"))
        }
        assertTrue(text.contains("uphill_energy_j="))
        assertTrue(text.contains("downhill_energy_j="))
        assertTrue(text.contains("regen_credit_j="))
        assertTrue(text.contains("climb_m="))
        assertTrue(text.contains("descent_m="))
        // Noise check: only known categories.
        for (line in text.lines().filter { it.isNotBlank() }) {
            val cat = line.split("|").getOrNull(1)?.trim()
            assertNotNull(line, cat)
            assertTrue("unexpected category $cat", cat in cats)
        }
    }

    @Test
    fun gps_rateLimited_notEveryCallback() {
        DiagnosticLog.applyEnabled(filesDir, true)
        DiagnosticLog.logGps(60.0, 11.0, 100.0, 5f, 8, "3D", nowMs = 1_000L)
        DiagnosticLog.logGps(60.00001, 11.00001, 100.0, 5f, 8, "3D", nowMs = 1_500L)
        DiagnosticLog.logGps(60.00002, 11.00002, 100.0, 5f, 8, "3D", nowMs = 2_000L)
        val mid = DiagnosticLog.currentSessionFile()!!.readText()
        assertEquals(1, mid.lines().count { it.contains("| GPS |") })

        DiagnosticLog.logGps(60.0, 11.0, 100.0, 5f, 8, "3D", nowMs = 1_000L + DiagnosticLog.GPS_MIN_INTERVAL_MS)
        val later = DiagnosticLog.currentSessionFile()!!.readText()
        assertEquals(2, later.lines().count { it.contains("| GPS |") })
    }

    @Test
    fun retention_keepsOnlyLastNSessions() {
        repeat(12) { i ->
            DiagnosticLog.applyEnabled(filesDir, true)
            DiagnosticLog.logToggle("eco_mode", i % 2 == 0)
            // Force distinct mtime / new file by closing then reopening.
            DiagnosticLog.applyEnabled(filesDir, false)
            Thread.sleep(15)
        }
        DiagnosticLog.applyEnabled(filesDir, true)
        val files = DiagnosticLog.listSessionFiles(filesDir)
        assertTrue("expected <= 10 session files, got ${files.size}", files.size <= 10)
        DiagnosticLog.applyEnabled(filesDir, false)
    }

    @Test
    fun ecoFromReport_parsesSeparateClimbDescent() {
        DiagnosticLog.applyEnabled(filesDir, true)
        val report =
            """
            profile=Car; use_eco=true
            eco_climb_m=2011.0; eco_descent_m=1476.0; eco_uphill_j=87200000; eco_downhill_j=7738589; eco_regen_credit_j=0; eco_net_j=94938589
            """.trimIndent()
        DiagnosticLog.logEcoFromReport(report)
        val text = DiagnosticLog.currentSessionFile()!!.readText()
        assertTrue(text.contains("| ECO_CALC |"))
        assertTrue(text.contains("climb_m=2011"))
        assertTrue(text.contains("descent_m=1476"))
        assertTrue(text.contains("uphill_energy_j=87200000"))
        assertTrue(text.contains("downhill_energy_j=7738589"))
    }

    @Test
    fun isPauseKind_filtersPois() {
        assertTrue(DiagnosticLog.isPauseKind("daily_rest"))
        assertTrue(DiagnosticLog.isPauseKind("tent"))
        assertFalse(DiagnosticLog.isPauseKind("Water"))
        assertFalse(DiagnosticLog.isPauseKind("spring"))
    }
}
