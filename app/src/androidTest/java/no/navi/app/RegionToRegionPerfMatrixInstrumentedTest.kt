package no.navi.app

import android.os.Debug
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
import java.security.MessageDigest

/**
 * Region-to-region timing matrix against real Norway v9 packs on the selected
 * pack volume (SD long-trip-packs). Greppable PROFILE_ROW lines feed
 * docs/perf/region-to-region-graph-build.md / docs/perf/mmap-graph-search.md.
 *
 * Innlandet is not a separate Geofabrik leaf (hedmark/oppland → Ostlandet).
 * Installed stems expected: ostlandet, vestlandet, trondelag, nord-norge,
 * sorlandet.
 *
 * Tablet pass criteria (Follow-up 3): Bergen eco cold &lt;15 s, warm &lt;3 s,
 * pack_hit, ~459.71 km; eco_reweight_ms &gt; 0 on eco rows.
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
            val manifest = File(packDir, "$stem.navi-manifest.json")
            assertTrue("need $stem v9 packs under $packDir", manifest.isFile)
            val fmt =
                runCatching {
                    JSONObject(manifest.readText()).optInt("graph_format_version", -1)
                }.getOrDefault(-1)
            assertTrue("$stem must be graph_format_version=9 (got $fmt)", fmt == 9)
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
            "expansions\tnodes\tedges\tdistance_km\tpeak_rss_mb\tpeak_native_heap_mb\t" +
            "mem_avail_before_mb\troute_ok\tferry_legs\tferry_fp\tgeom_sha256"

        fun memAvailableMb(): String {
            val line =
                File("/proc/meminfo")
                    .useLines { lines -> lines.firstOrNull { it.startsWith("MemAvailable:") } }
                    ?: return "-"
            val kb =
                line
                    .substringAfter(':')
                    .trim()
                    .substringBefore(' ')
                    .toLongOrNull() ?: return "-"
            return "%.1f".format(kb / 1024.0)
        }

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
            val memBefore = memAvailableMb()
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
            // Prefer the last (chunked aggregate) ferry tokens — per-leg reports
            // only list that hop's crossing and truncate at whitespace in extractToken.
            val ferryFromReport = extractTokenLast(report, "route_ferry_fp=")
            val ferryLegsFromReport = extractTokenLast(report, "route_ferry_legs=")
            val (ferryLegs, ferryFp) =
                if (ferryFromReport.isNotBlank() && ferryFromReport != "-") {
                    ferryLegsFromReport.ifBlank { "-" } to ferryFromReport
                } else {
                    ferryFpFromSim(route.simSamplesJson)
                }
            val geomSha = sha256Hex(route.routePolyline)
            Log.i(
                TAG,
                "PACK_STAGE $name " +
                    listOf(
                        "tiles",
                        "tile_bytes",
                        "mmap_ms",
                        "pagein_ms",
                        "validate_ms",
                        "copy_ms",
                        "merge_hash_ms",
                        "merge_adj_ms",
                        "ferry_ms",
                    ).joinToString(" ") { k ->
                        "pack_stage_$k=${extract(report, "pack_stage_$k")}"
                    },
            )
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
                    memBefore,
                    ok.toString(),
                    ferryLegs,
                    ferryFp,
                    geomSha,
                ).joinToString("\t")
            rows += row
            Log.i(TAG, "PROFILE_ROW $row")
            Log.i(TAG, "PROFILE_GEOM $name geom_sha256=$geomSha polyline_chars=${route.routePolyline.length}")
            Log.i(TAG, "PROFILE_FERRY $name legs=$ferryLegs fp=$ferryFp uses=${report.contains("route_uses_ferry=true")}")
            Log.i(TAG, "PROFILE_REPORT $name\n$report")
            Log.i(
                TAG,
                "PROFILE_MEM $name MemAvailable_before_mb=$memBefore " +
                    "peak_rss_mb=${extract(report, "peak_rss_mb")} " +
                    "peak_native_heap_mb=$peakNativeMb",
            )
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
        // Coastal ferry corridor (Vestlandet): permanent matrix cases.
        // longTrip on/off × default/eco — A.4 requires both densify and single-shot.
        run("bergen_stavanger", vestPbf, STAVANGER_LAT, STAVANGER_LON, eco = false, BERGEN_LAT, BERGEN_LON, longTrip = false)
        run("bergen_stavanger", vestPbf, STAVANGER_LAT, STAVANGER_LON, eco = true, BERGEN_LAT, BERGEN_LON, longTrip = false)
        run("bergen_stavanger_lt", vestPbf, STAVANGER_LAT, STAVANGER_LON, eco = false, BERGEN_LAT, BERGEN_LON, longTrip = true)
        run("bergen_stavanger_lt", vestPbf, STAVANGER_LAT, STAVANGER_LON, eco = true, BERGEN_LAT, BERGEN_LON, longTrip = true)
        // 3+ stem corridor (Ostlandet + Trøndelag + Nord-Norge) via densify/chunk.
        run("raufoss_tromso", ostPbf, TROMSO_LAT, TROMSO_LON, eco = false, longTrip = true)

        setRoutePlanTimingEnabled(false)
        val out = rows.joinToString("\n")
        File(packDir, "region_to_region_perf_matrix.tsv").writeText(out)
        Log.i(TAG, "PROFILE_TABLE\n$out")
        // Tablet Follow-up 3: cold &lt;15 s, warm &lt;3 s (emulator was tighter).
        assertTrue(
            "Raufoss→Bergen eco cold must pack-hit and finish under 15s:\n$out",
            rows.any {
                it.startsWith("raufoss_bergen\ttrue\ttrue\t") &&
                    it
                        .split('\t')
                        .getOrNull(3)
                        ?.toLongOrNull()
                        ?.let { ms -> ms < 15_000 } == true
            },
        )
        assertTrue(
            "Raufoss→Bergen eco warm must finish under 3s:\n$out",
            rows.any {
                it.startsWith("raufoss_bergen_warm\ttrue\ttrue\t") &&
                    it
                        .split('\t')
                        .getOrNull(3)
                        ?.toLongOrNull()
                        ?.let { ms -> ms < 3_000 } == true
            },
        )
        val ecoRows =
            rows.filter { row ->
                val cols = row.split('\t')
                cols.getOrNull(0) != "route" &&
                    cols.getOrNull(1) == "true" &&
                    cols.getOrNull(2) == "true"
            }
        assertTrue("expected at least one successful eco row:\n$out", ecoRows.isNotEmpty())
        assertTrue(
            "eco_reweight_ms must be non-zero on successful eco rows:\n$out",
            ecoRows.all { row ->
                val ecoMs = row.split('\t').getOrNull(6)?.toDoubleOrNull()
                ecoMs != null && ecoMs > 0.0
            },
        )
        val stavRows =
            rows.filter {
                it.startsWith("bergen_stavanger\t") || it.startsWith("bergen_stavanger_lt\t")
            }
        assertTrue(
            "expected bergen_stavanger longTrip on+off × default+eco (4 rows):\n$out",
            stavRows.size >= 4,
        )
        assertTrue(
            "bergen_stavanger must succeed with ferries:\n$out",
            stavRows.all { row ->
                val c = row.split('\t')
                c.getOrNull(15) == "true" &&
                    (c.getOrNull(16)?.toIntOrNull() ?: 0) > 0 &&
                    (c.getOrNull(17)?.isNotBlank() == true) &&
                    c.getOrNull(17) != "-"
            },
        )
        val fps = stavRows.map { it.split('\t').getOrNull(17) }.distinct()
        assertTrue(
            "bergen_stavanger variants must share the same ferry fingerprint:\n$out",
            fps.size == 1,
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

    /** Last match of `key=…` (value until newline / `;`). Used for chunked ferry fp. */
    private fun extractTokenLast(
        report: String,
        key: String,
    ): String {
        val esc = Regex.escape(key)
        return Regex("""$esc([^\n;]+)""")
            .findAll(report)
            .lastOrNull()
            ?.groupValues
            ?.get(1)
            ?.trim()
            ?: "-"
    }

    /** Fallback when native report lacks route_ferry_fp (e.g. dig builds). */
    private fun ferryFpFromSim(simJson: String): Pair<String, String> =
        try {
            val arr = org.json.JSONArray(simJson)
            val legs = mutableListOf<String>()
            var inFerry = false
            var label = "unnamed"
            var meters = 0.0
            var prevLat: Double? = null
            var prevLon: Double? = null

            fun flush() {
                if (inFerry && meters > 50.0) {
                    legs += "%s@%.2f".format(label.replace('|', '/'), meters / 1000.0)
                }
                inFerry = false
                meters = 0.0
                label = "unnamed"
            }
            for (i in 0 until arr.length()) {
                val o = arr.getJSONObject(i)
                val hwy = o.optString("highway", "")
                val street = o.optString("street", "").ifBlank { "unnamed" }
                val lat = o.optDouble("lat")
                val lon = o.optDouble("lon")
                val isFerry = hwy.equals("ferry", ignoreCase = true)
                if (isFerry) {
                    if (!inFerry) {
                        inFerry = true
                        label = street
                        meters = 0.0
                    } else if (street != label && street != "unnamed") {
                        flush()
                        inFerry = true
                        label = street
                    }
                    if (prevLat != null && prevLon != null) {
                        val dLat = Math.toRadians(lat - prevLat!!)
                        val dLon = Math.toRadians(lon - prevLon!!)
                        val a =
                            Math.sin(dLat / 2) * Math.sin(dLat / 2) +
                                Math.cos(Math.toRadians(prevLat!!)) *
                                Math.cos(Math.toRadians(lat)) *
                                Math.sin(dLon / 2) * Math.sin(dLon / 2)
                        meters += 2.0 * 6371000.0 * Math.asin(Math.sqrt(a))
                    }
                } else if (inFerry) {
                    flush()
                }
                prevLat = lat
                prevLon = lon
            }
            flush()
            legs.size.toString() to legs.joinToString("|").ifBlank { "-" }
        } catch (_: Throwable) {
            "-" to "-"
        }

    private fun sha256Hex(input: String): String {
        val digest = MessageDigest.getInstance("SHA-256").digest(input.toByteArray(Charsets.UTF_8))
        return digest.joinToString("") { b -> "%02x".format(b) }
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
        private const val STAVANGER_LAT = 58.969975
        private const val STAVANGER_LON = 5.733107
        private const val TROMSO_LAT = 69.6492
        private const val TROMSO_LON = 18.9553
    }
}
