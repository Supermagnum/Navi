package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.FixMethodOrder
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
 * Full Østlandet v2→v3 tiled rebuild on device (no MainActivity / MapLibre).
 * Confirms pack-hit + seasonal closures on the installed region pack.
 */
@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class OstlandetV3TiledRebuildInstrumentedTest {
    private fun dataDir(): File = NaviAppData.resolve(InstrumentationRegistry.getInstrumentation().targetContext)

    private fun ostlandetPbf(): File? {
        val p = File(dataDir(), "ostlandet-latest.osm.pbf")
        return p.takeIf { it.isFile && it.length() > 100_000_000L }
    }

    @Test
    fun a_fallbackUsableWhilePacksNotReady() {
        val pbf = ostlandetPbf()
        assumeTrue("Ostlandet PBF missing", pbf != null)
        val dir = dataDir()
        val before = indexedMapsStatus(pbf!!.absolutePath, dir.absolutePath).trim()
        assumeTrue(
            "Need packs not ready for fallback proof (got $before)",
            before == "version_mismatch" || before == "stale_pbf" || before == "missing",
        )
        val elevDir = File(dir, "elevation").absolutePath
        val cacheDir = File(dir, "graph-cache-ostlandet-fallback").also { it.mkdirs() }.absolutePath
        val vehicle =
            FfiVehicleLimits(
                axleWeightKg = null,
                bogieWeightKg = null,
                heightM = null,
                widthM = null,
                lengthM = null,
                totalWeightKg = null,
            )
        // Short Oslo-area OD: must plan via bbox/PBF fallback before packs exist.
        val r =
            planCarRouteAt(
                pbf.absolutePath,
                elevDir,
                cacheDir,
                59.9139,
                10.7522,
                59.9333,
                10.7166,
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
        android.util.Log.i("OstlandetV3Tiled", "FALLBACK ${r.report}")
        assertTrue("fallback plan failed:\n${r.report}", r.report.contains("PASS"))
        assertTrue(
            "expected pack_hit=false while packs not ready:\n${r.report}",
            r.report.contains("pack_hit=false"),
        )
    }

    @Test
    fun b_rebuildOstlandetToV3TiledWithoutRefetch() {
        val pbf = ostlandetPbf()
        assumeTrue("Ostlandet PBF missing", pbf != null)
        val dir = dataDir()
        val beforeLen = pbf!!.length()
        val beforeMtime = pbf.lastModified()
        val before = indexedMapsStatus(pbf.absolutePath, dir.absolutePath).trim()
        assumeTrue(
            "Need stale/mismatch Ostlandet packs (got $before)",
            before == "version_mismatch" || before == "stale_pbf" || before == "missing",
        )

        // No elev_dir: tiled convert skips region-wide DEM warm by design.
        val t0 = System.nanoTime()
        val report = ensureIndexedMaps(pbf.absolutePath, dir.absolutePath, null)
        val elapsedMs = (System.nanoTime() - t0) / 1_000_000L
        android.util.Log.i(
            "OstlandetV3Tiled",
            "ensureIndexedMaps elapsed_ms=$elapsedMs report=$report",
        )
        assertTrue("convert failed:\n$report", report.contains("PASS"))
        assertTrue("expected real convert:\n$report", report.contains("cache_hit=false"))
        assertTrue(
            "expected tiled convert:\n$report",
            report.contains("graph_tiles=") &&
                !report.contains("graph_tiles=0\n"),
        )
        val after = indexedMapsStatus(pbf.absolutePath, dir.absolutePath).trim()
        assertTrue("after=$after report=$report", after == "ready")
        assertTrue(pbf.length() == beforeLen)
        assertTrue(pbf.lastModified() == beforeMtime)

        val man = File(dir, "ostlandet-latest.navi-manifest.json").readText()
        assertTrue(man.contains("\"graph_format_version\": 6"))
        assertTrue("manifest missing graph_tiles:\n$man", man.contains("graph_tiles"))
        android.util.Log.i("OstlandetV3Tiled", "MANIFEST $man")
    }

    @Test
    fun c_friisvegenSeasonalViaOstlandetPackHit() {
        val pbf = ostlandetPbf()
        assumeTrue(pbf != null)
        val dir = dataDir()
        assumeTrue(
            indexedMapsStatus(pbf!!.absolutePath, dir.absolutePath).trim() == "ready",
        )
        val elevDir = File(dir, "elevation").absolutePath
        val cacheDir = File(dir, "graph-cache-ostlandet-friis-v3").also { it.mkdirs() }.absolutePath
        val vehicle =
            FfiVehicleLimits(
                axleWeightKg = null,
                bogieWeightKg = null,
                heightM = null,
                widthM = null,
                lengthM = null,
                totalWeightKg = null,
            )
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
        android.util.Log.i("OstlandetV3Tiled", "SUMMER ${summer.report}")
        assertTrue("summer:\n${summer.report}", summer.report.contains("PASS"))
        assertTrue("summer pack_hit:\n${summer.report}", summer.report.contains("pack_hit=true"))
        assertTrue(
            "summer seasonal:\n${summer.report}",
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
        android.util.Log.i("OstlandetV3Tiled", "WINTER ${winter.report}")
        assertTrue("winter pack_hit:\n${winter.report}", winter.report.contains("pack_hit=true"))
        val seasonal =
            winter.report
                .lineSequence()
                .first { it.startsWith("seasonal_closure_excluded_edges=") }
                .removePrefix("seasonal_closure_excluded_edges=")
                .trim()
                .toInt()
        assertTrue("winter seasonal=$seasonal:\n${winter.report}", seasonal > 0)
        assertTrue(
            "winter must apply closures:\n${winter.report}",
            winter.report.contains("FAIL:") ||
                (winter.report.contains("PASS") && winter.distanceKm > summer.distanceKm + 1.0),
        )
    }
}
