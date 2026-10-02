package no.navi.app

import android.os.Debug
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
 * Region-to-region timing matrix against real Norway v9 packs on the selected
 * pack volume (SD long-trip-packs). Greppable PROFILE_ROW lines feed
 * docs/perf/region-to-region-graph-build.md.
 *
 * Innlandet is not a separate Geofabrik leaf (hedmark/oppland → Ostlandet).
 * Installed stems expected: ostlandet, vestlandet, trondelag, nord-norge,
 * sorlandet.
 */
@RunWith(AndroidJUnit4::class)
class RegionToRegionPerfMatrixInstrumentedTest {
    @Test
    fun matrix_raufoss_bergen_eco_and_controls() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val packDir =
            runCatching { LongTripPackStorage.packDownloadDir(context) }.getOrElse {
                File(NaviAppData.resolve(context), LongTripPackStorage.PACKS_SUBDIR)
            }
        assertTrue("missing pack dir $packDir", packDir.isDirectory)
        for (stem in listOf(
            "ostlandet-latest",
            "vestlandet-latest",
            "trondelag-latest",
            "nord-norge-latest",
            "sorlandet-latest",
        )) {
            assertTrue(
                "need $stem v9 packs under $packDir",
                File(packDir, "$stem.navi-manifest.json").isFile,
            )
        }
        val ostPbf = File(packDir, "ostlandet-latest.osm.pbf")
        val vestPbf = File(packDir, "vestlandet-latest.osm.pbf")
        assertTrue(ostPbf.isFile)
        assertTrue(vestPbf.isFile)

        setRoutePlanTimingEnabled(true)
        val elev =
            listOf(File(packDir, "elevation"), File(NaviAppData.resolve(context), "elevation"))
                .firstOrNull { it.isDirectory }
                ?.absolutePath
                ?: File(packDir, "elevation").absolutePath
        val cache = File(packDir, "graph-cache-r2r-perf").also { it.mkdirs() }.absolutePath
        val dataDir = NaviAppData.resolve(context).absolutePath
        val packDirPath = packDir.absolutePath

        val rows = mutableListOf<String>()
        rows +=
            "route\teco\tpack_hit\twall_ms\tplan_ms\tpack_load_ms\teco_reweight_ms\tastar_ms\t" +
                "expansions\tnodes\tedges\tdistance_km\tpeak_rss_mb\tpeak_native_heap_mb\troute_ok"

        fun run(
            name: String,
            pbf: File,
            endLat: Double,
            endLon: Double,
            eco: Boolean,
            startLat: Double = RAUFOSS_LAT,
            startLon: Double = RAUFOSS_LON,
            longTrip: Boolean = false,
        ) {
            val heapBefore = Debug.getNativeHeapAllocatedSize()
            val t0 = System.nanoTime()
            val route =
                planCarRoute(
                    pbfPath = pbf.absolutePath,
                    elevDir = elev,
                    cacheDir = cache,
                    startLat = startLat,
                    startLon = startLon,
                    endLat = endLat,
                    endLon = endLon,
                    useEco = eco,
                    profile = TravelProfile.CAR,
                    avoidMotorways = false,
                    tollPolicy = FfiTollPolicy.ALLOW,
                    avoidFerries = false,
                    avoidTunnels = false,
                    vehicle = FfiVehicleLimits(null, null, null, null, null, null),
                    preferOfficialNetworks = false,
                    dataDir = dataDir,
                    packDir = packDirPath,
                    longTripEnabled = longTrip,
                    allowedCountries = null,
                    viaPoints = emptyList(),
                )
            val wallMs = (System.nanoTime() - t0) / 1_000_000L
            val heapAfter = Debug.getNativeHeapAllocatedSize()
            val peakNativeMb =
                "%.1f".format(
                    heapBefore.coerceAtLeast(heapAfter) / (1024.0 * 1024.0),
                )
            val report = route.report
            val ok =
                route.distanceKm > 1.0 &&
                    route.routePolyline.isNotBlank() &&
                    !report.contains("FAIL")
            val row =
                listOf(
                    name,
                    eco.toString(),
                    report.contains("pack_hit=true").toString(),
                    wallMs.toString(),
                    extract(report, "plan_duration_ms"),
                    extract(report, "pack_load_ms"),
                    extract(report, "eco_reweight_ms"),
                    extract(report, "astar_ms"),
                    extract(report, "expansions"),
                    extractToken(report, "nodes="),
                    extractToken(report, "edges="),
                    "%.2f".format(route.distanceKm),
                    extract(report, "peak_rss_mb"),
                    peakNativeMb,
                    ok.toString(),
                ).joinToString("\t")
            rows += row
            Log.i(TAG, "PROFILE_ROW $row")
            Log.i(TAG, "PROFILE_REPORT $name\n$report")
            Log.i(
                TAG,
                "PROFILE_PACKS $name primary_stem=${extractToken(report, "primary_stem=")} " +
                    "extra=${extractToken(report, "extra_stem_list=")} " +
                    "tiles=${extract(report, "tile_budget")} " +
                    "cache=${extractToken(report, "corridor_cache=")}",
            )
        }

        // Failing case + controls (coords from campaign / task brief).
        run("raufoss_bergen", ostPbf, BERGEN_LAT, BERGEN_LON, eco = true)
        run("raufoss_bergen", ostPbf, BERGEN_LAT, BERGEN_LON, eco = false)
        run("raufoss_bergen_warm", ostPbf, BERGEN_LAT, BERGEN_LON, eco = true)
        run("raufoss_dombas", ostPbf, DOMBAS_LAT, DOMBAS_LON, eco = true)
        run("raufoss_dombas", ostPbf, DOMBAS_LAT, DOMBAS_LON, eco = false)
        run("bergen_forde", vestPbf, FORDE_LAT, FORDE_LON, eco = true, BERGEN_LAT, BERGEN_LON)
        run("bergen_forde", vestPbf, FORDE_LAT, FORDE_LON, eco = false, BERGEN_LAT, BERGEN_LON)
        // 3+ stem corridor (Ostlandet + Trøndelag + Nord-Norge) via densify/chunk.
        run("raufoss_tromso", ostPbf, TROMSO_LAT, TROMSO_LON, eco = false, longTrip = true)

        setRoutePlanTimingEnabled(false)
        val out = rows.joinToString("\n")
        File(packDir, "region_to_region_perf_matrix.tsv").writeText(out)
        Log.i(TAG, "PROFILE_TABLE\n$out")
        // Warm (<2 s): Arc corridor + ferry-overlay skip on cache hit.
        // Cold eco on emulator SD is dominated by tile materialize (~5–6 s for
        // 11 tiles / ~496k edges); allow SD variance up to 9 s.
        assertTrue(
            "Raufoss→Bergen eco cold must pack-hit and finish under 9s:\n$out",
            rows.any {
                it.startsWith("raufoss_bergen\ttrue\ttrue\t") &&
                    it.split('\t').getOrNull(3)?.toLongOrNull()?.let { ms -> ms < 9_000 } == true
            },
        )
        assertTrue(
            "Raufoss→Bergen eco warm must finish under 2s:\n$out",
            rows.any {
                it.startsWith("raufoss_bergen_warm\ttrue\ttrue\t") &&
                    it.split('\t').getOrNull(3)?.toLongOrNull()?.let { ms -> ms < 2_000 } == true
            },
        )
    }

    private fun extract(
        report: String,
        key: String,
    ): String = Regex("""$key=([0-9.]+)""").find(report)?.groupValues?.get(1) ?: "-"

    private fun extractToken(
        report: String,
        key: String,
    ): String {
        val esc = Regex.escape(key)
        return Regex("""$esc([^\s;]+)""").find(report)?.groupValues?.get(1) ?: "-"
    }

    companion object {
        private const val TAG = "R2RPerfMatrix"
        private const val RAUFOSS_LAT = 60.7277483
        private const val RAUFOSS_LON = 10.6109403
        private const val BERGEN_LAT = 60.388144
        private const val BERGEN_LON = 5.3347434
        private const val DOMBAS_LAT = 62.0755
        private const val DOMBAS_LON = 9.1278
        private const val FORDE_LAT = 61.4522
        private const val FORDE_LON = 5.8570
        private const val TROMSO_LAT = 69.6492
        private const val TROMSO_LON = 18.9553
    }
}
