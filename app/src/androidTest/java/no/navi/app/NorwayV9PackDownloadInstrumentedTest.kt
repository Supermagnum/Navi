package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.decideRegionAcquisition
import uniffi.navi.geofabrikLatestPbfUrl
import uniffi.navi.initNativeLogging
import uniffi.navi.provisionRegionData
import java.io.File

/**
 * Explicit pack-server download / refresh for the five Norway landsdeler required
 * by [RegionToRegionPerfMatrixInstrumentedTest].
 *
 * Uses the same pack-server install entry as [RegionDownloadBackground] PACKS
 * phase: [decideRegionAcquisition] (fetches packs into [dataDir]) plus extract
 * [provisionRegionData]. Each region is an explicit download/refresh action.
 *
 * Place index / basemap are skipped: the matrix only needs v9 graph packs + PBFs.
 */
@RunWith(AndroidJUnit4::class)
class NorwayV9PackDownloadInstrumentedTest {
    private companion object {
        const val TAG = "NorwayV9PackDl"
        const val PREFERRED_FORMAT = 9

        val REGIONS =
            listOf(
                Region("europe/norway/ostlandet", "ostlandet-latest"),
                Region("europe/norway/vestlandet", "vestlandet-latest"),
                Region("europe/norway/trondelag", "trondelag-latest"),
                Region("europe/norway/nord-norge", "nord-norge-latest"),
                Region("europe/norway/sorlandet", "sorlandet-latest"),
            )
    }

    private data class Region(
        val path: String,
        val stem: String,
    ) {
        val filename: String get() = "$stem.osm.pbf"
        val manifestName: String get() = "$stem.navi-manifest.json"
    }

    @Test
    fun download_or_refresh_norway_v9_packs_into_long_trip_packs() {
        initNativeLogging()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val packDir =
            runCatching { LongTripPackStorage.packDownloadDir(context) }.getOrElse {
                File(NaviAppData.resolve(context), LongTripPackStorage.PACKS_SUBDIR)
            }
        packDir.mkdirs()
        Log.i(TAG, "packDir=${packDir.absolutePath}")

        File(NaviAppData.resolve(context), RegionDownloadBackground.QUEUE_FILE).delete()
        File(NaviAppData.resolve(context), RegionDownloadBackground.JOB_FILE).delete()
        File(packDir, RegionDownloadBackground.QUEUE_FILE).delete()
        File(packDir, RegionDownloadBackground.JOB_FILE).delete()

        for (region in REGIONS) {
            downloadOrRefresh(packDir, region)
        }

        for (region in REGIONS) {
            assertV9Ready(packDir, region)
        }
        // Drop legacy files/ostlandet-latest* when long-trip-packs holds v9.
        val filesRoot = NaviAppData.resolve(context)
        val rootMan = File(filesRoot, "ostlandet-latest.navi-manifest.json")
        val hadRootDuplicate = rootMan.isFile
        val (_, removed) =
            DownloadedRegionDelete.removeStaleRootStemDuplicate(
                filesRoot,
                packDir,
                "ostlandet-latest",
            )
        Log.i(
            TAG,
            "stale_root_cleanup ostlandet had_root=$hadRootDuplicate removed_files=$removed",
        )
        if (hadRootDuplicate) {
            assertTrue(
                "root ostlandet duplicate must be gone after refresh cleanup",
                !rootMan.isFile,
            )
        }
        Log.i(TAG, "PASS all five Norway stems at graph_format_version=$PREFERRED_FORMAT")
    }

    private fun downloadOrRefresh(
        packDir: File,
        region: Region,
    ) {
        val manifest = File(packDir, region.manifestName)
        val currentFmt = manifestFormatVersion(manifest)
        val alreadyV9 =
            manifest.isFile &&
                currentFmt == PREFERRED_FORMAT &&
                PackRegionAvailability.localBakeReady(packDir, region.path)

        if (alreadyV9) {
            Log.i(TAG, "SKIP already v9 path=${region.path}")
            ensurePbf(packDir, region)
            return
        }

        if (manifest.isFile && currentFmt != null && currentFmt < PREFERRED_FORMAT) {
            Log.i(
                TAG,
                "REFRESH_ACTION path=${region.path} local_fmt=$currentFmt → $PREFERRED_FORMAT",
            )
            clearStemPackFiles(packDir, region.stem)
        } else {
            Log.i(TAG, "DOWNLOAD_ACTION path=${region.path} (missing)")
        }

        val t0 = System.currentTimeMillis()
        // Same call RegionDownloadBackground uses: fetches packs into dataDir when
        // the catalog has the region (executeLocalConvert=false on success).
        val decision =
            decideRegionAcquisition(
                regionId = region.path,
                packServerBaseUrl = null,
                dataDir = packDir.absolutePath,
            )
        val elapsedS = (System.currentTimeMillis() - t0) / 1000
        Log.i(
            TAG,
            "DECIDE path=${region.path} elapsed_s=$elapsedS " +
                "source=${decision.source} data_source=${decision.dataSource} " +
                "execute_local=${decision.executeLocalConvert} " +
                "reason=${decision.decisionReason} msg=${decision.reason.take(240)}",
        )
        assertTrue(
            "pack-server install did not land Ready packs for ${region.path} " +
                "(execute_local=${decision.executeLocalConvert} reason=${decision.decisionReason})",
            PackRegionAvailability.localBakeReady(packDir, region.path) &&
                !decision.executeLocalConvert,
        )
        ensurePbf(packDir, region)
        assertV9Ready(packDir, region)
        Log.i(
            TAG,
            "OK path=${region.path} fmt=${manifestFormatVersion(File(packDir, region.manifestName))}",
        )
    }

    private fun ensurePbf(
        packDir: File,
        region: Region,
    ) {
        val pbf = File(packDir, region.filename)
        if (pbf.isFile && pbf.length() >= 1_000_000L) {
            Log.i(TAG, "PBF_OK path=${region.path} bytes=${pbf.length()}")
            return
        }
        val url = geofabrikLatestPbfUrl(region.path)
        Log.i(TAG, "PBF_DOWNLOAD path=${region.path} url=$url")
        val report =
            provisionRegionData(
                packDir.absolutePath,
                url,
                region.filename,
                null,
            )
        Log.i(TAG, "PBF_DONE path=${region.path} report=${report.take(240)}")
        assertTrue(
            "extract download failed for ${region.path}: ${report.take(200)}",
            report.contains("PASS", ignoreCase = true) &&
                pbf.isFile &&
                pbf.length() >= 1_000_000L,
        )
    }

    private fun assertV9Ready(
        packDir: File,
        region: Region,
    ) {
        val manifest = File(packDir, region.manifestName)
        assertTrue("missing manifest ${manifest.name}", manifest.isFile)
        assertEquals(
            "expected v$PREFERRED_FORMAT for ${region.path}",
            PREFERRED_FORMAT,
            manifestFormatVersion(manifest),
        )
        assertTrue(
            "packs not bake-ready for ${region.path}",
            PackRegionAvailability.localBakeReady(packDir, region.path),
        )
        val pbf = File(packDir, region.filename)
        assertTrue("missing PBF ${pbf.name}", pbf.isFile && pbf.length() >= 1_000_000L)
    }

    private fun manifestFormatVersion(manifest: File): Int? {
        if (!manifest.isFile) return null
        return runCatching {
            JSONObject(manifest.readText()).optInt("graph_format_version", -1).takeIf { it >= 0 }
        }.getOrNull()
    }

    /** Remove outdated stem pack artifacts; leave the Geofabrik extract alone. */
    private fun clearStemPackFiles(
        packDir: File,
        stem: String,
    ) {
        val keepSuffixes = listOf(".osm.pbf", ".osm.pbf.partial")
        packDir.listFiles()?.forEach { f ->
            if (!f.isFile) return@forEach
            if (!f.name.startsWith(stem)) return@forEach
            if (keepSuffixes.any { f.name.endsWith(it) }) return@forEach
            Log.i(TAG, "delete outdated ${f.name} (${f.length()})")
            f.delete()
        }
    }
}
