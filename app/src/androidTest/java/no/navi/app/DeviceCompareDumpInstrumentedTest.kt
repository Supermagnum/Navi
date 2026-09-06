package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.indexedMapsStatus
import uniffi.navi.searchPlaces
import java.io.File

/**
 * Read-only dump of region/pack/place-index state plus one Espa→Atnbrufossen
 * plan on the same [RouteReplan] path the app uses. Does not copy fixtures,
 * rebuild packs, or overwrite region files.
 */
@RunWith(AndroidJUnit4::class)
class DeviceCompareDumpInstrumentedTest {
    @Test
    fun dumpRegionPackAndPlanEspaAtnbrufossen() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = NaviAppData.resolve(context)
        val out = StringBuilder()

        fun line(s: String) {
            out.append(s).append('\n')
            Log.i(TAG, s)
        }

        val model =
            android.os.Build.MODEL
                .replace(' ', '_')
        line("=== DEVICE ===")
        line("model=${android.os.Build.MODEL} device=${android.os.Build.DEVICE} serial=${android.os.Build.SERIAL}")
        line("android=${android.os.Build.VERSION.RELEASE} sdk=${android.os.Build.VERSION.SDK_INT}")
        line("dataDir=${dataDir.absolutePath}")

        line("=== APP FILES (pbf/pack/index/pmtiles) ===")
        dumpTree(dataDir) { line(it) }
        line("=== STAGED FIXTURES ===")
        dumpTree(File("/data/local/tmp/navi_fixtures")) { line(it) }

        val pbf = RouteReplan.resolvePbf(dataDir)
        line("=== RESOLVE ===")
        line("resolvePbf=${pbf?.absolutePath} bytes=${pbf?.length() ?: -1}")
        val status =
            if (pbf != null && pbf.isFile) {
                runCatching { indexedMapsStatus(pbf.absolutePath, dataDir.absolutePath).trim() }
                    .getOrElse { "error: ${it.message}" }
            } else {
                "no_pbf"
            }
        line("indexedMapsStatus=$status")
        val ui = IndexedMapsBackground.uiLine(pbf, dataDir)
        line("indexedMapsUiLine=$ui")
        line("downloadedGeofabrikPaths=${RegionCoverage.downloadedGeofabrikPaths(dataDir)}")

        val man = File(dataDir, "ostlandet-latest.navi-manifest.json")
        line("manifest_exists=${man.isFile} bytes=${if (man.isFile) man.length() else -1}")
        if (man.isFile) {
            line("manifest=${man.readText().take(2000)}")
        }

        val placeApp = File(dataDir, "place_index.db")
        val placeStaged = File("/data/local/tmp/navi_fixtures/place_index_search_check.db")
        val placeDb =
            when {
                placeApp.isFile && placeApp.length() > 10_000L -> placeApp
                placeStaged.isFile && placeStaged.canRead() && placeStaged.length() > 10_000L -> placeStaged
                else -> placeApp
            }
        line("=== PLACE INDEX ===")
        line("app_place_index exists=${placeApp.isFile} bytes=${if (placeApp.isFile) placeApp.length() else -1}")
        line("staged_place_index exists=${placeStaged.isFile} bytes=${if (placeStaged.isFile) placeStaged.length() else -1}")
        line("resolved_place_index=${placeDb.absolutePath} bytes=${if (placeDb.isFile) placeDb.length() else -1}")
        val queries = listOf("Espa", "Atnbrufossen", "Esso", "Circle K", "Storgata", "bensin")
        if (placeDb.isFile && placeDb.length() > 10_000L) {
            for (q in queries) {
                val hits =
                    runCatching { searchPlaces(placeDb.absolutePath, q, 8u) }
                        .getOrElse { emptyList() }
                line(
                    "search q=$q hits=${hits.size} " +
                        hits.take(5).joinToString(" | ") { "${it.name}/${it.kind}@${it.lat},${it.lon}" },
                )
            }
        } else {
            line("place_index missing — search skipped")
        }

        DiagnosticLog.setEnabled(context, true)
        line("diagnostic_logging_enabled=${DiagnosticLog.isEnabled()} dir=${DiagnosticLog.publicLocationDescription()}")

        line("=== PLAN Espa→Atnbrufossen (RouteReplan, car, eco=false) ===")
        val t0 = System.currentTimeMillis()
        RoutingPlanLog.start(
            profile = "car",
            ecoEnabled = false,
            legCount = 1,
            waypointNames = listOf("Espa", "Atnbrufossen"),
            startLat = 60.5621914,
            startLon = 11.2561239,
            endLat = 61.85125,
            endLon = 10.233842,
        )
        val result =
            runBlocking {
                RouteReplan.plan(
                    dataDir = dataDir,
                    profile = TravelProfile.CAR,
                    waypoints =
                        listOf(
                            Waypoint("Espa", 60.5621914, 11.2561239),
                            Waypoint("Atnbrufossen", 61.85125, 10.233842),
                        ),
                    useEco = false,
                    avoidMotorways = false,
                    avoidTolls = false,
                    avoidFerries = false,
                    vehicle = FfiVehicleLimits(null, null, null, null, null, null),
                    preferOfficialNetworks = false,
                    preferPilgrimRoutes = false,
                )
            }
        val wallMs = System.currentTimeMillis() - t0
        RoutingPlanLog.complete(result, ecoEnabled = false, durationMs = wallMs)
        line("wall_ms=$wallMs distance_km=${result.distanceKm} eta_min=${result.etaMinutes}")
        line("cacheHit=${result.cacheHit} coldBuildS=${result.coldBuildS} warmLoadS=${result.warmLoadS}")
        line("pack_hit=${result.report.contains("pack_hit=true")} poi_pack_hit=${result.report.contains("poi_pack_hit=true")} wetland_pack_hit=${result.report.contains("wetland_pack_hit=true")}")
        line("--- native report ---")
        line(result.report)

        val uiWaypoints =
            listOf(
                RegionCoverage.Waypoint("From", "Espa", 60.5621914, 11.2561239),
                RegionCoverage.Waypoint("To", "Atnbrufossen", 61.85125, 10.233842),
            )
        val uiPbf = RegionCoverage.resolvePlanPbf(dataDir, uiWaypoints)
        line("=== PLAN UI resolvePlanPbf + explicit dataDir (car, eco=false) ===")
        line("resolvePlanPbf=${uiPbf?.absolutePath} bytes=${uiPbf?.length() ?: -1}")
        val uiStatus =
            if (uiPbf != null && uiPbf.isFile) {
                runCatching { indexedMapsStatus(uiPbf.absolutePath, dataDir.absolutePath).trim() }
                    .getOrElse { "error: ${it.message}" }
            } else {
                "no_pbf"
            }
        line("indexedMapsStatus_uiPbf=$uiStatus")
        line("indexedMapsUiLine_appFirstPbf=${IndexedMapsBackground.uiLine(pbf, dataDir)}")
        if (uiPbf != null && uiPbf.isFile) {
            val cacheDir =
                File(dataDir, "graph-cache-${uiPbf.nameWithoutExtension}-car")
            cacheDir.mkdirs()
            val tUi = System.currentTimeMillis()
            val uiResult =
                uniffi.navi.planCarRoute(
                    uiPbf.absolutePath,
                    File(dataDir, "elevation").absolutePath,
                    cacheDir.absolutePath,
                    60.5621914,
                    11.2561239,
                    61.85125,
                    10.233842,
                    false,
                    TravelProfile.CAR,
                    false,
                    uniffi.navi.FfiTollPolicy.ALLOW,
                    false,
                    FfiVehicleLimits(null, null, null, null, null, null),
                    false,
                    dataDir.absolutePath,
                )
            val uiWallMs = System.currentTimeMillis() - tUi
            RoutingPlanLog.complete(uiResult, ecoEnabled = false, durationMs = uiWallMs)
            line("ui_wall_ms=$uiWallMs distance_km=${uiResult.distanceKm}")
            line(
                "ui_pack_hit=${uiResult.report.contains("pack_hit=true")} " +
                    "ui_poi_pack_hit=${uiResult.report.contains("poi_pack_hit=true")}",
            )
            line("--- UI-path native report ---")
            line(uiResult.report)
        }

        DiagnosticLog.setEnabled(context, false)
        val session = DiagnosticLog.listSessionFiles(context).maxByOrNull { it.lastModified() }
        line("session_file=${session?.absolutePath} bytes=${session?.length() ?: -1}")
        val dumpFile = File(dataDir, "navi_device_compare_$model.txt")
        dumpFile.writeText(out.toString())
        line("dump_file=${dumpFile.absolutePath} bytes=${dumpFile.length()}")
        Log.i(TAG, "DUMP_DONE ${dumpFile.absolutePath}")
    }

    private fun dumpTree(
        dir: File,
        line: (String) -> Unit,
    ) {
        if (!dir.isDirectory) {
            line("missing $dir")
            return
        }
        dir.walkTopDown().maxDepth(2).forEach { f ->
            if (f == dir) return@forEach
            val rel = f.relativeTo(dir).path
            if (f.isDirectory) {
                line("dir $rel")
            } else {
                line("file $rel bytes=${f.length()} mtime=${f.lastModified()}")
            }
        }
    }

    companion object {
        private const val TAG = "DeviceCompareDump"
    }
}
