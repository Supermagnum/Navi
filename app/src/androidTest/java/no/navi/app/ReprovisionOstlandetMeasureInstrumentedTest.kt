package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.FixMethodOrder
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.ensureIndexedMaps
import uniffi.navi.ensurePlaceIndex
import uniffi.navi.indexedMapsStatus
import uniffi.navi.planCarRoute
import uniffi.navi.pmtilesPlanetUrl
import uniffi.navi.pmtilesQueueDemRegion
import uniffi.navi.pmtilesQueueRegion
import uniffi.navi.pmtilesRunJob
import java.io.File

/**
 * Re-provision full Østlandet (PBF + place index + v4 packs + full basemap
 * PMTiles) then measure Espa→Atnbrufossen with pack_hit required.
 *
 * Does not use [OfflinePmtilesBootstrap] for the basemap — staged fixtures are
 * a truncated ~184 MB extract; a real Tools-style range-extract is required
 * for full map labels/POIs.
 */
@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class ReprovisionOstlandetMeasureInstrumentedTest {
    private fun dataDir(): File = NaviAppData.resolve(InstrumentationRegistry.getInstrumentation().targetContext)

    private fun staged(): File = File("/data/local/tmp/navi_fixtures")

    private fun ensureOstlandetPbf(): File {
        val dir = dataDir()
        val dest = File(dir, "ostlandet-latest.osm.pbf")
        if (dest.isFile && dest.length() > 100_000_000L) return dest
        val src = File(staged(), "ostlandet-latest.osm.pbf")
        check(src.isFile && src.length() > 100_000_000L) {
            "push ostlandet-latest.osm.pbf to /data/local/tmp/navi_fixtures"
        }
        src.copyTo(dest, overwrite = true)
        return dest
    }

    /** Full basemap is ~1 GB; staged fixture (~184 MB) is treated as truncated. */
    private fun ensureFullBasemapPmtiles() {
        val dir = dataDir()
        val pmDir = File(dir, "pmtiles").also { it.mkdirs() }
        val basemap = File(pmDir, "europe_norway_ostlandet.pmtiles")
        val dem = File(pmDir, "europe_norway_ostlandet_dem.pmtiles")
        val minFullBytes = 500_000_000L
        if (basemap.isFile &&
            basemap.length() >= minFullBytes &&
            dem.isFile &&
            dem.length() > 1_000_000_000L
        ) {
            Log.i(TAG, "basemap already full bytes=${basemap.length()} dem=${dem.length()}")
            MapHudPrefs.rememberDownloadedPmtilesRegion(
                InstrumentationRegistry.getInstrumentation().targetContext,
                "europe_norway_ostlandet",
            )
            return
        }
        // Prefer a host-pushed full basemap if present under staging.
        val stagedFull = File(staged(), "europe_norway_ostlandet_full.pmtiles")
        if (stagedFull.isFile && stagedFull.length() >= minFullBytes) {
            stagedFull.copyTo(basemap, overwrite = true)
            Log.i(TAG, "copied staged full basemap bytes=${basemap.length()}")
        } else {
            Log.i(TAG, "downloading full Ostlandet basemap via pmtilesQueueRegion…")
            val job = pmtilesQueueRegion(dir.absolutePath, "europe/norway/ostlandet", pmtilesPlanetUrl())
            check(job.id.isNotBlank()) { "pmtilesQueueRegion empty id" }
            val done = pmtilesRunJob(dir.absolutePath, job.id)
            Log.i(TAG, "basemap job status=${done.status} bytes=${basemap.length()}")
            check(done.status == "completed") { "basemap download failed: ${done.status}" }
            check(basemap.isFile && basemap.length() >= minFullBytes) {
                "basemap still truncated after download: ${basemap.length()}"
            }
        }
        if (!dem.isFile || dem.length() < 1_000_000_000L) {
            val stagedDem = File(staged(), "europe_norway_ostlandet_dem.pmtiles")
            if (stagedDem.isFile && stagedDem.length() > 1_000_000_000L) {
                stagedDem.copyTo(dem, overwrite = true)
            } else {
                Log.i(TAG, "downloading Ostlandet DEM…")
                val demJob = pmtilesQueueDemRegion(dir.absolutePath, "europe/norway/ostlandet")
                check(demJob.id.isNotBlank()) { "pmtilesQueueDemRegion empty id" }
                val demDone = pmtilesRunJob(dir.absolutePath, demJob.id)
                check(demDone.status == "completed") { "DEM download failed: ${demDone.status}" }
            }
        }
        MapHudPrefs.rememberDownloadedPmtilesRegion(
            InstrumentationRegistry.getInstrumentation().targetContext,
            "europe_norway_ostlandet",
        )
        Log.i(TAG, "basemap ready bytes=${basemap.length()} dem=${dem.length()}")
    }

    @Test
    fun a_provision_pbf_basemap_index_and_packs() {
        val dir = dataDir()
        val pbf = ensureOstlandetPbf()
        Log.i(TAG, "pbf=${pbf.absolutePath} bytes=${pbf.length()}")
        ensureFullBasemapPmtiles()

        val placeDb = File(dir, "place_index.db")
        val indexReport = ensurePlaceIndex(pbf.absolutePath, placeDb.absolutePath)
        Log.i(TAG, "ensurePlaceIndex=$indexReport place_bytes=${placeDb.length()}")
        assertTrue("place index must exist", placeDb.isFile && placeDb.length() > 10_000L)

        val before = indexedMapsStatus(pbf.absolutePath, dir.absolutePath).trim()
        Log.i(TAG, "indexedMapsStatus before=$before")
        if (before != "ready") {
            val t0 = System.currentTimeMillis()
            val report = ensureIndexedMaps(pbf.absolutePath, dir.absolutePath, null)
            val elapsed = System.currentTimeMillis() - t0
            Log.i(TAG, "ensureIndexedMaps elapsed_ms=$elapsed report=$report")
            assertTrue("ensureIndexedMaps must PASS:\n$report", report.contains("PASS"))
        }
        val after = indexedMapsStatus(pbf.absolutePath, dir.absolutePath).trim()
        Log.i(TAG, "indexedMapsStatus after=$after")
        assertTrue("expected ready, got $after", after == "ready")
        val man = File(dir, "ostlandet-latest.navi-manifest.json")
        assertTrue(man.isFile)
        assertTrue(
            "expected graph_format_version 4 in ${man.readText().take(500)}",
            man.readText().contains("\"graph_format_version\": 4") ||
                man.readText().contains("\"graph_format_version\":4"),
        )
    }

    @Test
    fun b_measure_espa_atnbrufossen_pack_hit() {
        val dir = dataDir()
        val pbf = File(dir, "ostlandet-latest.osm.pbf")
        assertTrue(pbf.isFile)
        val status = indexedMapsStatus(pbf.absolutePath, dir.absolutePath).trim()
        assertTrue("packs must be ready before measure, got $status", status == "ready")

        val context = InstrumentationRegistry.getInstrumentation().targetContext
        DiagnosticLog.setEnabled(context, true)
        val elev = File(dir, "elevation").absolutePath
        val cache = File(dir, "graph-cache-ostlandet-fair-measure").also { it.mkdirs() }.absolutePath
        val t0 = System.currentTimeMillis()
        val route =
            planCarRoute(
                pbfPath = pbf.absolutePath,
                elevDir = elev,
                cacheDir = cache,
                startLat = 60.5621914,
                startLon = 11.2561239,
                endLat = 61.85125,
                endLon = 10.233842,
                useEco = false,
                profile = TravelProfile.CAR,
                avoidMotorways = false,
                avoidTolls = false,
                avoidFerries = false,
                vehicle = FfiVehicleLimits(null, null, null, null, null, null),
                preferOfficialNetworks = false,
            )
        val wallMs = System.currentTimeMillis() - t0
        RoutingPlanLog.complete(route, ecoEnabled = false, durationMs = wallMs)
        DiagnosticLog.setEnabled(context, false)

        val model =
            android.os.Build.MODEL
                .replace(' ', '_')
        val out =
            buildString {
                appendLine("model=${android.os.Build.MODEL}")
                appendLine("indexedMapsStatus=$status")
                appendLine("wall_ms=$wallMs")
                appendLine("pack_hit=${route.report.contains("pack_hit=true")}")
                appendLine("poi_pack_hit=${route.report.contains("poi_pack_hit=true")}")
                appendLine("distance_km=${route.distanceKm}")
                appendLine("--- report ---")
                appendLine(route.report)
            }
        File(dir, "navi_fair_measure_$model.txt").writeText(out)
        Log.i(TAG, out)

        assertTrue("plan must PASS:\n${route.report}", route.report.contains("PASS"))
        assertTrue("must be pack_hit=true:\n${route.report}", route.report.contains("pack_hit=true"))
        assertTrue(route.distanceKm > 50.0)
    }

    companion object {
        private const val TAG = "ReprovisionMeasure"
    }
}
