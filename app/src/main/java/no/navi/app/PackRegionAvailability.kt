package no.navi.app

import java.io.File

/**
 * Region-pill availability: green when the pack host lists the path (or a child)
 * in `current.json`, or when a successful local install already exists under
 * [dataDir] (packs **and** basemap PMTiles).
 *
 * Pack **download** from the host installs published packs when listed in
 * `current.json`. Server green means "published packs exist". Device-local green
 * means packs + offline basemap are verified on disk.
 */
object PackRegionAvailability {
    /**
     * Mirrors core `pack_catalog_region_id_aliases` /
     * `region_ids_match_for_catalog` in `acquisition.rs`.
     *
     * Permanent client-only exception: navi-server and Tools chips use
     * `europe/sweden/vastra_gotaland` (underscore). Keep the hyphen alias so
     * typed / legacy `vastra-gotaland` paths still match. Not a general
     * hyphen↔underscore normalizer — do not expand this list casually.
     */
    fun packCatalogRegionIdAliases(path: String): List<String> =
        when (normalize(path)) {
            "europe/sweden/vastra-gotaland" -> listOf("europe/sweden/vastra_gotaland")
            "europe/sweden/vastra_gotaland" -> listOf("europe/sweden/vastra-gotaland")
            else -> emptyList()
        }

    fun regionIdsMatchForCatalog(
        a: String,
        b: String,
    ): Boolean {
        val na = normalize(a)
        val nb = normalize(b)
        if (na == nb) return true
        return packCatalogRegionIdAliases(na).any { it == nb } ||
            packCatalogRegionIdAliases(nb).any { it == na }
    }

    /** Same rules as core `path_covered_by_ready_ids` (includes catalog aliases). */
    fun pathCoveredByReadyIds(
        path: String,
        readyIds: Collection<String>,
    ): Boolean {
        val p = normalize(path)
        if (p.isEmpty()) return false
        return readyIds.any { raw ->
            val r = normalize(raw)
            r.isNotEmpty() &&
                (
                    regionIdsMatchForCatalog(r, p) ||
                        r.startsWith("$p/") ||
                        p.startsWith("$r/")
                )
        }
    }

    fun normalize(path: String): String = path.trim().trim('/').lowercase()

    /** Leaf stem used by local convert (`ostlandet-latest`). */
    fun localStem(geofabrikPath: String): String {
        val leaf = normalize(geofabrikPath).substringAfterLast('/')
        return "$leaf-latest"
    }

    /**
     * Same stem as core `geofabrik_path_to_region_key`
     * (`europe/norway/ostlandet` → `europe_norway_ostlandet`).
     */
    fun geofabrikPathToRegionKey(path: String): String {
        val trimmed = normalize(path)
        if (trimmed.isEmpty()) return "unknown"
        return trimmed
            .replace('/', '_')
            .map { c ->
                if (c.isLetterOrDigit() || c == '_' || c == '-') c else '_'
            }.joinToString("")
            .trim('_')
            .lowercase()
    }

    fun localPmtilesReady(
        dataDir: File,
        geofabrikPath: String,
    ): Boolean {
        val candidates =
            buildList {
                add(normalize(geofabrikPath))
                addAll(packCatalogRegionIdAliases(geofabrikPath))
            }
        return candidates.any { path ->
            val key = geofabrikPathToRegionKey(path)
            File(dataDir, "pmtiles/$key.pmtiles").isFile
        }
    }

    fun localBakeReady(
        dataDir: File,
        geofabrikPath: String,
    ): Boolean {
        val candidates =
            buildList {
                add(normalize(geofabrikPath))
                addAll(packCatalogRegionIdAliases(geofabrikPath))
            }
        return candidates.any { path ->
            File(dataDir, "${localStem(path)}.navi-manifest.json").isFile
        }
    }

    /** Packs + basemap present on device for this Geofabrik path. */
    fun localInstalledReady(
        dataDir: File,
        geofabrikPath: String,
    ): Boolean = localBakeReady(dataDir, geofabrikPath) && localPmtilesReady(dataDir, geofabrikPath)

    /**
     * Country / continent chips: also green when any landsdel under the path is
     * fully installed locally (packs + PMTiles).
     */
    fun localBakeReadyUnderPrefix(
        dataDir: File,
        pathPrefix: String,
        childPaths: Iterable<String>,
    ): Boolean {
        val prefix = normalize(pathPrefix)
        if (localInstalledReady(dataDir, prefix)) return true
        return childPaths.any { child ->
            val c = normalize(child)
            (c == prefix || c.startsWith("$prefix/")) && localInstalledReady(dataDir, c)
        }
    }

    fun pillReady(
        path: String,
        serverReadyIds: Collection<String>,
        dataDir: File?,
        childPathsForLocal: Iterable<String> = emptyList(),
    ): Boolean {
        if (pathCoveredByReadyIds(path, serverReadyIds)) return true
        val dir = dataDir ?: return false
        if (childPathsForLocal.any()) {
            return localBakeReadyUnderPrefix(dir, path, childPathsForLocal)
        }
        return localInstalledReady(dir, path)
    }

    fun statusLine(
        selectedPath: String,
        serverReadyIds: Collection<String>,
        dataSource: String,
        unreachableReason: String?,
        probing: Boolean,
        dataDir: File?,
    ): String {
        if (probing) return "Checking pack server…"
        val path = normalize(selectedPath)
        val onServer = pathCoveredByReadyIds(path, serverReadyIds)
        val onDevice = dataDir != null && localInstalledReady(dataDir, path)
        val packsOnly =
            dataDir != null && localBakeReady(dataDir, path) && !localPmtilesReady(dataDir, path)
        return when {
            onServer && onDevice ->
                "Ready on pack server ($dataSource) and on device (packs + basemap)."
            onServer && packsOnly ->
                "Ready on pack server ($dataSource); packs on device — basemap still missing."
            onServer ->
                "Ready on pack server ($dataSource). Download installs packs + basemap."
            onDevice -> "Packs and basemap installed on device."
            packsOnly -> "Packs on device — basemap still missing (use Download region)."
            !unreachableReason.isNullOrBlank() && serverReadyIds.isEmpty() ->
                "Pack server offline — Geofabrik download + place index + basemap."
            else -> ""
        }
    }

    /** Tools Download button label — short when pack server lists the path. */
    fun downloadRegionButtonLabel(serverReady: Boolean): String =
        if (serverReady) {
            "Download region"
        } else {
            "Download region + build place index"
        }

    /** Green Download styling when the selected path is on the pack catalog. */
    fun downloadRegionUsesReadyStyle(serverReady: Boolean): Boolean = serverReady

    /** Green Check-OSM styling when the selected path is pill-ready. */
    fun osmCheckUsesReadyStyle(pillReady: Boolean): Boolean = pillReady
}
