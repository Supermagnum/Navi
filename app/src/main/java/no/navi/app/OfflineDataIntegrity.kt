package no.navi.app

import android.content.Context
import uniffi.navi.pmtilesListJobs
import java.io.File

/**
 * Detects offline basemap/DEM mismatches after reinstall or cleared storage so
 * the UI does not silently fall through to online Liberty.
 */
object OfflineDataIntegrity {
    /** Host-staged fixtures used by instrumented tests and adb pushes. */
    val STAGED_FIXTURES_DIR = File("/data/local/tmp/navi_fixtures")

    data class Report(
        /** Completed job rows whose local_path file is gone. */
        val missingJobFiles: List<String> = emptyList(),
        /** Region keys remembered in prefs but absent from disk/jobs. */
        val missingRememberedRegions: List<String> = emptyList(),
        /** Staged `*.pmtiles` basemaps available to restore into app storage. */
        val stagedBasemapNames: List<String> = emptyList(),
        val canRestoreFromStaging: Boolean = false,
    ) {
        val hasIssue: Boolean
            get() =
                missingJobFiles.isNotEmpty() ||
                    missingRememberedRegions.isNotEmpty() ||
                    (canRestoreFromStaging && stagedBasemapNames.isNotEmpty())

        /** Short status-chip / HUD line. */
        fun userMessage(): String? {
            if (!hasIssue) return null
            val region =
                missingRememberedRegions.firstOrNull()
                    ?: missingJobFiles.firstOrNull()?.substringBefore("_dem")
                    ?: stagedBasemapNames.firstOrNull()?.removeSuffix(".pmtiles")
                    ?: "region"
            return when {
                canRestoreFromStaging && stagedBasemapNames.isNotEmpty() ->
                    "Offline data for $region was cleared (reinstall/storage) — " +
                        "open Tools to restore staged files or re-download"
                missingJobFiles.isNotEmpty() || missingRememberedRegions.isNotEmpty() ->
                    "Offline data for $region was previously downloaded but is no longer " +
                        "available — open Tools to re-download"
                else -> null
            }
        }
    }

    fun inspect(
        context: Context,
        dataDir: File,
        stagedDir: File = STAGED_FIXTURES_DIR,
    ): Report {
        val jobs =
            runCatching { pmtilesListJobs(dataDir.absolutePath) }
                .getOrDefault(emptyList())
        val missingJobs =
            jobs
                .filter { it.status == "completed" && it.localPath.isNotBlank() }
                .filter { !File(it.localPath).isFile }
                .map { it.regionKey.ifBlank { File(it.localPath).name } }

        val remembered = MapHudPrefs.loadDownloadedPmtilesRegions(context)
        val presentKeys =
            jobs
                .filter { it.status == "completed" && File(it.localPath).isFile }
                .map { it.regionKey }
                .toSet() +
                File(dataDir, "pmtiles")
                    .listFiles()
                    .orEmpty()
                    .filter { it.isFile && it.name.endsWith(".pmtiles") }
                    .map { it.name.removeSuffix(".pmtiles") }
                    .toSet()
        val missingRemembered =
            remembered.filter { key ->
                presentKeys.none { it == key || it.startsWith("${key}_") || key.startsWith(it) }
            }

        val stagedBasemaps =
            stagedDir
                .listFiles()
                .orEmpty()
                .filter {
                    it.isFile &&
                        it.name.endsWith(".pmtiles") &&
                        !it.name.contains("_dem") &&
                        it.length() > 1_000L
                }.map { it.name }
                .sorted()
        val fullStagedBasemaps =
            stagedDir
                .listFiles()
                .orEmpty()
                .filter { OfflinePmtilesBootstrap.isFullRegionBasemap(it) && !it.name.contains("_dem") }
                .map { it.name }
                .sorted()
        val appPmtilesEmpty =
            File(dataDir, "pmtiles").listFiles()?.none {
                it.isFile && it.name.endsWith(".pmtiles") && it.length() > 1_000L
            } != false

        return Report(
            missingJobFiles = missingJobs.distinct(),
            missingRememberedRegions = missingRemembered,
            stagedBasemapNames = stagedBasemaps,
            // Only offer Tools restore when a full maxzoom-15 extract is staged —
            // mz12 test fixtures must not look like a recoverable regional download.
            canRestoreFromStaging = appPmtilesEmpty && fullStagedBasemaps.isNotEmpty(),
        )
    }
}
