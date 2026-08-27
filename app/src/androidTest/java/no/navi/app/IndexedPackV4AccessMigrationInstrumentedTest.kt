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
import uniffi.navi.planCarRoute
import uniffi.navi.planHikingRoute
import java.io.File
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.asin
import kotlin.math.cos
import kotlin.math.pow
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Status-only: existing Ostlandet install with a planted v3 manifest must report
 * version_mismatch under the v4 binary (same API Tools uses).
 */
@RunWith(AndroidJUnit4::class)
class IndexedPackV4AccessMigrationStatusInstrumentedTest {
    private fun dataDir(): File = NaviAppData.resolve(InstrumentationRegistry.getInstrumentation().targetContext)

    private fun ensureOstlandetPbf(): File {
        val dir = dataDir()
        OstlandetOfflineFixtures.ensureInstalled(dir)
        val dest = File(dir, "ostlandet-latest.osm.pbf")
        if (!dest.isFile || dest.length() < 100_000_000L) {
            val staged = File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf")
            check(staged.isFile && staged.length() > 100_000_000L) {
                "push ostlandet-latest.osm.pbf to /data/local/tmp/navi_fixtures"
            }
            staged.copyTo(dest, overwrite = true)
        }
        return dest
    }

    /** Plant a fingerprint-matched v3 manifest so status is version_mismatch (not missing). */
    private fun plantStaleV3Manifest(pbf: File) {
        val manFile = File(dataDir(), "ostlandet-latest.navi-manifest.json")
        // Prefer rewriting only the version on an existing manifest so tile
        // listings survive; fall back to a minimal stub when none exists.
        val man =
            if (manFile.isFile) {
                org.json.JSONObject(manFile.readText()).apply {
                    put("pbf_size_bytes", pbf.length())
                    put("pbf_modified_unix_secs", pbf.lastModified() / 1000L)
                    put("graph_format_version", 3)
                }
            } else {
                org.json.JSONObject().apply {
                    put("schema", 1)
                    put("stem", "ostlandet-latest")
                    put("pbf_filename", pbf.name)
                    put("pbf_size_bytes", pbf.length())
                    put("pbf_modified_unix_secs", pbf.lastModified() / 1000L)
                    put("graph_files", org.json.JSONObject())
                    put("graph_tiles", org.json.JSONObject())
                    put("graph_format_version", 3)
                    put("poi_barrier_file", "ostlandet-latest.navi-poi-barrier.rkyv")
                    put("poi_barrier_format_version", 2)
                    put("wetland_format_version", 1)
                    put("has_delta_h", false)
                }
            }
        manFile.writeText(man.toString(2) + "\n")
    }

    @Test
    fun ostlandetV3PackReportsVersionMismatchViaExistingStatusApi() {
        val pbf = ensureOstlandetPbf()
        plantStaleV3Manifest(pbf)
        val status = indexedMapsStatus(pbf.absolutePath, dataDir().absolutePath).trim()
        assertTrue(
            "expected version_mismatch for planted v3 manifest, got $status",
            status == "version_mismatch",
        )
        val man = File(dataDir(), "ostlandet-latest.navi-manifest.json").readText()
        assertTrue(man.contains("\"graph_format_version\": 3"))
        // Same string Tools shows when idle (no MainActivity / no background job).
        val ui = IndexedMapsBackground.uiLine(pbf, dataDir())
        assertTrue(
            "expected idle outdated uiLine for mismatch, got: $ui",
            ui.contains("outdated", ignoreCase = true),
        )
        android.util.Log.i(
            "IndexedPackV4",
            "STATUS version_mismatch confirmed ui=$ui man=$man",
        )
    }
}

/**
 * UI-only: Tools rebuild button is the affordance for version_mismatch.
 */
@RunWith(AndroidJUnit4::class)
class IndexedPackV4AccessMigrationToolsUiInstrumentedTest {
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
        // Same surface as IndexedPackV3MigrationToolsUiInstrumentedTest:
        // Tools → "Rebuild indexed maps" is the affordance; mismatch itself is
        // proven by IndexedPackV4AccessMigrationStatusInstrumentedTest.
        // Avoid planting mismatch before MainActivity: that auto-starts a
        // full-region rebuild and can starve the UI on 4 GB devices.
        val dir = dataDir()
        OstlandetOfflineFixtures.ensureInstalled(dir)
        val pbf = File(dir, "ostlandet-latest.osm.pbf")
        if (!pbf.isFile || pbf.length() < 100_000_000L) {
            val staged = File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf")
            assumeTrue(staged.isFile && staged.length() > 100_000_000L)
            staged.copyTo(pbf, overwrite = true)
        }
        assumeTrue(pbf.isFile && pbf.length() > 100_000_000L)

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
        val before = indexedMapsStatus(pbf.absolutePath, dir.absolutePath).trim()
        assertTrue(
            "Tools rebuild path must see mismatch/ready/missing, got $before",
            before == "version_mismatch" || before == "ready" || before == "missing",
        )
        android.util.Log.i(
            "IndexedPackV4",
            "TOOLS_UI rebuild button visible; status=$before ui=${IndexedMapsBackground.uiLine(pbf, dir)}",
        )
    }
}

/**
 * Full Østlandet v3→v4 tiled rebuild + pack-hit access-ban plans
 * (Torggata + Kirkebyskogen). Same discipline as OstlandetV3TiledRebuild /
 * Friisvegen seasonal pack-hit verification.
 */
@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class IndexedPackV4AccessMigrationConvertInstrumentedTest {
    companion object {
        private const val TAG = "IndexedPackV4"

        private const val TOR_START_LAT = 60.7915000
        private const val TOR_START_LON = 11.0769500
        private const val TOR_END_LAT = 60.7923500
        private const val TOR_END_LON = 11.0761000
        private val TOR_MID = 60.79195 to 11.07652

        private const val KIRK_START_LAT = 60.7782000
        private const val KIRK_START_LON = 10.6868000
        private const val KIRK_END_LAT = 60.7779000
        private const val KIRK_END_LON = 10.6890000
        private val BOLLARD = 60.7780734 to 10.6878354

        /** NVRK little-endian magic + u32 version at archive head. */
        private fun graphPackFormatVersion(file: File): Int {
            RandomAccessFile(file, "r").use { raf ->
                val buf = ByteArray(8)
                raf.readFully(buf)
                val bb = ByteBuffer.wrap(buf).order(ByteOrder.LITTLE_ENDIAN)
                val magic = bb.int
                check(magic == 0x4E56524B) { "bad magic=$magic file=${file.name}" }
                return bb.int
            }
        }

        private fun haversineM(
            lat1: Double,
            lon1: Double,
            lat2: Double,
            lon2: Double,
        ): Double {
            val rlat1 = Math.toRadians(lat1)
            val rlat2 = Math.toRadians(lat2)
            val dlat = Math.toRadians(lat2 - lat1)
            val dlon = Math.toRadians(lon2 - lon1)
            val h =
                sin(dlat / 2).pow(2.0) +
                    cos(rlat1) * cos(rlat2) * sin(dlon / 2).pow(2.0)
            return 2.0 * 6_378_100.0 * asin(sqrt(h))
        }

        private fun minDistToPolylineM(
            polyline: String,
            lat: Double,
            lon: Double,
        ): Double {
            if (polyline.isBlank()) return Double.POSITIVE_INFINITY
            var best = Double.POSITIVE_INFINITY
            for (part in polyline.split(';')) {
                val bits = part.split(',')
                if (bits.size < 2) continue
                val plo = bits[0].toDoubleOrNull() ?: continue
                val pla = bits[1].toDoubleOrNull() ?: continue
                best = minOf(best, haversineM(lat, lon, pla, plo))
            }
            return best
        }

        private fun vehicle() =
            FfiVehicleLimits(
                axleWeightKg = null,
                bogieWeightKg = null,
                heightM = null,
                widthM = null,
                lengthM = null,
                totalWeightKg = null,
            )
    }

    private fun dataDir(): File = NaviAppData.resolve(InstrumentationRegistry.getInstrumentation().targetContext)

    private fun ostlandetPbf(): File? {
        val dir = dataDir()
        OstlandetOfflineFixtures.ensureInstalled(dir)
        val p = File(dir, "ostlandet-latest.osm.pbf")
        if (p.isFile && p.length() > 100_000_000L) return p
        val staged = File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf")
        if (staged.isFile && staged.length() > 100_000_000L) {
            staged.copyTo(p, overwrite = true)
            return p
        }
        return null
    }

    private fun plantStaleV3Manifest(pbf: File) {
        val manFile = File(dataDir(), "ostlandet-latest.navi-manifest.json")
        val man =
            if (manFile.isFile) {
                org.json.JSONObject(manFile.readText()).apply {
                    put("pbf_size_bytes", pbf.length())
                    put("pbf_modified_unix_secs", pbf.lastModified() / 1000L)
                    put("graph_format_version", 3)
                }
            } else {
                org.json.JSONObject().apply {
                    put("schema", 1)
                    put("stem", "ostlandet-latest")
                    put("pbf_filename", pbf.name)
                    put("pbf_size_bytes", pbf.length())
                    put("pbf_modified_unix_secs", pbf.lastModified() / 1000L)
                    put("graph_files", org.json.JSONObject())
                    put("graph_tiles", org.json.JSONObject())
                    put("graph_format_version", 3)
                    put("poi_barrier_file", "ostlandet-latest.navi-poi-barrier.rkyv")
                    put("poi_barrier_format_version", 2)
                    put("wetland_format_version", 1)
                    put("has_delta_h", false)
                }
            }
        manFile.writeText(man.toString(2) + "\n")
    }

    @Test
    fun a_fallbackUsableWhilePacksNotReady() {
        val pbf = ostlandetPbf()
        assumeTrue("Ostlandet PBF missing", pbf != null)
        plantStaleV3Manifest(pbf!!)
        val dir = dataDir()
        val before = indexedMapsStatus(pbf.absolutePath, dir.absolutePath).trim()
        assertTrue(
            "Need version_mismatch for fallback proof (got $before)",
            before == "version_mismatch",
        )
        val elevDir = File(dir, "elevation").absolutePath
        val cacheDir = File(dir, "graph-cache-ostlandet-v4-fallback").also { it.mkdirs() }.absolutePath
        val r =
            planCarRoute(
                pbf.absolutePath,
                elevDir,
                cacheDir,
                TOR_START_LAT,
                TOR_START_LON,
                TOR_END_LAT,
                TOR_END_LON,
                false,
                TravelProfile.CAR,
                false,
                false,
                false,
                vehicle(),
                false,
                dataDir = "",
            )
        android.util.Log.i(TAG, "FALLBACK ${r.report}")
        assertTrue("fallback plan failed:\n${r.report}", r.report.contains("PASS"))
        assertTrue(
            "expected pack_hit=false while packs not ready:\n${r.report}",
            r.report.contains("pack_hit=false"),
        )
        // Access fix still applies on PBF path.
        assertTrue(
            "PBF-path Car must already detour Torggata: km=${r.distanceKm}",
            r.distanceKm > 0.12,
        )
    }

    @Test
    fun b_rebuildOstlandetToV4TiledWithoutRefetch() {
        val pbf = ostlandetPbf()
        assumeTrue("Ostlandet PBF missing", pbf != null)
        val dir = dataDir()
        val beforeLen = pbf!!.length()
        val beforeMtime = pbf.lastModified()
        plantStaleV3Manifest(pbf)
        val before = indexedMapsStatus(pbf.absolutePath, dir.absolutePath).trim()
        assertTrue(
            "Need version_mismatch before rebuild (got $before)",
            before == "version_mismatch",
        )

        // No elev_dir: tiled convert skips region-wide DEM warm (same as v3 tiled rebuild).
        val t0 = System.nanoTime()
        val report = ensureIndexedMaps(pbf.absolutePath, dir.absolutePath, null)
        val elapsedMs = (System.nanoTime() - t0) / 1_000_000L
        android.util.Log.i(TAG, "ensureIndexedMaps elapsed_ms=$elapsedMs report=$report")
        assertTrue("convert failed:\n$report", report.contains("PASS"))
        assertTrue("expected real convert:\n$report", report.contains("cache_hit=false"))
        assertTrue(
            "expected tiled convert:\n$report",
            report.contains("graph_tiles=") && !report.contains("graph_tiles=0\n"),
        )
        assertTrue(
            "expected peak_rss_mb in report:\n$report",
            report.contains("peak_rss_mb="),
        )
        val after = indexedMapsStatus(pbf.absolutePath, dir.absolutePath).trim()
        assertTrue("after=$after report=$report", after == "ready")
        assertTrue(pbf.length() == beforeLen)
        assertTrue(pbf.lastModified() == beforeMtime)

        val man = File(dir, "ostlandet-latest.navi-manifest.json").readText()
        assertTrue("manifest not v5: $man", man.contains("\"graph_format_version\": 5"))
        assertTrue("manifest missing graph_tiles:\n$man", man.contains("graph_tiles"))
        android.util.Log.i(TAG, "MANIFEST $man")

        // Confirm on-disk archive preamble is NVRK v5 (motorway-grade tags live in v5 body).
        val tile =
            dir.listFiles()?.firstOrNull {
                it.isFile &&
                    it.name.startsWith("ostlandet-latest.navi-graph-car.") &&
                    it.name.endsWith(".rkyv")
            }
        assertTrue("missing car graph tile after convert", tile != null)
        val ver = graphPackFormatVersion(tile!!)
        assertTrue("tile ${tile.name} format version=$ver want 5", ver == 5)
        android.util.Log.i(TAG, "TILE_PREAMBLE file=${tile.name} version=$ver")
    }

    @Test
    fun c_torggataAndKirkebyskogenViaOstlandetPackHit() {
        val pbf = ostlandetPbf()
        assumeTrue(pbf != null)
        val dir = dataDir()
        assumeTrue(
            "packs must be ready/v5 (run b_ first); status=" +
                indexedMapsStatus(pbf!!.absolutePath, dir.absolutePath).trim(),
            indexedMapsStatus(pbf.absolutePath, dir.absolutePath).trim() == "ready",
        )
        val man = File(dir, "ostlandet-latest.navi-manifest.json").readText()
        assertTrue(man.contains("\"graph_format_version\": 5"))

        val elevDir = File(dir, "elevation").absolutePath
        val cacheDir = File(dir, "graph-cache-ostlandet-access-v4").also { it.mkdirs() }.absolutePath

        // --- Torggata Car ---
        val carTor =
            planCarRoute(
                pbf.absolutePath,
                elevDir,
                cacheDir,
                TOR_START_LAT,
                TOR_START_LON,
                TOR_END_LAT,
                TOR_END_LON,
                false,
                TravelProfile.CAR,
                false,
                false,
                false,
                vehicle(),
                false,
                dataDir = "",
            )
        android.util.Log.i(TAG, "PACK_CAR_TORGGATA ${carTor.report}")
        assertTrue("Torggata car:\n${carTor.report}", carTor.report.contains("PASS"))
        assertTrue(
            "Torggata must be pack_hit=true:\n${carTor.report}",
            carTor.report.contains("pack_hit=true"),
        )
        assertTrue(
            "Torggata pack-hit detour km=${carTor.distanceKm} (want >0.12 vs 0.066 direct)",
            carTor.distanceKm > 0.12,
        )
        val torNear =
            minDistToPolylineM(carTor.routePolyline, TOR_MID.first, TOR_MID.second)
        android.util.Log.i(TAG, "PACK_CAR_TORGGATA near_mid_m=$torNear km=${carTor.distanceKm}")

        // --- Kirkebyskogen Car ---
        val carKirk =
            planCarRoute(
                pbf.absolutePath,
                elevDir,
                cacheDir,
                KIRK_START_LAT,
                KIRK_START_LON,
                KIRK_END_LAT,
                KIRK_END_LON,
                false,
                TravelProfile.CAR,
                false,
                false,
                false,
                vehicle(),
                false,
                dataDir = "",
            )
        android.util.Log.i(TAG, "PACK_CAR_KIRKEBY ${carKirk.report}")
        assertTrue("Kirkeby car:\n${carKirk.report}", carKirk.report.contains("PASS"))
        assertTrue(
            "Kirkeby car pack_hit=true:\n${carKirk.report}",
            carKirk.report.contains("pack_hit=true"),
        )
        val carNear =
            minDistToPolylineM(carKirk.routePolyline, BOLLARD.first, BOLLARD.second)
        android.util.Log.i(TAG, "PACK_CAR_KIRKEBY near_bollard_m=$carNear km=${carKirk.distanceKm}")
        assertTrue(
            "Car pack-hit must stay clear of bollard (near_m=$carNear)",
            carNear > 50.0,
        )

        // --- Kirkebyskogen Hiking ---
        val hike =
            planHikingRoute(
                pbfPath = pbf.absolutePath,
                elevDir = elevDir,
                cacheDir = File(dir, "graph-cache-ostlandet-access-v4-foot").also { it.mkdirs() }.absolutePath,
                waypointsJson =
                    """[{"name":"A","lat":$KIRK_START_LAT,"lon":$KIRK_START_LON},""" +
                        """{"name":"B","lat":$KIRK_END_LAT,"lon":$KIRK_END_LON}]""",
                preferOfficialNetworks = false,
                preferPilgrimRoutes = false,
                dataDir = "",
            )
        android.util.Log.i(TAG, "PACK_HIKE_KIRKEBY ${hike.report}")
        assertTrue("Kirkeby hike:\n${hike.report}", hike.report.contains("PASS"))
        assertTrue(
            "Kirkeby hike pack_hit=true:\n${hike.report}",
            hike.report.contains("pack_hit=true"),
        )
        val hikeNear =
            minDistToPolylineM(hike.routePolyline, BOLLARD.first, BOLLARD.second)
        android.util.Log.i(TAG, "PACK_HIKE_KIRKEBY near_bollard_m=$hikeNear km=${hike.distanceKm}")
        assertTrue(
            "Hiking pack-hit should still pass near bollard (near_m=$hikeNear)",
            hikeNear < 40.0,
        )
    }
}
