package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.loadUseNetworkedCabins
import uniffi.navi.planHikingRoute
import uniffi.navi.savePreferOfficialNetworks
import uniffi.navi.saveUseNetworkedCabins
import uniffi.navi.searchPlaces
import uniffi.navi.setRoutePlanTimingEnabled
import java.io.File

/**
 * On-device toggle matrix: Nashaugsætra → Skriurusten (hiking).
 *
 * Adapts the Wetland / Skolla host-planner pattern: resolve places via FTS,
 * set prefer_official_networks + use_networked_cabins, planHikingRoute ×4,
 * dump reports under files/nashaugs_skriurusten_matrix/.
 */
@RunWith(AndroidJUnit4::class)
class NashaugsSkriurustenMatrixInstrumentedTest {
    private val dataDir: File =
        NaviAppData.resolve(InstrumentationRegistry.getInstrumentation().targetContext)

    private fun outDir(): File = File(dataDir, "nashaugs_skriurusten_matrix").also { it.mkdirs() }

    private fun dump(
        name: String,
        body: String,
    ) {
        File(outDir(), name).writeText(body)
        android.util.Log.i(TAG, "DUMP $name bytes=${body.length}")
        body.lineSequence().take(80).forEachIndexed { i, line ->
            android.util.Log.i(TAG, "$name[$i] $line")
        }
    }

    private fun pbf(): File? {
        val p = File(dataDir, "ostlandet-latest.osm.pbf")
        return p.takeIf { it.isFile && it.length() > 100_000_000L }
    }

    private fun resolveWp(
        query: String,
        nameNeedle: String,
    ): Pair<String, Pair<Double, Double>> {
        val db = File(dataDir, "place_index.db")
        assumeTrue("missing place_index.db", db.isFile)
        val hits = searchPlaces(db.absolutePath, query, 20u)
        val hit =
            hits.firstOrNull { it.name.equals(nameNeedle, ignoreCase = true) }
                ?: hits.firstOrNull { it.name.contains(nameNeedle, ignoreCase = true) }
        assertTrue(
            "no place hit for '$query'/'$nameNeedle' (got=${hits.map { it.name }.take(10)})",
            hit != null,
        )
        return hit!!.name to (hit.lat to hit.lon)
    }

    @Test
    fun nashaugs_to_skriurusten_four_toggle_combos() {
        val pbf = pbf()
        assumeTrue("missing Ostlandet PBF", pbf != null)

        val (fromName, fromLl) = resolveWp("nashaugsætra", "Nashaugsætra")
        val (toName, toLl) = resolveWp("skriurusten", "Skriurusten")
        val wps =
            """[{"name":${jsonStr(fromName)},"lat":${fromLl.first},"lon":${fromLl.second}},""" +
                """{"name":${jsonStr(toName)},"lat":${toLl.first},"lon":${toLl.second}}]"""
        dump(
            "waypoints.json",
            "from=$fromName ${fromLl.first},${fromLl.second}\n" +
                "to=$toName ${toLl.first},${toLl.second}\n$wps\n",
        )

        val cache =
            File(dataDir, "graph-cache-nashaugs-skriurusten").also { it.mkdirs() }
        setRoutePlanTimingEnabled(true)

        val combos =
            listOf(
                Triple(1, false, false),
                Triple(2, true, false),
                Triple(3, false, true),
                Triple(4, true, true),
            )
        val summary = StringBuilder()
        summary.appendLine(
            "combo\tofficial\tcabins\tdistance_km\teta_min\tauto_vias\tauto_via_names\taccessish",
        )

        for ((num, official, cabins) in combos) {
            assertTrue(saveUseNetworkedCabins(dataDir.absolutePath, cabins))
            assertTrue(
                "cabins pref",
                loadUseNetworkedCabins(dataDir.absolutePath) == cabins,
            )
            // Official networks is a planHikingRoute arg; also persist for UI parity.
            savePreferOfficialNetworks(dataDir.absolutePath, official)

            android.util.Log.i(TAG, "=== combo $num official=$official cabins=$cabins ===")
            val t0 = System.nanoTime()
            val hike =
                planHikingRoute(
                    pbfPath = pbf!!.absolutePath,
                    elevDir = File(dataDir, "elevation").absolutePath,
                    cacheDir = cache.absolutePath,
                    waypointsJson = wps,
                    preferOfficialNetworks = official,
                    preferPilgrimRoutes = false,
                    dataDir = "",
                )
            val ms = (System.nanoTime() - t0) / 1_000_000L
            dump("c${num}_report.txt", "plan_wall_ms=$ms\n${hike.report}")
            dump("c${num}_breaks.json", hike.breakPoisJson)
            File(outDir(), "c${num}_polyline.txt").writeText(hike.routePolyline)

            assertTrue("combo $num failed:\n${hike.report}", hike.report.contains("PASS"))
            assertTrue(
                "combo $num pref echo:\n${hike.report}",
                hike.report.contains("use_networked_cabins=$cabins"),
            )

            val autoLine =
                hike.report.lineSequence().firstOrNull { it.startsWith("auto_vias=") }
                    ?: "auto_vias=?"
            val autoN = autoLine.removePrefix("auto_vias=").substringBefore(';')
            val autoNames =
                autoLine.substringAfter("names=", missingDelimiterValue = "").ifBlank { "-" }
            val blob =
                (hike.report + "\n" + hike.breakPoisJson + "\n" + hike.offTrailAdvisory)
                    .lowercase()
            val accessish =
                listOf(
                    "member",
                    "membership",
                    "non-member",
                    "emergency",
                    "overnight",
                    "stay",
                    "enter the hut",
                    "sleep",
                    "dnt key",
                ).filter { blob.contains(it) }
                    .joinToString(",")
                    .ifBlank { "-" }

            summary.appendLine(
                "$num\t$official\t$cabins\t${"%.2f".format(hike.distanceKm)}\t" +
                    "${"%.0f".format(hike.etaMinutes)}\t$autoN\t$autoNames\t$accessish",
            )
        }
        setRoutePlanTimingEnabled(false)
        dump("matrix_summary.tsv", summary.toString())
        // Also stage for adb pull
        File("/data/local/tmp/nashaugs_skriurusten_matrix_summary.tsv")
            .writeText(summary.toString())
    }

    private fun jsonStr(s: String): String = "\"" + s.replace("\\", "\\\\").replace("\"", "\\\"") + "\""

    companion object {
        private const val TAG = "NashaugsMatrix"
    }
}
