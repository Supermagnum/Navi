package no.navi.app

import uniffi.navi.pmtilesRegionBbox
import java.io.File

/**
 * Pre-flight offline coverage for route planning: detect From/To/Via points
 * that fall outside downloaded Geofabrik extracts, and suggest a download path.
 *
 * Bboxes come from the same table as offline PMTiles (`pmtilesRegionBbox`).
 */
data class MissingRegionCoverage(
    /** Waypoint role: "To", "Via", or "From". */
    val role: String,
    val placeName: String,
    val lat: Double,
    val lon: Double,
    /** Geofabrik path to offer in Tools / download prompt. */
    val suggestedGeofabrikPath: String,
    /** True when the trip needs more than one landsdel and country extract is safer. */
    val crossRegion: Boolean,
    val message: String,
)

object RegionCoverage {
    data class Waypoint(
        val role: String,
        val name: String,
        val lat: Double,
        val lon: Double,
    )

    /** Norway landsdels first (tight), country last (fallback). */
    private val SUGGEST_CANDIDATES =
        listOf(
            "europe/norway/ostlandet",
            "europe/norway/vestlandet",
            "europe/norway/trondelag",
            "europe/norway/nord-norge",
            "europe/norway/sorlandet",
            "europe/norway",
        )

    fun displayName(geofabrikPath: String): String =
        when (geofabrikPath.trim().trim('/').lowercase()) {
            "europe/norway" -> "Norway"
            "europe/norway/ostlandet" -> "Ostlandet"
            "europe/norway/vestlandet" -> "Vestlandet"
            "europe/norway/trondelag" -> "Trondelag"
            "europe/norway/nord-norge" -> "Nord-Norge"
            "europe/norway/sorlandet" -> "Sorlandet"
            else -> geofabrikPath.substringAfterLast('/').ifBlank { geofabrikPath }
        }

    fun geofabrikPathForPbfName(pbfName: String): String? {
        val leaf =
            pbfName
                .trim()
                .removeSuffix(".osm.pbf")
                .removeSuffix("-latest")
                .removeSuffix("_latest")
                .lowercase()
        return when (leaf) {
            "norway" -> "europe/norway"
            "ostlandet", "oppland" -> "europe/norway/ostlandet"
            "vestlandet" -> "europe/norway/vestlandet"
            "trondelag" -> "europe/norway/trondelag"
            "nord-norge", "nord_norge" -> "europe/norway/nord-norge"
            "sorlandet" -> "europe/norway/sorlandet"
            else -> {
                val asPath = leaf.replace('_', '/')
                when {
                    pmtilesRegionBbox(asPath) != null -> asPath
                    pmtilesRegionBbox("europe/norway/$leaf") != null -> "europe/norway/$leaf"
                    else -> null
                }
            }
        }
    }

    fun suggestGeofabrikPath(
        lat: Double,
        lon: Double,
    ): String? {
        var best: Pair<String, Double>? = null
        for (path in SUGGEST_CANDIDATES) {
            val bbox = pmtilesRegionBbox(path) ?: continue
            if (bbox.size < 4) continue
            if (!covers(bbox, lat, lon)) continue
            val area = (bbox[2] - bbox[0]).coerceAtLeast(0.0) * (bbox[3] - bbox[1]).coerceAtLeast(0.0)
            val cur = best
            if (cur == null || area < cur.second) {
                best = path to area
            }
        }
        return best?.first
    }

    fun downloadedGeofabrikPaths(dataDir: File): List<String> {
        val files =
            buildList {
                dataDir.listFiles()?.forEach { f ->
                    if (f.isFile && f.name.endsWith(".osm.pbf") && f.length() > 1_000_000L) {
                        add(f)
                    }
                }
                // Same fixture fallback Plan route can use.
                listOf(
                    File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"),
                    File("/data/local/tmp/navi_fixtures/oppland-latest.osm.pbf"),
                ).forEach { f ->
                    if (f.isFile && f.length() > 1_000_000L) add(f)
                }
            }
        return files
            .mapNotNull { geofabrikPathForPbfName(it.name) }
            .distinct()
            .sorted()
    }

    fun pointCovered(
        lat: Double,
        lon: Double,
        downloadedPaths: List<String>,
    ): Boolean = downloadedPaths.any { regionCovers(it, lat, lon) }

    private fun regionCovers(
        path: String,
        lat: Double,
        lon: Double,
    ): Boolean {
        val bbox = pmtilesRegionBbox(path) ?: return false
        if (bbox.size < 4) return false
        return covers(bbox, lat, lon)
    }

    private fun covers(
        bbox: List<Double>,
        lat: Double,
        lon: Double,
    ): Boolean = lat >= bbox[0] && lat <= bbox[2] && lon >= bbox[1] && lon <= bbox[3]

    /**
     * If any waypoint lies outside all downloaded region bboxes, return a
     * download suggestion. Prefer a country extract when From and To need
     * different landsdels (single-PBF planner cannot stitch extracts).
     */
    fun missingCoverage(
        waypoints: List<Waypoint>,
        dataDir: File,
    ): MissingRegionCoverage? {
        if (waypoints.isEmpty()) return null
        val downloaded = downloadedGeofabrikPaths(dataDir)
        val uncovered =
            waypoints.filter { wp ->
                !pointCovered(wp.lat, wp.lon, downloaded)
            }
        if (uncovered.isEmpty()) return null

        val needed =
            waypoints
                .mapNotNull { wp -> suggestGeofabrikPath(wp.lat, wp.lon) }
                .distinct()
        val crossRegion = needed.size > 1
        val first = uncovered.first()
        val destSuggest = suggestGeofabrikPath(first.lat, first.lon) ?: "europe/norway"
        val suggested =
            if (crossRegion && downloaded.none { it == "europe/norway" }) {
                "europe/norway"
            } else {
                destSuggest
            }
        val label = displayName(suggested)
        val place = first.name.ifBlank { "${first.lat}, ${first.lon}" }
        val message =
            if (crossRegion && suggested == "europe/norway") {
                "This trip leaves your downloaded map data ($place). " +
                    "Download $label (country extract) so the whole corridor is covered. " +
                    "On ~4 GB devices prefer a single region when both ends fit in one."
            } else {
                "$place is not in any downloaded area. " +
                    "Download $label ($suggested) to plan here."
            }
        return MissingRegionCoverage(
            role = first.role,
            placeName = place,
            lat = first.lat,
            lon = first.lon,
            suggestedGeofabrikPath = suggested,
            crossRegion = crossRegion,
            message = message,
        )
    }

    /**
     * Pick the best local region PBF for a trip: prefer a single extract that
     * covers every waypoint; else legacy Ostlandet names / first available.
     */
    fun resolvePlanPbf(
        dataDir: File,
        waypoints: List<Waypoint>,
    ): File? {
        val candidates =
            buildList {
                dataDir.listFiles()?.forEach { f ->
                    if (f.isFile && f.name.endsWith(".osm.pbf") && f.length() > 1_000_000L) {
                        add(f)
                    }
                }
                add(File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"))
                add(File("/data/local/tmp/navi_fixtures/oppland-latest.osm.pbf"))
            }.filter { it.isFile }
                .distinctBy { it.absolutePath }

        if (candidates.isEmpty()) return null

        fun coversAll(path: String): Boolean = waypoints.all { wp -> pointCovered(wp.lat, wp.lon, listOf(path)) }

        val scored =
            candidates.mapNotNull { f ->
                val path = geofabrikPathForPbfName(f.name) ?: return@mapNotNull null
                if (!coversAll(path)) return@mapNotNull null
                val areaRank =
                    when {
                        path == "europe/norway" -> 1_000_000.0
                        else -> f.length().toDouble()
                    }
                f to areaRank
            }
        scored.minByOrNull { it.second }?.let { return it.first }

        return listOf(
            "ostlandet-latest.osm.pbf",
            "oppland-latest.osm.pbf",
            "norway-latest.osm.pbf",
        ).firstNotNullOfOrNull { name -> candidates.firstOrNull { it.name == name } }
            ?: candidates.firstOrNull()
    }
}
