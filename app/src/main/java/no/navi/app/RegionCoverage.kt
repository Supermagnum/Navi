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

    fun displayName(geofabrikPath: String): String {
        val norm = geofabrikPath.trim().trim('/').lowercase()
        return when (norm) {
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
            "europe/united-kingdom" -> "United Kingdom"
            "europe/united-kingdom/england" -> "England"
            "europe/united-kingdom/scotland" -> "Scotland"
            "europe/united-kingdom/wales" -> "Wales"
            "europe/united-kingdom/england/greater-london" -> "Greater London"
            "north-america/us" -> "United States"
            "north-america/us/west-virginia" -> "West Virginia"
            "north-america/us/nevada" -> "Nevada"
            "russia" -> "Russia"
            else -> {
                // Prefer exact leaf chip labels (Sweden län, German Länder, …)
                // before findByPath parent-country fallback, so progress strings
                // name "Västra Götaland" not "Sweden".
                leafDisplayName(norm)
                    ?: GeofabrikDownloadCatalog.findByPath(geofabrikPath)?.label
                    ?: geofabrikPath.substringAfterLast('/').ifBlank { geofabrikPath }
            }
        }
    }

    private fun leafDisplayName(normPath: String): String? {
        val leaf = normPath.substringAfterLast('/').ifBlank { return null }
        val leafAlt = leaf.replace('-', '_')
        val leafHyphen = leaf.replace('_', '-')

        fun match(pairs: List<Pair<String, String>>): String? =
            pairs
                .firstOrNull {
                    it.first.equals(leaf, ignoreCase = true) ||
                        it.first.equals(leafAlt, ignoreCase = true) ||
                        it.first.equals(leafHyphen, ignoreCase = true)
                }?.second
        return when {
            normPath.startsWith("europe/norway/") -> match(GeofabrikDownloadCatalog.norwayRegions)
            normPath.startsWith("europe/sweden/") -> match(GeofabrikDownloadCatalog.swedenRegions)
            normPath.startsWith("europe/germany/baden-wuerttemberg/") ->
                match(GeofabrikDownloadCatalog.germanyBadenWuerttembergRegions)
            normPath.startsWith("europe/germany/bayern/") ->
                match(GeofabrikDownloadCatalog.germanyBayernRegions)
            normPath.startsWith("europe/germany/nordrhein-westfalen/") ->
                match(GeofabrikDownloadCatalog.germanyNordrheinWestfalenRegions)
            normPath.startsWith("europe/germany/") -> match(GeofabrikDownloadCatalog.germanyRegions)
            else -> null
        }
    }

    fun geofabrikPathForPbfName(pbfName: String): String? {
        // Single source of truth: native pack-catalog stem map (no Norway parent-walk).
        return runCatching {
            uniffi.navi
                .geofabrikPathForPbfName(pbfName)
                .trim()
                .trim('/')
                .ifBlank { null }
        }.getOrNull()
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

    /**
     * HUD road-sign gate without Natural Earth `country_iso_at` (cold load ANRs
     * on SM-P613). Mirrors `resolve_road_sign_jurisdiction_at` coarse intent.
     */
    fun roadSignHudAllowed(
        lat: Double,
        lon: Double,
    ): Boolean {
        if (lat in 57.8..71.4 && lon in 4.0..31.5 && !eastOfNorwaySwedenBorder(lat, lon)) {
            return true
        }
        // Same Innlandet carve as core road_sign.rs (coarse SE ring).
        return lat in 59.3..63.5 && lon < 12.15
    }

    /**
     * Speed-camera opt-in prompt gate without Natural Earth. Allowed ISO set is
     * NO + GB (`SPEED_CAMERA_ALLOWED_ISO` in core).
     */
    fun speedCameraHudOptInAllowed(
        lat: Double,
        lon: Double,
    ): Boolean {
        if (lat in 57.8..71.4 && lon in 4.0..31.5 && !eastOfNorwaySwedenBorder(lat, lon)) {
            return true
        }
        // Rough UK / Ireland box (GB only for the product table).
        return lat in 49.8..61.0 && lon in -8.6..2.0
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

    fun downloadedGeofabrikPaths(dataDir: File): List<String> = downloadedGeofabrikPaths(dataDir, packDir = null)

    /**
     * Geofabrik paths with a local install under [dataDir] and optionally
     * [packDir] (long-trip-packs / Removable). Manifests count even without a
     * large PBF (pack-server stubs).
     */
    fun downloadedGeofabrikPaths(
        dataDir: File,
        packDir: File?,
    ): List<String> {
        val roots =
            buildList {
                add(dataDir)
                if (packDir != null && packDir.isDirectory && packDir.absolutePath != dataDir.absolutePath) {
                    add(packDir)
                }
                // Nested long-trip-packs under dataDir (always probed by planner).
                val nested = File(dataDir, "long-trip-packs")
                if (nested.isDirectory && nested.absolutePath != packDir?.absolutePath) {
                    add(nested)
                }
            }
        val files =
            buildList {
                for (root in roots) {
                    root.listFiles()?.forEach { f ->
                        if (f.isFile && f.name.endsWith(".osm.pbf") && f.length() > 1_000_000L) {
                            add(f)
                        }
                    }
                    root.listFiles()?.forEach { f ->
                        if (f.isFile && f.name.endsWith(".navi-manifest.json")) {
                            val stem = f.name.removeSuffix(".navi-manifest.json")
                            add(File(root, "$stem.osm.pbf"))
                        }
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
            .filter { GeofabrikDownloadCatalog.isKnownPackRegionId(it) }
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
     * download suggestion for the missing landsdel/country that covers that
     * point. Cross-landsdel trips that are already covered by multiple
     * installed extracts do not prompt — the corridor tile loader sources
     * tiles from each Ready pack in one pass.
     *
     * [packDir] is searched in addition to [dataDir] so the region screen and
     * planner agree on what is installed (long-trip-packs).
     */
    fun missingCoverage(
        waypoints: List<Waypoint>,
        dataDir: File,
        packDir: File? = null,
    ): MissingRegionCoverage? {
        if (waypoints.isEmpty()) return null
        val downloaded = downloadedGeofabrikPaths(dataDir, packDir)
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
        // Always suggest the region that covers the uncovered waypoint — never
        // a country-scale fallback when landsdels are the product unit.
        val suggested = suggestGeofabrikPath(first.lat, first.lon) ?: "europe/norway"
        val label = displayName(suggested)
        val place = first.name.ifBlank { "${first.lat}, ${first.lon}" }
        val message =
            when {
                suggested == "europe/sweden" ->
                    "$place is in Sweden, which is not downloaded. Download Sweden to plan this trip."
                else ->
                    "$label is not downloaded. Download $label to plan this trip" +
                        if (place.isNotBlank() && place != label) " ($place)." else "."
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
     * Build a [MissingRegionCoverage] from a planner `missing_regions` failure
     * (first Geofabrik path). Used when native fail-fast returns before UI
     * pre-flight, so the same download dialog / deep-link is shown.
     */
    fun missingCoverageFromRegionPath(
        geofabrikPath: String,
        role: String = "To",
        placeName: String = "",
    ): MissingRegionCoverage {
        val path = geofabrikPath.trim().trim('/')
        val label = displayName(path)
        val place = placeName.ifBlank { label }
        return MissingRegionCoverage(
            role = role,
            placeName = place,
            lat = 0.0,
            lon = 0.0,
            suggestedGeofabrikPath = path,
            crossRegion = false,
            message = "$label is not downloaded. Download $label to plan this trip.",
        )
    }

    /**
     * Pick a local region PBF for the trip. Prefer a single extract that covers
     * every waypoint; otherwise any extract that covers at least one waypoint
     * (multi-stem tile load covers the rest). Country extracts are demoted so
     * landsdel packs are preferred when both exist.
     *
     * [packDir] is optional ([LongTripPackStorage.packDownloadDir]). Candidates
     * there are searched **in addition to** [dataDir] top-level — Tools /
     * ReuseInternal extracts still live directly under [dataDir].
     *
     * Pack-server installs leave a 16 KiB stub `.osm.pbf` beside Ready graph
     * packs. Those stubs are accepted when a matching `.navi-manifest.json` (or
     * `.navi-server-install.json`) is present so planning does not fall through
     * to `/data/local/tmp` fixtures and cold-build for minutes.
     */
    fun resolvePlanPbf(
        dataDir: File,
        waypoints: List<Waypoint>,
        packDir: File? = null,
    ): File? {
        fun isCandidate(f: File): Boolean {
            if (!f.isFile || !f.name.endsWith(".osm.pbf")) return false
            if (f.length() > 1_000_000L) return true
            // Pack-server stub: accept when Ready packs sit beside it.
            val stem = f.name.removeSuffix(".osm.pbf")
            val parent = f.parentFile ?: return false
            return File(parent, "$stem.navi-manifest.json").isFile ||
                File(parent, "$stem.navi-server-install.json").isFile
        }

        val candidates =
            buildList {
                dataDir.listFiles()?.forEach { f ->
                    if (isCandidate(f)) add(f)
                }
                packDir?.listFiles()?.forEach { f ->
                    if (isCandidate(f)) add(f)
                }
                add(File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"))
                add(File("/data/local/tmp/navi_fixtures/oppland-latest.osm.pbf"))
            }.filter { it.isFile }
                .distinctBy { it.absolutePath }

        if (candidates.isEmpty()) return null

        fun coversAll(path: String): Boolean = waypoints.all { wp -> pointCovered(wp.lat, wp.lon, listOf(path)) }

        fun coversAny(path: String): Boolean = waypoints.isEmpty() || waypoints.any { wp -> pointCovered(wp.lat, wp.lon, listOf(path)) }

        fun areaRank(
            f: File,
            path: String,
        ): Double =
            when {
                path == "europe/norway" -> 1_000_000.0
                else -> f.length().toDouble().coerceAtLeast(1.0)
            }

        // Prefer non-fixture candidates so stub/SD packs beat /data/local/tmp fixtures.
        fun isFixture(f: File): Boolean = f.absolutePath.contains("/navi_fixtures/")

        val preferred = candidates.filterNot(::isFixture).ifEmpty { candidates }

        val fullCover =
            preferred.mapNotNull { f ->
                val path = geofabrikPathForPbfName(f.name) ?: return@mapNotNull null
                if (!coversAll(path)) return@mapNotNull null
                f to areaRank(f, path)
            }
        fullCover.minByOrNull { it.second }?.let { return it.first }

        val partialCover =
            preferred.mapNotNull { f ->
                val path = geofabrikPathForPbfName(f.name) ?: return@mapNotNull null
                if (!coversAny(path)) return@mapNotNull null
                f to areaRank(f, path)
            }
        partialCover.minByOrNull { it.second }?.let { return it.first }

        return listOf(
            "ostlandet-latest.osm.pbf",
            "oppland-latest.osm.pbf",
            "norway-latest.osm.pbf",
        ).firstNotNullOfOrNull { name -> preferred.firstOrNull { it.name == name } }
            ?: preferred.firstOrNull()
            ?: candidates.firstOrNull()
    }
}
