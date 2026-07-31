package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * After wipe/reinstall, staged fixtures remain under `/data/local/tmp/navi_fixtures`
 * while app `files/pmtiles` is empty — must surface a clear mismatch, not silent Liberty.
 */
@RunWith(AndroidJUnit4::class)
class OfflineDataMismatchInstrumentedTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context
    private lateinit var dataDir: File

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        // Simulate post-install empty app storage while host staging still has Ostlandet.
        File(dataDir, "pmtiles").deleteRecursively()
        MapHudPrefs.rememberDownloadedPmtilesRegion(context, "europe_norway_ostlandet")
        MapHudPrefs.saveOptIn3d(context, true)
        NaviMapTestHooks.forceOnlineBasemap = false
    }

    @Test
    fun inspect_and_status_chip_signal_missing_offline_data() {
        val staged = OfflineDataIntegrity.STAGED_FIXTURES_DIR
        assertTrue(
            "need staged basemap at ${staged.absolutePath}",
            File(staged, OfflinePmtilesBootstrap.DEFAULT_BASEMAP).isFile,
        )

        val report = OfflineDataIntegrity.inspect(context, dataDir)
        assertTrue(report.hasIssue)
        assertTrue(report.canRestoreFromStaging)
        val msg = report.userMessage()
        assertTrue(!msg.isNullOrBlank())
        assertTrue(
            "message should mention offline/reinstall: $msg",
            msg!!.contains("Offline data", ignoreCase = true) ||
                msg.contains("reinstall", ignoreCase = true) ||
                msg.contains("restore", ignoreCase = true),
        )

        val resolved =
            BasemapStyleResolver.resolve(
                context = context,
                dataDir = dataDir,
                lat = 60.79,
                lon = 11.08,
                prefer3d = true,
                vulkanAvailable = true,
            )
        // No local files → online fallback, but note must not be the silent Liberty default alone.
        assertTrue(resolved.kind.name.startsWith("Online"))
        assertTrue(!resolved.note.isNullOrBlank())
        assertTrue(
            "resolver note should explain missing offline data: ${resolved.note}",
            resolved.note!!.contains("Offline data", ignoreCase = true) ||
                resolved.note!!.contains("restore", ignoreCase = true) ||
                resolved.note!!.contains("re-download", ignoreCase = true) ||
                resolved.note!!.contains("reinstall", ignoreCase = true),
        )
        assertFalse(
            "must not look like a healthy offline Protomaps session",
            resolved.kind == BasemapStyleResolver.StyleKind.OfflineProtomaps,
        )

        activityRule.launchActivity(null)
        Thread.sleep(3_000)
        val deadline = System.currentTimeMillis() + 30_000
        var chip = ""
        while (System.currentTimeMillis() < deadline) {
            chip = NaviMapTestHooks.lastBasemapKind
            // Status is pushed via onStyleNote into Activity `status`; kind stays Online*.
            if (chip.startsWith("Online")) break
            Thread.sleep(400)
        }
        assertTrue("expected online fallback kind, got $chip", chip.startsWith("Online"))
    }
}
