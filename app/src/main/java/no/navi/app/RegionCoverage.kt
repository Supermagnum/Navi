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

    fun displayName(geofabrikPath: String): String =
        when (geofabrikPath.trim().trim('/').lowercase()) {
            "europe/norway" -> "Norway"
            "europe/norway/ostlandet" -> "Ostlandet"
            "europe/norway/vestlandet" -> "Vestlandet"
            "europe/norway/trondelag" -> "Trondelag"
            "europe/norway/nord-norge" -> "Nord-Norge"
            "europe/norway/sorlandet" -> "Sorlandet"
            "europe/sweden" -> "Sweden"
            "europe/finland" -> "Finland"
            "europe/germany" -> "Germany"
            "europe/france" -> "France"
            "europe/switzerland" -> "Switzerland"
            "europe/austria" -> "Austria"
            "europe/great-britain" -> "Great Britain"
            "north-america/us" -> "United States"
            "north-america/us/west-virginia" -> "West Virginia"
            "north-america/us/nevada" -> "Nevada"
            "russia" -> "Russia"
            else ->
                GeofabrikDownloadCatalog.findByPath(geofabrikPath)?.label
                    ?: geofabrikPath.substringAfterLast('/').ifBlank { geofabrikPath }
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
        val fromCore = uniffi.navi.suggestGeofabrikPath(lat, lon).trim()
        return fromCore.ifBlank { null }
    }

    /**
     * Piecewise Norway–Sweden land border longitude. East of this line at [lat]
     * is Sweden. Vertices run south to north.
     */
    fun norwaySwedenBorderLon(lat: Double): Double? {
        val pts =
            listOf(
                58.88 to 11.12,
                59.20 to 11.55,
                59.60 to 11.90,
                60.00 to 12.38,
                60.50 to 12.55,
                61.00 to 12.75,
                61.50 to 12.55,
                61.90 to 12.24,
                62.30 to 12.20,
                63.00 to 12.05,
                64.00 to 13.80,
                65.00 to 14.20,
                66.00 to 16.40,
                68.00 to 20.00,
                69.06 to 20.55,
            )
        if (lat < pts.first().first || lat > pts.last().first) return null
        for (i in 0 until pts.size - 1) {
            val (lat0, lon0) = pts[i]
            val (lat1, lon1) = pts[i + 1]
            if (lat >= lat0 && lat <= lat1) {
                val t = if (lat1 == lat0) 0.0 else (lat - lat0) / (lat1 - lat0)
                return lon0 + t * (lon1 - lon0)
            }
        }
        return null
    }

    fun eastOfNorwaySwedenBorder(
        lat: Double,
        lon: Double,
    ): Boolean {
        val border = norwaySwedenBorderLon(lat) ?: return false
        return lon > border
    }

    fun downloadedCoversIdentity(
        downloaded: String,
        identity: String?,
    ): Boolean {
        if (identity.isNullOrBlank()) return true
        val d = downloaded.trim().trim('/')
        val id = identity.trim().trim('/')
        if (d.equals(id, ignoreCase = true)) return true
        if (id.startsWith("$d/", ignoreCase = true)) return true
        return false
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
    ): Boolean {
        val identity = suggestGeofabrikPath(lat, lon)
        return downloadedPaths.any { path ->
            regionCovers(path, lat, lon) && downloadedCoversIdentity(path, identity)
        }
    }

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
        val norwayInternal =
            needed.all { it == "europe/norway" || it.startsWith("europe/norway/") }
        val suggested =
            if (crossRegion &&
                norwayInternal &&
                downloaded.none { it == "europe/norway" }
            ) {
                "europe/norway"
            } else {
                destSuggest
            }
        val label = displayName(suggested)
        val place = first.name.ifBlank { "${first.lat}, ${first.lon}" }
        val message =
            when {
                suggested == "europe/sweden" ->
                    "$place is in Sweden, which is not downloaded. Download Sweden to plan this trip."
                crossRegion && suggested == "europe/norway" ->
                    "This trip leaves your downloaded map data ($place). " +
                        "Download $label so the whole corridor is covered."
                else ->
                    "$place is not in any downloaded area. Download $label to plan here."
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
