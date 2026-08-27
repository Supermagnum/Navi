package no.navi.app

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.FixMethodOrder
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.ensureIndexedMaps
import uniffi.navi.indexedMapsStatus
import uniffi.navi.planCarRouteAt
import java.io.File

/**
 * Status-only against the real Ostlandet install (v2 packs already on device).
 */
@RunWith(AndroidJUnit4::class)
class IndexedPackV3MigrationStatusInstrumentedTest {
    private fun dataDir(): File = NaviAppData.resolve(InstrumentationRegistry.getInstrumentation().targetContext)

    @Test
    fun ostlandetV2PackReportsVersionMismatchViaExistingStatusApi() {
        val pbf = File(dataDir(), "ostlandet-latest.osm.pbf")
        assumeTrue("Ostlandet PBF missing on device", pbf.isFile && pbf.length() > 100_000_000L)
        val status = indexedMapsStatus(pbf.absolutePath, dataDir().absolutePath).trim()
        assertTrue(
            "unexpected indexed status=$status",
            status == "version_mismatch" || status == "ready",
        )
        if (status == "ready") {
            val man = File(dataDir(), "ostlandet-latest.navi-manifest.json").readText()
            assertTrue(
                "ready status must be graph_format_version 3: $man",
                man.contains("\"graph_format_version\": 3"),
            )
        } else {
            val man = File(dataDir(), "ostlandet-latest.navi-manifest.json").readText()
            assertTrue(
                "expected on-device Ostlandet pack still v2: $man",
                man.contains("\"graph_format_version\": 2"),
            )
        }
    }
}

/**
 * UI-only: Tools rebuild button is the M4 affordance (same status API as UniFFI).
 */
@RunWith(AndroidJUnit4::class)
class IndexedPackV3MigrationToolsUiInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private fun dataDir(): File = NaviAppData.resolve(InstrumentationRegistry.getInstrumentation().targetContext)

    private fun waitForToolsButton(timeoutMs: Long = 60_000) {
        val deadline = System.currentTimeMillis() + timeoutMs
        var last: Throwable? = null
        while (System.currentTimeMillis() < deadline) {
            try {
                composeRule.waitForIdle()
                composeRule.onNodeWithTag("btn_tools", useUnmergedTree = true).assertExists()
                return
            } catch (t: Throwable) {
                last = t
                Thread.sleep(500)
            }
        }
        throw IllegalStateException("btn_tools never appeared", last)
    }

    @Test
    fun toolsRebuildButtonVisibleAndStatusApiSeesMismatchOrReady() {
        val pbf = File(dataDir(), "ostlandet-latest.osm.pbf")
        assumeTrue(pbf.isFile)
        waitForToolsButton()
        composeRule
            .onNodeWithTag("btn_tools", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        composeRule.onNodeWithTag("tools_menu", useUnmergedTree = true).assertIsDisplayed()
        composeRule
            .onNodeWithTag("btn_rebuild_indexed_maps", useUnmergedTree = true)
            .performScrollTo()
            .assertIsDisplayed()
        val before = indexedMapsStatus(pbf.absolutePath, dataDir().absolutePath).trim()
        assertTrue(
            "Tools rebuild path must see mismatch or ready, got $before",
            before == "version_mismatch" || before == "ready",
        )
    }
}

/**
 * On-device v2→v3 rebuild + Friisvegen pack-hit plan.
 *
 * Uses a Friisvegen corridor extract (fits SM-P613 RAM). Full Ostlandet
 * `ensureIndexedMaps` currently OOMs on this tablet; corridor still exercises
 * the same M4 UniFFI path (`indexedMapsStatus` / `ensureIndexedMaps`) from an
 * already-local PBF with no Geofabrik re-fetch.
 */
@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class IndexedPackV3MigrationConvertInstrumentedTest {
    private fun dataDir(): File = NaviAppData.resolve(InstrumentationRegistry.getInstrumentation().targetContext)

    private fun stagedSeed(): File = File("/data/local/tmp/navi_fixtures/friisvegen-v2-seed")

    private fun installStaleV2Seed() {
        val staged = stagedSeed()
        assumeTrue(
            "staged Friisvegen v2 seed missing at ${staged.absolutePath}",
            staged.isDirectory && File(staged, "friisvegen-corridor.osm.pbf").isFile,
        )
        val dir = dataDir()
        for (name in staged.list() ?: emptyArray()) {
            val src = File(staged, name)
            if (!src.isFile) continue
            src.copyTo(File(dir, name), overwrite = true)
        }
        // Copying changes mtime; refresh fingerprint so status is VersionMismatch
        // (not StalePbf). Keep graph_format_version=2 to simulate a v2-era pack.
        val pbf = File(dir, "friisvegen-corridor.osm.pbf")
        val manFile = File(dir, "friisvegen-corridor.navi-manifest.json")
        val man =
            org.json.JSONObject(manFile.readText()).apply {
                put("pbf_size_bytes", pbf.length())
                put("pbf_modified_unix_secs", pbf.lastModified() / 1000L)
                put("graph_format_version", 2)
            }
        manFile.writeText(man.toString(2) + "\n")
    }

    @Test
    fun a_rebuildFromLocalPbfProducesV3WithoutRefetch() {
        installStaleV2Seed()
        val dir = dataDir()
        val pbf = File(dir, "friisvegen-corridor.osm.pbf")
        assertTrue(pbf.isFile)
        val pbfBeforeLen = pbf.length()
        val pbfBeforeMtime = pbf.lastModified()

        val before = indexedMapsStatus(pbf.absolutePath, dir.absolutePath).trim()
        assertTrue(
            "deliberate stale seed must report version_mismatch, got $before",
            before == "version_mismatch",
        )
        val manBefore = File(dir, "friisvegen-corridor.navi-manifest.json").readText()
        assertTrue(manBefore.contains("\"graph_format_version\": 2"))

        val elev = File(dir, "elevation").takeIf { it.isDirectory }?.absolutePath
        // Identical UniFFI call the Tools "Rebuild indexed maps (local PBF)" button uses
        // (button picks the first *.osm.pbf; we call with the corridor path explicitly).
        val report = ensureIndexedMaps(pbf.absolutePath, dir.absolutePath, elev)
        assertTrue("ensureIndexedMaps failed:\n$report", report.contains("PASS"))
        assertTrue(
            "expected convert (cache_hit=false) on version mismatch:\n$report",
            report.contains("cache_hit=false"),
        )
        val after = indexedMapsStatus(pbf.absolutePath, dir.absolutePath).trim()
        assertTrue("after status=$after report=$report", after == "ready")

        assertTrue(pbf.isFile)
        assertTrue(
            "PBF size changed (possible re-download): before=$pbfBeforeLen after=${pbf.length()}",
            pbf.length() == pbfBeforeLen,
        )
        assertTrue(
            "PBF mtime changed (possible re-download): before=$pbfBeforeMtime after=${pbf.lastModified()}",
            pbf.lastModified() == pbfBeforeMtime,
        )

        val man = File(dir, "friisvegen-corridor.navi-manifest.json").readText()
        assertTrue("manifest not v3: $man", man.contains("\"graph_format_version\": 3"))
    }

    @Test
    fun b_friisvegenSeasonalClosureViaPackHitAfterV3() {
        val dir = dataDir()
        val pbf = File(dir, "friisvegen-corridor.osm.pbf")
        assumeTrue(pbf.isFile)
        val status = indexedMapsStatus(pbf.absolutePath, dir.absolutePath).trim()
        assumeTrue("packs must be ready/v3 (run a_ first); status=$status", status == "ready")
        val man = File(dir, "friisvegen-corridor.navi-manifest.json").readText()
        assertTrue(man.contains("\"graph_format_version\": 3"))

        val elevDir = File(dir, "elevation").absolutePath
        val cacheDir = File(dir, "graph-cache-friisvegen-v3").also { it.mkdirs() }.absolutePath
        val vehicle =
            FfiVehicleLimits(
                axleWeightKg = null,
                bogieWeightKg = null,
                heightM = null,
                widthM = null,
                lengthM = null,
                totalWeightKg = null,
            )
        // On-road Friisvegen endpoints from the corridor pack (Nov–Jun closed).
        val startLat = 61.562531
        val startLon = 10.307516
        val endLat = 61.633868
        val endLon = 10.424381

        val summer =
            planCarRouteAt(
                pbf.absolutePath,
                elevDir,
                cacheDir,
                startLat,
                startLon,
                endLat,
                endLon,
                false,
                TravelProfile.CAR,
                false,
                false,
                false,
                vehicle,
                false,
                "2026-07-15T12:00:00",
                dataDir = "",
            )
        assertTrue("summer plan failed:\n${summer.report}", summer.report.contains("PASS"))
        assertTrue(
            "summer expected pack_hit=true:\n${summer.report}",
            summer.report.contains("pack_hit=true"),
        )
        assertTrue(
            "summer should not exclude Friisvegen:\n${summer.report}",
            summer.report.contains("seasonal_closure_excluded_edges=0"),
        )

        val winter =
            planCarRouteAt(
                pbf.absolutePath,
                elevDir,
                cacheDir,
                startLat,
                startLon,
                endLat,
                endLon,
                false,
                TravelProfile.CAR,
                false,
                false,
                false,
                vehicle,
                false,
                "2026-01-15T12:00:00",
                dataDir = "",
            )
        // Pack-hit + seasonal filter: closed mountain road is no longer snap/routable.
        assertTrue(
            "winter expected pack_hit=true:\n${winter.report}",
            winter.report.contains("pack_hit=true"),
        )
        val seasonalLine =
            winter.report.lineSequence().firstOrNull {
                it.startsWith("seasonal_closure_excluded_edges=")
            }
        assertTrue("missing seasonal_closure line:\n${winter.report}", seasonalLine != null)
        val excluded =
            seasonalLine!!
                .removePrefix("seasonal_closure_excluded_edges=")
                .trim()
                .toIntOrNull() ?: -1
        assertTrue(
            "expected seasonal closures in pack-hit graph, got $excluded:\n${winter.report}",
            excluded > 0,
        )
        assertTrue(
            "winter must apply closures (no open summer path):\n${winter.report}",
            winter.report.contains("FAIL:") ||
                (winter.report.contains("PASS") && winter.distanceKm > summer.distanceKm + 1.0),
        )
    }
}
