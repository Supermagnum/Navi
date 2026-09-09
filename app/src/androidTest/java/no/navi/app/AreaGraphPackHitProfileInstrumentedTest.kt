package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiTollPolicy
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.planCarRoute
import uniffi.navi.setRoutePlanTimingEnabled
import java.io.File

/**
 * Pack-hit area-graph timing for the Part-4 ODs, against v6 packs already under
 * [PACK_DIR] (OstlandetVestlandet download). Logs a greppable comparison table.
 *
 * Cross-landsdel ODs (Årdalstangen / Aga) are attempted with the Ostlandet stem
 * first (current single-PBF planner behaviour when Norway is absent) and then
 * noted if pack_hit cannot cover the destination.
 */
@RunWith(AndroidJUnit4::class)
class AreaGraphPackHitProfileInstrumentedTest {
    @Test
    fun profile_raufoss_ods_pack_hit() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val packDir = File(NaviAppData.resolve(context), PACK_DIR)
        assertTrue("missing pack dir $packDir", packDir.isDirectory)
        val ostPbf = File(packDir, "ostlandet-latest.osm.pbf")
        val vestPbf = File(packDir, "vestlandet-latest.osm.pbf")
        assertTrue(ostPbf.isFile && ostPbf.length() > 1_000_000L)
        assertTrue(File(packDir, "ostlandet-latest.navi-manifest.json").isFile)
        assertTrue(File(packDir, "vestlandet-latest.navi-manifest.json").isFile)

        setRoutePlanTimingEnabled(true)
        val elev = File(packDir, "elevation").absolutePath
        val cache = File(packDir, "graph-cache-pack-hit-profile").also { it.mkdirs() }.absolutePath
        val dataDir = packDir.absolutePath

        val rows = mutableListOf<String>()
        rows += "route\tpbf_stem\tpack_hit\tpoi_pack_hit\tgraph_build_ms\tastar_ms\tpoi_barrier_ms\tnodes\tedges\tdistance_km\twall_ms\troute_ok"

        fun run(
            name: String,
            pbf: File,
            endLat: Double,
            endLon: Double,
        ) {
            val t0 = System.nanoTime()
            val route =
                planCarRoute(
                    pbfPath = pbf.absolutePath,
                    elevDir = elev,
                    cacheDir = cache,
                    startLat = START_LAT,
                    startLon = START_LON,
                    endLat = endLat,
                    endLon = endLon,
                    useEco = false,
                    profile = TravelProfile.CAR,
                    avoidMotorways = false,
                    tollPolicy = FfiTollPolicy.ALLOW,
                    avoidFerries = false,
                    vehicle = FfiVehicleLimits(null, null, null, null, null, null),
                    preferOfficialNetworks = false,
                    dataDir = dataDir,
                )
            val wallMs = (System.nanoTime() - t0) / 1_000_000L
            val report = route.report
            val packHit = report.contains("pack_hit=true")
            val poiHit = report.contains("poi_pack_hit=true")
            val ok =
                route.distanceKm > 1.0 &&
                    route.routePolyline.isNotBlank() &&
                    !report.contains("FAIL")
            val row =
                listOf(
                    name,
                    pbf.nameWithoutExtension,
                    packHit.toString(),
                    poiHit.toString(),
                    extractMs(report, "graph_build_ms"),
                    extractMs(report, "astar_ms"),
                    extractMs(report, "poi_barrier_ms"),
                    extractToken(report, "nodes="),
                    extractToken(report, "edges="),
                    "%.2f".format(route.distanceKm),
                    wallMs.toString(),
                    ok.toString(),
                ).joinToString("\t")
            rows += row
            Log.i(TAG, "PROFILE_ROW $row")
            Log.i(TAG, "PROFILE_REPORT $name\n$report")
        }

        run("baseline_short", ostPbf, 60.7000, 10.6200)
        run("raufoss_os_innlandet", ostPbf, 62.4963960, 11.2233111)
        // Cross-landsdel: current planner uses one stem. Ostlandet cannot cover
        // Vestlandet destinations; record the attempt for the comparison table.
        run("raufoss_ardalstangen_ost_stem", ostPbf, 61.2361360, 7.7025037)
        run("raufoss_aga_ost_stem", ostPbf, 60.2993285, 6.6030684)
        // Vestlandet stem alone cannot cover Raufoss start — also recorded.
        if (vestPbf.isFile && vestPbf.length() > 1_000_000L) {
            run("raufoss_ardalstangen_vest_stem", vestPbf, 61.2361360, 7.7025037)
            run("raufoss_aga_vest_stem", vestPbf, 60.2993285, 6.6030684)
        }

        setRoutePlanTimingEnabled(false)
        val out = rows.joinToString("\n")
        File(packDir, "area_graph_pack_hit_profile.tsv").writeText(out)
        Log.i(TAG, "PROFILE_TABLE\n$out")
        // At least baseline + Os must be pack-hit successes.
        assertTrue(
            "expected pack_hit rows for baseline/Os:\n$out",
            out.contains("baseline_short\tostlandet-latest\ttrue") &&
                out.contains("raufoss_os_innlandet\tostlandet-latest\ttrue"),
        )
    }

    private fun extractMs(
        report: String,
        key: String,
    ): String {
        val re = Regex("""$key=(\d+)""")
        return re.find(report)?.groupValues?.get(1) ?: "-"
    }

    private fun extractToken(
        report: String,
        key: String,
    ): String {
        val re = Regex("""$key(\d+)""")
        return re.find(report)?.groupValues?.get(1) ?: "-"
    }

    companion object {
        private const val TAG = "AreaGraphPackHit"
        private const val PACK_DIR = "ost_vest_pack_dl"
        private const val START_LAT = 60.7163834
        private const val START_LON = 10.6202916
    }
}
