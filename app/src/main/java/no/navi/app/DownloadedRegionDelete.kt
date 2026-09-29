package no.navi.app

import android.content.Context
import android.util.Log
import java.io.File

/**
 * User-initiated delete of one downloaded Geofabrik region.
 *
 * Removes pack artifacts (internal [NaviAppData] and redirected
 * [LongTripPackStorage.PACKS_SUBDIR]), basemap/DEM PMTiles, graph-cache dirs
 * for the stem, place-index rows + ready stamp, and download bookkeeping.
 *
 * Distinct from SD eject ([LongTripPackStorage.handleVolumeUnavailable]): eject
 * only scrubs incomplete pack writes and marks long-trip [State.Unavailable];
 * it never clears the internal place index or installed finals.
 */
object DownloadedRegionDelete {
    private const val TAG = "RegionDelete"

    data class Result(
        val ok: Boolean,
        val message: String,
        val bytesFreed: Long,
        val filesRemoved: Int,
    )

    /**
     * Why delete must wait. Empty = safe to proceed.
     * Mid-download / mid-index / active long-trip corridor all block.
     */
    fun blockReason(
        geofabrikPath: String,
        dataDir: File,
    ): String? {
        val path = PackRegionAvailability.normalize(geofabrikPath)
        if (path.isEmpty()) return "Select a Geofabrik path first."
        if (!hasLocalInstall(dataDir, path) && !hasRedirectedPacks(dataDir, path)) {
            // Still allow delete when only bookkeeping/place-index remains.
            val stamp = PlaceIndexReady.isReady(dataDir, path)
            val jobMatches =
                RegionDownloadBackground.loadJob(dataDir)?.let {
                    PackRegionAvailability.regionIdsMatchForCatalog(
                        it.geofabrikPath.ifBlank {
                            RegionCoverage.geofabrikPathForPbfName(it.filename).orEmpty()
                        },
                        path,
                    )
                } == true
            if (!stamp && !jobMatches) {
                return "Nothing installed for ${RegionCoverage.displayName(path)}."
            }
        }
        if (RegionDownloadBackground.isRunning()) {
            val active = PackRegionAvailability.normalize(RegionDownloadBackground.activeRegionPath())
            if (active.isNotEmpty() &&
                PackRegionAvailability.regionIdsMatchForCatalog(active, path)
            ) {
                return "Cannot delete while this region is downloading or indexing."
            }
        }
        if (PlaceIndexBackground.isRunning()) {
            val active = PackRegionAvailability.normalize(PlaceIndexBackground.activeRegionId())
            if (active.isNotEmpty() &&
                PackRegionAvailability.regionIdsMatchForCatalog(active, path)
            ) {
                return "Cannot delete while place index is building for this region."
            }
        }
        val plan = LongTripCoordinator.currentPlan()
        if (LongTripCoordinator.isEnabled() && plan != null) {
            val inCorridor =
                plan.regionsInOrder.any {
                    PackRegionAvailability.regionIdsMatchForCatalog(it, path)
                }
            if (inCorridor) {
                return "Cannot delete a region used by the active long-trip plan. " +
                    "Turn off Long trip first."
            }
        }
        return null
    }

    fun hasLocalInstall(
        dataDir: File,
        geofabrikPath: String,
    ): Boolean =
        PackRegionAvailability.localBakeReady(dataDir, geofabrikPath) ||
            PackRegionAvailability.localPmtilesReady(dataDir, geofabrikPath) ||
            PackRegionAvailability.resolvePbfForRegion(dataDir, geofabrikPath) != null

    private fun hasRedirectedPacks(
        dataDir: File,
        geofabrikPath: String,
    ): Boolean {
        // dataDir is internal; redirected packs live under sibling long-trip-packs
        // or an absolute packDownloadDir when Context is available — callers pass
        // extraDirs. Here we only probe the conventional internal redirect.
        val redirect = File(dataDir, LongTripPackStorage.PACKS_SUBDIR)
        return PackRegionAvailability.localBakeReady(redirect, geofabrikPath)
    }

    /**
     * Delete [geofabrikPath] from [dataDir] and optional [extraPackDirs]
     * (e.g. removable long-trip pack root). Clears place-index rows for the
     * region under internal [dataDir] only.
     */
    fun delete(
        context: Context?,
        dataDir: File,
        geofabrikPath: String,
        extraPackDirs: List<File> = emptyList(),
    ): Result {
        val path = PackRegionAvailability.normalize(geofabrikPath)
        blockReason(path, dataDir)?.let { reason ->
            return Result(ok = false, message = reason, bytesFreed = 0L, filesRemoved = 0)
        }

        val packDirs =
            linkedSetOf(dataDir).apply {
                add(File(dataDir, LongTripPackStorage.PACKS_SUBDIR))
                extraPackDirs.forEach { add(it) }
                if (context != null) {
                    runCatching { LongTripPackStorage.packDownloadDir(context) }
                        .getOrNull()
                        ?.let { add(it) }
                }
            }

        var bytes = 0L
        var count = 0
        val stems = leafStems(path)
        val regionKeys = regionKeys(path)

        for (dir in packDirs) {
            if (!dir.isDirectory) continue
            for (stem in stems) {
                val (b, n) = deleteStemArtifacts(dir, stem)
                bytes += b
                count += n
            }
        }

        // Basemap + DEM under internal pmtiles/ (and any pack dir that hosts them).
        for (dir in packDirs) {
            val pm = File(dir, "pmtiles")
            if (!pm.isDirectory) continue
            for (key in regionKeys) {
                for (name in listOf("$key.pmtiles", "${key}_dem.pmtiles")) {
                    val f = File(pm, name)
                    val (b, n) = deleteFile(f)
                    bytes += b
                    count += n
                }
            }
        }

        for (stem in stems) {
            val (b, n) = deleteGraphCaches(dataDir, stem)
            bytes += b
            count += n
        }

        // Place index + ready stamp (internal only).
        PlaceIndexReady.clearReady(dataDir, path)
        for (alias in PackRegionAvailability.packCatalogRegionIdAliases(path)) {
            PlaceIndexReady.clearReady(dataDir, alias)
        }

        clearRegionMetaIfMatches(dataDir, path)
        RegionDownloadBackground.clearBookkeepingForRegion(dataDir, path)

        if (context != null) {
            for (key in regionKeys) {
                MapHudPrefs.forgetDownloadedPmtilesRegion(context, key)
            }
        }

        val label = RegionCoverage.displayName(path)
        val freed = formatBytesShort(bytes)
        val msg =
            if (count == 0) {
                "Cleared bookkeeping for $label (no pack files found)."
            } else {
                "Deleted $label — removed $count files ($freed freed)."
            }
        Log.i(TAG, "delete path=$path files=$count bytes=$bytes")
        return Result(ok = true, message = msg, bytesFreed = bytes, filesRemoved = count)
    }

    internal fun leafStems(geofabrikPath: String): List<String> {
        val paths =
            buildList {
                add(PackRegionAvailability.normalize(geofabrikPath))
                addAll(PackRegionAvailability.packCatalogRegionIdAliases(geofabrikPath))
                val extract =
                    GeofabrikDownloadCatalog.extractPathForPbf(
                        GeofabrikDownloadCatalog.canonicalizePath(geofabrikPath),
                    )
                if (extract.isNotBlank()) add(extract)
            }.map { PackRegionAvailability.normalize(it) }
                .filter { it.isNotEmpty() }
                .distinct()
        return paths.map { PackRegionAvailability.localStem(it) }.distinct()
    }

    internal fun regionKeys(geofabrikPath: String): List<String> {
        val paths =
            buildList {
                add(PackRegionAvailability.normalize(geofabrikPath))
                addAll(PackRegionAvailability.packCatalogRegionIdAliases(geofabrikPath))
            }.map { PackRegionAvailability.normalize(it) }
                .filter { it.isNotEmpty() }
                .distinct()
        return paths.map { PackRegionAvailability.geofabrikPathToRegionKey(it) }.distinct()
    }

    /**
     * Delete every file/dir in [packDir] that belongs to [stem]
     * (`ostlandet-latest.navi-manifest.json`, graphs, partials, fetch staging).
     */
    internal fun deleteStemArtifacts(
        packDir: File,
        stem: String,
    ): Pair<Long, Int> {
        if (stem.isBlank() || !packDir.isDirectory) return 0L to 0
        var bytes = 0L
        var count = 0
        val prefix = "$stem."
        val files = packDir.listFiles() ?: return 0L to 0
        for (f in files) {
            val name = f.name
            val match =
                name == stem ||
                    name.startsWith(prefix) ||
                    name.startsWith("$stem.osm.pbf") ||
                    name == ".pack-fetch-$stem.partial" ||
                    name.startsWith(".pack-fetch-$stem.")
            if (!match) continue
            val (b, n) = deleteRecursivelyCounted(f)
            bytes += b
            count += n
        }
        return bytes to count
    }

    internal fun deleteGraphCaches(
        dataDir: File,
        stem: String,
    ): Pair<Long, Int> {
        if (stem.isBlank() || !dataDir.isDirectory) return 0L to 0
        var bytes = 0L
        var count = 0
        val files = dataDir.listFiles() ?: return 0L to 0
        for (f in files) {
            val name = f.name
            // graph-cache, graph-cache-foot are shared — only stem-scoped dirs.
            if (!name.startsWith("graph-cache")) continue
            if (name.contains(stem)) {
                val (b, n) = deleteRecursivelyCounted(f)
                bytes += b
                count += n
            }
        }
        return bytes to count
    }

    private fun clearRegionMetaIfMatches(
        dataDir: File,
        geofabrikPath: String,
    ) {
        val meta = File(dataDir, "region_meta.json")
        if (!meta.isFile) return
        val text = runCatching { meta.readText() }.getOrNull() ?: return
        val bound =
            Regex(""""geofabrik_region"\s*:\s*"([^"]+)"""")
                .find(text)
                ?.groupValues
                ?.getOrNull(1)
                ?.let { PackRegionAvailability.normalize(it) }
                .orEmpty()
        if (bound.isEmpty()) return
        if (PackRegionAvailability.regionIdsMatchForCatalog(bound, geofabrikPath)) {
            runCatching { meta.delete() }
            Log.i(TAG, "removed region_meta.json for $geofabrikPath")
        }
    }

    private fun deleteFile(f: File): Pair<Long, Int> {
        if (!f.exists()) return 0L to 0
        return deleteRecursivelyCounted(f)
    }

    private fun deleteRecursivelyCounted(f: File): Pair<Long, Int> {
        if (!f.exists()) return 0L to 0
        var bytes = 0L
        var count = 0
        if (f.isDirectory) {
            f.listFiles()?.forEach { child ->
                val (b, n) = deleteRecursivelyCounted(child)
                bytes += b
                count += n
            }
            if (f.delete()) count++
        } else {
            bytes += f.length().coerceAtLeast(0L)
            if (f.delete()) count++
        }
        return bytes to count
    }
}
