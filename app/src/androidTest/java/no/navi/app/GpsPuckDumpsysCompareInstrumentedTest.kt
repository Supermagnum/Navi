package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import androidx.test.rule.GrantPermissionRule
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Live diagnostic: compare app puck ([NaviMapTestHooks.lastGpsLat]/Lon]) to
 * `dumpsys location` at the same moment. Investigation only — no product assert
 * beyond "we captured comparable samples".
 */
@RunWith(AndroidJUnit4::class)
class GpsPuckDumpsysCompareInstrumentedTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    @get:Rule
    val permissionRule: GrantPermissionRule =
        GrantPermissionRule.grant(
            android.Manifest.permission.ACCESS_FINE_LOCATION,
            android.Manifest.permission.ACCESS_COARSE_LOCATION,
        )

    private lateinit var outDir: File

    @Before
    fun setUp() {
        outDir =
            File(
                InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,
                "navi_gps_puck_diag",
            ).also {
                it.mkdirs()
                it.listFiles()?.forEach { f -> f.delete() }
            }
        shell("mkdir -p /data/local/tmp/navi_gps_puck_diag && chmod 777 /data/local/tmp/navi_gps_puck_diag")
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.hideSearchChrome = true
        // Do NOT disable follow or ignore live GPS — this is a live diagnostic.
        NaviMapTestHooks.ignoreLiveGpsFixes = false
        NaviMapTestHooks.disableGpsFollow = false
        NaviMapTestHooks.followGps = true
    }

    @After
    fun tearDown() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        runCatching { activityRule.finishActivity() }
    }

    @Test
    fun sample_cold_launch_settle_and_report() {
        activityRule.launchActivity(null)

        val report = StringBuilder()
        report.appendLine("device=${android.os.Build.MODEL} sdk=${android.os.Build.VERSION.SDK_INT}")
        report.appendLine("utc=${java.time.Instant.now()}")

        // Cold: sample ASAP after launch (may still be lastKnown).
        Thread.sleep(2_000)
        appendSample(report, "t+2s_cold")

        // Settle: wait for live updates.
        Thread.sleep(15_000)
        appendSample(report, "t+17s_settled")

        Thread.sleep(15_000)
        appendSample(report, "t+32s_settled2")

        val text = report.toString()
        File(outDir, "report.txt").writeText(text)
        shell("cp ${File(outDir, "report.txt").absolutePath} /data/local/tmp/navi_gps_puck_diag/report.txt")
        Log.i(TAG, text)
        assertTrue("report written", File(outDir, "report.txt").length() > 50)
    }

    private fun appendSample(
        report: StringBuilder,
        label: String,
    ) {
        val dumpsys = shellCapture("dumpsys location")
        val parsed = parseDumpsysLastFix(dumpsys)
        val appLat = NaviMapTestHooks.lastGpsLat
        val appLon = NaviMapTestHooks.lastGpsLon
        val appAlt = NaviMapTestHooks.lastHudAltitudeM
        val street = NaviMapTestHooks.lastCurrentStreet
        val follow = NaviMapTestHooks.followGps
        val camLat = NaviMapTestHooks.lastCameraLat
        val camLon = NaviMapTestHooks.lastCameraLon

        report.appendLine("--- $label ---")
        report.appendLine(
            "app_puck lat=${fmt(appLat)} lon=${fmt(appLon)} altHud=$appAlt " +
                "street='$street' followGps=$follow",
        )
        report.appendLine("app_camera lat=${fmt(camLat)} lon=${fmt(camLon)}")
        if (parsed != null) {
            val dist =
                if (!appLat.isNaN() && !appLon.isNaN()) {
                    haversineM(appLat, appLon, parsed.lat, parsed.lon)
                } else {
                    Double.NaN
                }
            report.appendLine(
                "dumpsys provider=${parsed.provider} lat=${parsed.lat} lon=${parsed.lon} " +
                    "ageMs=${parsed.ageMs} accM=${parsed.accuracyM} alt=${parsed.altitudeM} " +
                    "dist_app_vs_dumpsys_m=${fmt(dist)}",
            )
        } else {
            report.appendLine("dumpsys: no parseable last-fix (see raw snippet)")
            report.appendLine(dumpsysSnippet(dumpsys))
        }
    }

    private fun dumpsysSnippet(raw: String): String =
        raw
            .lineSequence()
            .filter { line ->
                val lastLoc = line.contains("last location", ignoreCase = true)
                val locBracket = line.contains("Location[")
                val gpsLoc =
                    line.contains("gps", ignoreCase = true) &&
                        line.contains("Location")
                lastLoc || locBracket || gpsLoc
            }.take(12)
            .joinToString("\n")

    private data class DumpsysFix(
        val provider: String,
        val lat: Double,
        val lon: Double,
        val ageMs: Long?,
        val accuracyM: Float?,
        val altitudeM: Double?,
    )

    private fun parseDumpsysLastFix(raw: String): DumpsysFix? {
        // Prefer GPS provider last location lines, e.g.:
        // last location=Location[gps 60.79...,11.06... hAcc=...]
        val gpsRe =
            Regex(
                """last location=Location\[(gps|network|fused|passive)\s+(-?\d+\.\d+),(-?\d+\.\d+)([^\]]*)\]""",
                RegexOption.IGNORE_CASE,
            )
        val matches = gpsRe.findAll(raw).toList()
        val preferred =
            matches.firstOrNull { it.groupValues[1].equals("gps", true) }
                ?: matches.firstOrNull { it.groupValues[1].equals("fused", true) }
                ?: matches.firstOrNull()
                ?: return null
        val provider = preferred.groupValues[1]
        val lat = preferred.groupValues[2].toDouble()
        val lon = preferred.groupValues[3].toDouble()
        val rest = preferred.groupValues[4]
        val accMatch = Regex("""hAcc=([\d.]+)""").find(rest)
        val acc = accMatch?.groupValues?.get(1)?.toFloatOrNull()
        val altMatch = Regex("""alt=([-\d.]+)""").find(rest)
        val alt = altMatch?.groupValues?.get(1)?.toDoubleOrNull()
        val ageMatch = Regex("""(\d+)\s*ms\s*ago""").find(raw)
        val age = ageMatch?.groupValues?.get(1)?.toLongOrNull()
        return DumpsysFix(provider, lat, lon, age, acc, alt)
    }

    private fun haversineM(
        lat1: Double,
        lon1: Double,
        lat2: Double,
        lon2: Double,
    ): Double {
        val r = 6_371_000.0
        val p1 = Math.toRadians(lat1)
        val p2 = Math.toRadians(lat2)
        val dp = Math.toRadians(lat2 - lat1)
        val dl = Math.toRadians(lon2 - lon1)
        val a = sin(dp / 2) * sin(dp / 2) + cos(p1) * cos(p2) * sin(dl / 2) * sin(dl / 2)
        return 2 * r * atan2(sqrt(a), sqrt(1 - a))
    }

    private fun fmt(v: Double): String = if (v.isNaN()) "NaN" else "%.6f".format(v)

    private fun shell(cmd: String) {
        val pfd = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
        java.io.FileInputStream(pfd.fileDescriptor).use { input ->
            val buf = ByteArray(4096)
            while (input.read(buf) >= 0) {
            }
        }
        pfd.close()
    }

    private fun shellCapture(cmd: String): String {
        val pfd = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
        val text =
            java.io.FileInputStream(pfd.fileDescriptor).use { input ->
                input.readBytes().toString(Charsets.UTF_8)
            }
        pfd.close()
        return text
    }

    companion object {
        private const val TAG = "GpsPuckDumpsys"
    }
}
