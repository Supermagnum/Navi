package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
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
 * Emulator matrix: ECO on/off × Car/Truck/MobileHome/Motorcycle ×
 * Raufoss→Bergen, Espa→Dombås, Hamar→Lillehammer against real v9 packs.
 *
 * Truck/MobileHome long Stage-B failures with ECO off are expected (#166);
 * those rows are annotated, not treated as ECO regressions.
 */
@RunWith(AndroidJUnit4::class)
class EcoProfileMatrixInstrumentedTest {
    @Test
    fun eco_profile_route_matrix_v9_packs() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val packDir =
            runCatching { LongTripPackStorage.packDownloadDir(context) }.getOrElse {
                File(NaviAppData.resolve(context), LongTripPackStorage.PACKS_SUBDIR)
            }
        assertTrue("missing pack dir $packDir", packDir.isDirectory)
        for (stem in listOf("ostlandet-latest", "vestlandet-latest")) {
            val manifest = File(packDir, "$stem.navi-manifest.json")
            assertTrue("need $stem under $packDir", manifest.isFile)
            val fmt =
                runCatching {
                    JSONObject(manifest.readText()).optInt("graph_format_version", -1)
                }.getOrDefault(-1)
            assertTrue("$stem must be graph_format_version=9 (got $fmt)", fmt == 9)
        }
        val ostPbf = File(packDir, "ostlandet-latest.osm.pbf")
        assertTrue(ostPbf.isFile)

        setRoutePlanTimingEnabled(true)
        val elev =
            listOf(File(packDir, "elevation"), File(NaviAppData.resolve(context), "elevation"))
                .firstOrNull { it.isDirectory }
                ?.absolutePath
                ?: File(packDir, "elevation").absolutePath
        val cache = File(packDir, "graph-cache-eco-profile-matrix").also { it.mkdirs() }.absolutePath
        val dataDir = NaviAppData.resolve(context).absolutePath
        val packDirPath = packDir.absolutePath

        val header =
            "route\tprofile\teco\tpack_hit\twall_ms\tpack_load_ms\teco_reweight_ms\tastar_ms\t" +
                "distance_km\tok\tnote"
        val rows = mutableListOf(header)

        fun extract(
            report: String,
            key: String,
        ): String = Regex("""$key=([0-9.]+)""").find(report)?.groupValues?.get(1) ?: "-"

        fun run(
            route: String,
            profile: TravelProfile,
            eco: Boolean,
            startLat: Double,
            startLon: Double,
            endLat: Double,
            endLon: Double,
            longTrip: Boolean,
        ) {
            val t0 = System.nanoTime()
            val result =
                planCarRoute(
                    pbfPath = ostPbf.absolutePath,
                    elevDir = elev,
                    cacheDir = cache,
                    startLat = startLat,
                    startLon = startLon,
                    endLat = endLat,
                    endLon = endLon,
                    useEco = eco,
                    profile = profile,
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
            val report = result.report
            val ok =
                result.distanceKm > 1.0 &&
                    result.routePolyline.isNotBlank() &&
                    !report.contains("FAIL")
            val skeletonFail =
                report.contains("skeleton_preparing") ||
                    report.contains("preparing corridor skeleton")
            val note =
                when {
                    !ok &&
                        skeletonFail &&
                        (profile == TravelProfile.TRUCK || profile == TravelProfile.MOBILE_HOME) &&
                        longTrip ->
                        "expected #166 car-only corridor skeleton"
                    !ok && skeletonFail -> "skeleton_preparing"
                    !ok -> "FAIL"
                    else -> ""
                }
            val row =
                listOf(
                    route,
                    profile.name,
                    if (eco) "eco=true" else "eco=false",
                    report.contains("pack_hit=true").toString(),
                    wallMs.toString(),
                    extract(report, "pack_load_ms"),
                    extract(report, "eco_reweight_ms"),
                    extract(report, "astar_ms"),
                    "%.2f".format(result.distanceKm),
                    ok.toString(),
                    note,
                ).joinToString("\t")
            rows += row
            Log.i(TAG, "ECO_PROFILE_ROW $row")
            Log.i(TAG, "ECO_PROFILE_REPORT $route ${profile.name} eco=$eco\n$report")
        }

        val profiles =
            listOf(
                TravelProfile.CAR,
                TravelProfile.TRUCK,
                TravelProfile.MOBILE_HOME,
                TravelProfile.MOTORCYCLE,
            )
        val cases =
            listOf(
                // Stage-B densify corridors (longTrip on).
                Case("raufoss_bergen", RAUFOSS_LAT, RAUFOSS_LON, BERGEN_LAT, BERGEN_LON, true),
                Case("espa_dombas", ESPA_LAT, ESPA_LON, DOMBAS_LAT, DOMBAS_LON, true),
                // Short: no Stage-B densify requirement.
                Case(
                    "hamar_lillehammer",
                    HAMAR_LAT,
                    HAMAR_LON,
                    LILLEHAMMER_LAT,
                    LILLEHAMMER_LON,
                    false,
                ),
            )

        for (c in cases) {
            for (profile in profiles) {
                for (eco in listOf(false, true)) {
                    run(
                        route = c.name,
                        profile = profile,
                        eco = eco,
                        startLat = c.startLat,
                        startLon = c.startLon,
                        endLat = c.endLat,
                        endLon = c.endLon,
                        longTrip = c.longTrip,
                    )
                }
            }
        }

        setRoutePlanTimingEnabled(false)
        val out = rows.joinToString("\n")
        File(packDir, "eco_profile_matrix_emulator.tsv").writeText(out)
        File(context.cacheDir, "eco_profile_matrix_emulator.tsv").writeText(out)
        runCatching {
            File("/data/local/tmp/eco_profile_matrix_emulator.tsv").writeText(out)
        }
        Log.i(TAG, "ECO_PROFILE_TABLE\n$out")

        // Car long corridors must succeed with pack_hit for both eco settings.
        val carLongOk =
            rows.filter {
                (it.startsWith("raufoss_bergen\tCAR\t") || it.startsWith("espa_dombas\tCAR\t")) &&
                    it.split('\t').getOrNull(9) == "true"
            }
        assertTrue("Car long routes must succeed:\n$out", carLongOk.size >= 4)

        // Truck/MH long fails are expected (#166) — both eco on and off.
        val truckMhLong =
            rows.filter {
                (it.startsWith("raufoss_bergen\tTRUCK\t") ||
                    it.startsWith("raufoss_bergen\tMOBILE_HOME\t") ||
                    it.startsWith("espa_dombas\tTRUCK\t") ||
                    it.startsWith("espa_dombas\tMOBILE_HOME\t"))
            }
        assertTrue(
            "Truck/MH long Stage-B rows must be annotated #166:\n$out",
            truckMhLong.all { it.contains("#166") },
        )
    }

    private data class Case(
        val name: String,
        val startLat: Double,
        val startLon: Double,
        val endLat: Double,
        val endLon: Double,
        val longTrip: Boolean,
    )

    companion object {
        private const val TAG = "EcoProfileMatrix"
        private const val RAUFOSS_LAT = 60.7277483
        private const val RAUFOSS_LON = 10.6109403
        private const val BERGEN_LAT = 60.388144
        private const val BERGEN_LON = 5.3347434
        private const val ESPA_LAT = 60.5621914
        private const val ESPA_LON = 11.2561239
        private const val DOMBAS_LAT = 62.0755
        private const val DOMBAS_LON = 9.1278
        private const val HAMAR_LAT = 60.794721
        private const val HAMAR_LON = 11.068055
        private const val LILLEHAMMER_LAT = 61.114545
        private const val LILLEHAMMER_LON = 10.467007
    }
}
