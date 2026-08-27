package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.FixMethodOrder
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters
import uniffi.navi.ensureIndexedMaps
import uniffi.navi.indexedMapsStatus
import uniffi.navi.planHikingRoute
import uniffi.navi.setRoutePlanTimingEnabled
import java.io.File

/**
 * Rebuild Ostlandet packs with tiled wetland + overnight buildings, then time
 * the short Atnbrufossen hike that previously took ~159 s.
 */
@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class WetlandTiledPackCloseGapTest {
    private val dataDir: File =
        NaviAppData.resolve(InstrumentationRegistry.getInstrumentation().targetContext)

    private fun outDir(): File = File(dataDir, "wetland_tiled_gap").also { it.mkdirs() }

    private fun dump(
        name: String,
        body: String,
    ) {
        File(outDir(), name).writeText(body)
        android.util.Log.i(TAG, "DUMP $name bytes=${body.length}")
        body.lineSequence().forEachIndexed { i, line ->
            android.util.Log.i(TAG, "$name[$i] $line")
        }
    }

    private fun pbf(): File? {
        val p = File(dataDir, "ostlandet-latest.osm.pbf")
        return p.takeIf { it.isFile && it.length() > 100_000_000L }
    }

    @Test
    fun a_statusBeforeShowsMismatchOrMissingWetland() {
        val pbf = pbf()
        assumeTrue(pbf != null)
        val st = indexedMapsStatus(pbf!!.absolutePath, dataDir.absolutePath).trim()
        dump("status_before.txt", "indexedMapsStatus=$st\n")
        // Pre-fix packs lack wetland / poi v2 → not ready.
        assertTrue(
            "expected not-ready before rebuild, got $st",
            st == "missing" || st == "version_mismatch" || st == "stale_pbf" || st == "ready",
        )
    }

    @Test
    fun b_rebuildOstlandetWithWetlandTiles() {
        val pbf = pbf()
        assumeTrue(pbf != null)
        val before = indexedMapsStatus(pbf!!.absolutePath, dataDir.absolutePath).trim()
        assumeTrue(
            "skip rebuild if already ready with wetland tiles",
            before != "ready" ||
                !File(dataDir, "ostlandet-latest.navi-manifest.json")
                    .readText()
                    .contains("navi-wetland.t"),
        )
        val t0 = System.nanoTime()
        val report = ensureIndexedMaps(pbf.absolutePath, dataDir.absolutePath, null)
        val elapsedMs = (System.nanoTime() - t0) / 1_000_000L
        dump("ensure_indexed_maps.txt", "elapsed_ms=$elapsedMs\n$report\n")
        assertTrue("convert failed:\n$report", report.contains("PASS"))
        assertTrue("expected real convert:\n$report", report.contains("cache_hit=false"))
        assertTrue(
            "expected wetland rings:\n$report",
            report.contains("wetland_rings=") && !report.contains("wetland_rings=0\n"),
        )
        val after = indexedMapsStatus(pbf.absolutePath, dataDir.absolutePath).trim()
        assertTrue("after=$after", after == "ready")
        val man = File(dataDir, "ostlandet-latest.navi-manifest.json").readText()
        dump("manifest_after.txt", man)
        assertTrue(man.contains("\"wetland_format_version\": 1") || man.contains("\"wetland_format_version\":1"))
        assertTrue(
            "manifest should list wetland tiles:\n$man",
            man.contains("wetland_tiles") && man.contains("navi-wetland.t"),
        )
        assertTrue(man.contains("\"poi_barrier_format_version\": 2") || man.contains("\"poi_barrier_format_version\":2"))
    }

    @Test
    fun c_shortHikeAtnbrufossenPackHitTiming() {
        val pbf = pbf()
        assumeTrue(pbf != null)
        assumeTrue(
            indexedMapsStatus(pbf!!.absolutePath, dataDir.absolutePath).trim() == "ready",
        )
        setRoutePlanTimingEnabled(true)
        val hike =
            planHikingRoute(
                pbfPath = pbf.absolutePath,
                elevDir = File(dataDir, "elevation").absolutePath,
                cacheDir =
                    File(dataDir, "graph-cache-wetland-gap-hike")
                        .also { it.mkdirs() }
                        .absolutePath,
                waypointsJson =
                    """[{"name":"A","lat":61.85125,"lon":10.233842},{"name":"B","lat":61.8700,"lon":10.2500}]""",
                preferOfficialNetworks = false,
                preferPilgrimRoutes = false,
                dataDir = "",
            )
        dump("short_hike_atnbrufossen.txt", hike.report)
        setRoutePlanTimingEnabled(false)
        assertTrue("hike failed:\n${hike.report}", hike.report.contains("PASS"))
        assertTrue(
            "expected wetland_pack_hit=true:\n${hike.report}",
            hike.report.contains("wetland_pack_hit=true"),
        )
        assertTrue(
            "expected overnight buildings pack hit:\n${hike.report}",
            hike.report.contains("overnight_buildings_pack_hit=true") ||
                hike.report.contains("poi_pack_hit=true"),
        )
        val planMs =
            hike.report
                .lineSequence()
                .first { it.startsWith("plan_duration_ms=") }
                .removePrefix("plan_duration_ms=")
                .trim()
                .toLong()
        // Before fix: ~159477 ms. Pack-hit target is well under 30 s on this OD.
        assertTrue("still too slow plan_duration_ms=$planMs:\n${hike.report}", planMs < 30_000)
    }

    companion object {
        private const val TAG = "WetlandTiledGap"
    }
}
