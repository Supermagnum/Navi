package no.navi.app

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import org.json.JSONArray
import org.json.JSONObject
import uniffi.navi.FfiPmtilesJob
import uniffi.navi.pmtilesListCovering
import uniffi.navi.pmtilesListJobs
import java.io.File
import java.io.FileOutputStream
import kotlin.math.max
import kotlin.math.min

/**
 * Resolves which MapLibre style to load: live OpenFreeMap Liberty, Liberty with
 * Mapterhorn DEM hillshade (opt-in 3D), or a local Protomaps PMTiles style when
 * a completed extract covers the camera.
 *
 * See [docs/map-styles.md].
 */
object BasemapStyleResolver {
    const val LIBERTY_URL = "https://tiles.openfreemap.org/styles/liberty"

    /**
     * Opt-in “3D” is Mapterhorn DEM **hillshade** only. MapLibre Native has no
     * mesh `terrain` / `sky` — see [MapterhornTerrain]. Camera stays flat (no tilt).
     */
    @Deprecated("3D no longer tilts the camera; always 0", ReplaceWith("0.0"))
    const val TERRAIN_VIEW_TILT = 0.0

    @Deprecated("Renamed; unused", ReplaceWith("0.0"))
    const val OPENFREEMAP_3D_PITCH = 0.0

    @Deprecated("Hardcoded liberty-3d bearing removed; camera follows user rotation modes")
    const val OPENFREEMAP_3D_BEARING = 0.0

    private const val ASSET_STYLE_ROOT = "map-styles/protomaps-light"
    private const val PREPARED_DIR = "map-styles/protomaps-light"
    const val WORLD_OVERVIEW_REGION_KEY = "world_overview"
    const val WORLD_OVERVIEW_MAX_ZOOM = 6
    const val MAX_REGIONAL_SOURCES = 3
    const val OVERVIEW_HANDOVER_MAXZOOM = 7.0
    const val REGIONAL_HANDOVER_MINZOOM = 6.0
    const val PROTOMAPS_PLANET_FALLBACK = "https://build.protomaps.com/20260722.pmtiles"

    enum class StyleKind {
        OnlineLiberty,
        Online3d,
        OfflineProtomaps,
    }

    data class ResolvedStyle(
        val kind: StyleKind,
        /** Path or URL passed to [org.maplibre.android.maps.MapLibreMap.setStyle]. */
        val styleUri: String,
        val coveringJob: FfiPmtilesJob? = null,
        val note: String? = null,
        /** Camera tilt when hillshade 3D is active (0 for flat 2D). */
        val cameraPitch: Double = 0.0,
        val cameraBearing: Double? = null,
        /** When true, [MapterhornTerrain.attach] after the vector style loads. */
        val attachMapterhornTerrain: Boolean = false,
        /**
         * DEM source for hillshade: online TileJSON or local `pmtiles://file://…`.
         * Null when 3D is not requested / unavailable.
         */
        val demSourceUri: String? = null,
        /** True when the online planet is mounted under local archives. */
        val onlineUnderlay: Boolean = false,
        /** Regional archives mounted as native sources (at most three). */
        val overlayArchives: List<FfiPmtilesJob> = emptyList(),
        /** Always-on z0–z6 world overview, if present. */
        val overviewArchive: FfiPmtilesJob? = null,
        /** Stable mount key: overview + top-3 regionals + online flag. */
        val mountedKey: String = "",
    )

    fun hasNetwork(context: Context): Boolean {
        val airplane =
            android.provider.Settings.Global.getInt(
                context.contentResolver,
                android.provider.Settings.Global.AIRPLANE_MODE_ON,
                0,
            ) != 0
        val cm =
            context.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
                ?: run {
                    val usable = networkUsable(airplane, hasInternet = false)
                    Fu49MapDiag.logNetworkDecision(airplane, false, usable, Fu49MapDiag.forceOffline)
                    return usable && !Fu49MapDiag.forceOffline
                }
        val network = cm.activeNetwork
        val caps = network?.let { cm.getNetworkCapabilities(it) }
        val hasInternet =
            caps != null &&
                (
                    caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) ||
                        caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED) ||
                        caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) ||
                        caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) ||
                        caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) ||
                        caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN)
                )
        val usable = networkUsable(airplane, hasInternet)
        Fu49MapDiag.logNetworkDecision(airplane, hasInternet, usable, Fu49MapDiag.forceOffline)
        if (Fu49MapDiag.forceOffline) return false
        return usable
    }

    /**
     * Airplane mode is offline even when the emulator still reports Wi‑Fi with
     * INTERNET. The online map must not stay mounted then.
     */
    internal fun networkUsable(
        airplane: Boolean,
        hasInternet: Boolean,
    ): Boolean = !airplane && hasInternet

    /**
     * True when [regionKey] / [localPath] names a Mapterhorn DEM archive
     * (`*_dem` / `*_dem.pmtiles`). Those must never be used as the vector
     * Protomaps style source — MapLibre would parse raster bytes as MVT and
     * paint a blank map.
     */
    fun isDemArchive(
        regionKey: String,
        localPath: String = "",
    ): Boolean {
        val key = regionKey.trim().lowercase()
        if (key.endsWith("_dem")) return true
        val name = File(localPath).name.lowercase()
        return name.endsWith(MapterhornTerrain.DEM_FILE_SUFFIX) ||
            name.contains("_dem.pmtiles")
    }

    /**
     * First completed covering job eligible as the **vector** offline basemap.
     * DEM jobs are skipped even when they are newer (`created_at DESC`). A
     * missing, empty or invalid file does not block later coverings.
     */
    fun selectVectorCoveringJob(coveringJobs: List<FfiPmtilesJob>): FfiPmtilesJob? =
        coveringJobs.firstOrNull { job ->
            !isDemArchive(job.regionKey, job.localPath) &&
                PmtilesArchiveGate.isUsable(File(job.localPath), job.regionKey)
        }

    /**
     * Prefer local PMTiles when a completed job covers [lat]/[lon].
     * Opt-in 3D with a local `{region}_dem.pmtiles` beside the basemap uses
     * **downloaded Protomaps + Mapterhorn DEM hillshade** (no network).
     * Without a local DEM, keep the offline vector basemap in flat 2D (do **not**
     * abandon PMTiles for online Liberty) — surface a degrade note instead.
     */
    fun resolve(
        context: Context,
        dataDir: File,
        lat: Double,
        lon: Double,
        prefer3d: Boolean,
        vulkanAvailable: Boolean,
        forceOnline2d: Boolean = false,
        viewSouth: Double? = null,
        viewWest: Double? = null,
        viewNorth: Double? = null,
        viewEast: Double? = null,
    ): ResolvedStyle {
        val want3d = prefer3d && vulkanAvailable
        val view =
            if (viewSouth != null && viewWest != null && viewNorth != null && viewEast != null) {
                Viewport(viewSouth, viewWest, viewNorth, viewEast)
            } else {
                null
            }

        val forcedPath = NaviMapTestHooks.forceBasemapSource
        if (!forcedPath.isNullOrBlank()) {
            val forcedJob =
                FfiPmtilesJob(
                    id = "forced",
                    regionKey = "forced",
                    url = "",
                    localPath = forcedPath,
                    bytesReceived = 0uL,
                    totalBytes = null,
                    status = "completed",
                    paused = false,
                    minLat = view?.south ?: (lat - 2.0),
                    minLon = view?.west ?: (lon - 2.0),
                    maxLat = view?.north ?: (lat + 2.0),
                    maxLon = view?.east ?: (lon + 2.0),
                )
            val mounted =
                MountedSources(
                    overview = null,
                    regionals = listOf(forcedJob),
                    includeOnline = false,
                )
            val uri =
                prepareMountedStyle(context, mounted, demFor3d = null)
                    ?: return fallbackOnline(
                        context,
                        dataDir,
                        prefer3d,
                        vulkanAvailable,
                        "forced source was not a map",
                    )
            return ResolvedStyle(
                kind = StyleKind.OfflineProtomaps,
                styleUri = uri,
                coveringJob = forcedJob,
                note = "forced source",
                overlayArchives = listOf(forcedJob),
                mountedKey = mounted.key(),
            )
        }
        if (!forceOnline2d) {
            val coveringJobs =
                runCatching {
                    pmtilesListCovering(dataDir.absolutePath, lat, lon)
                }.getOrDefault(emptyList())
            val allJobs =
                runCatching { pmtilesListJobs(dataDir.absolutePath) }.getOrDefault(emptyList())
            val intersecting =
                selectIntersectingVectorJobs(allJobs, view, coveringJobs)
            val covering = intersecting.firstOrNull { job ->
                pointInJobBbox(lat, lon, job)
            } ?: intersecting.firstOrNull() ?: selectVectorCoveringJob(coveringJobs)
            val blockedCovering =
                coveringJobs.filter { job ->
                    !isDemArchive(job.regionKey, job.localPath) &&
                        job.localPath.isNotBlank() &&
                        !PmtilesArchiveGate.isUsable(File(job.localPath), job.regionKey)
                }

            val overview = findWorldOverview(allJobs, dataDir)
            val regionals = selectMountedRegionals(intersecting, view)
            val archives = if (regionals.isNotEmpty()) regionals else listOfNotNull(covering)
            val mountedRegionals =
                if (archives.isNotEmpty()) {
                    selectMountedRegionals(archives, view)
                } else {
                    emptyList()
                }
            val extendsBeyond =
                view != null &&
                    viewportExtendsBeyondArchives(view, mountedRegionals)
            val network = hasNetwork(context)
            val mountOnline = shouldMountOnlineUnderlay(network, extendsBeyond)
            val mounted =
                if (Fu49MapDiag.simpleMount) {
                    val one = covering ?: mountedRegionals.firstOrNull()
                    val regs =
                        if (Fu49MapDiag.enableSecondRegional) {
                            mountedRegionals.take(2).ifEmpty { listOfNotNull(one) }
                        } else {
                            listOfNotNull(one)
                        }
                    MountedSources(
                        overview = if (Fu49MapDiag.enableOverview) overview else null,
                        regionals = regs,
                        includeOnline = Fu49MapDiag.enableOnline && mountOnline,
                    )
                } else {
                    MountedSources(
                        overview = overview,
                        regionals = mountedRegionals,
                        includeOnline = mountOnline,
                    )
                }

            if (mounted.overview != null || mounted.regionals.isNotEmpty()) {
                val primary = covering ?: mounted.regionals.firstOrNull() ?: mounted.overview
                val localDem =
                    primary?.localPath?.let { MapterhornTerrain.localDemBesideBasemap(it) }
                val offlineFlags =
                    offlineCoveringFlags(
                        want3d = want3d,
                        localDemPresent = localDem != null,
                    )
                val uri =
                    prepareMountedStyle(
                        context,
                        mounted,
                        demFor3d = if (offlineFlags.offline3d) localDem else null,
                    )
                        ?: return fallbackOnline(
                            context,
                            dataDir,
                            prefer3d,
                            vulkanAvailable,
                            "offline style prepare failed",
                        )
                return ResolvedStyle(
                    kind = StyleKind.OfflineProtomaps,
                    styleUri = uri,
                    coveringJob = primary,
                    note =
                        when {
                            mountOnline && mounted.regionals.isNotEmpty() ->
                                "Offline map with online fill beyond archives"
                            mountOnline -> "World overview with online map underneath"
                            else -> offlineFlags.note
                        },
                    cameraPitch = offlineFlags.cameraPitch,
                    attachMapterhornTerrain = false,
                    demSourceUri =
                        if (offlineFlags.offline3d && localDem != null) {
                            MapterhornTerrain.ensureLocalDemTileJsonUrl(localDem)
                        } else {
                            null
                        },
                    onlineUnderlay = mountOnline,
                    overlayArchives = mounted.regionals,
                    overviewArchive = mounted.overview,
                    mountedKey = mounted.key(),
                )
            }

            if (blockedCovering.isNotEmpty()) {
                val first = blockedCovering.first()
                val region =
                    first.regionKey.ifBlank {
                        File(first.localPath).nameWithoutExtension
                    }
                val why =
                    PmtilesArchiveGate.rejectionReason(File(first.localPath), first.regionKey)
                        ?: "missing or invalid archive"
                return fallbackOnline(
                    context,
                    dataDir,
                    prefer3d,
                    vulkanAvailable,
                    note =
                        "This region has no offline map ($why). Open Tools to download $region again.",
                )
            }
        }

        return fallbackOnline(context, dataDir, prefer3d, vulkanAvailable, note = null)
    }

    data class Viewport(
        val south: Double,
        val west: Double,
        val north: Double,
        val east: Double,
    )

    /**
     * Completed, usable vector jobs whose bbox intersects [view]. When [view]
     * is null, the camera-centre covering list is used (one-archive fallback).
     */
    internal fun selectIntersectingVectorJobs(
        allJobs: List<FfiPmtilesJob>,
        view: Viewport?,
        coveringJobs: List<FfiPmtilesJob>,
    ): List<FfiPmtilesJob> {
        val usable =
            allJobs.filter { job ->
                job.status == "completed" &&
                    !isDemArchive(job.regionKey, job.localPath) &&
                    !isWorldOverviewRegion(job.regionKey, job.localPath) &&
                    job.localPath.isNotBlank() &&
                    PmtilesArchiveGate.isUsable(File(job.localPath), job.regionKey)
            }
        if (view == null) {
            return selectVectorCoveringJob(coveringJobs)?.let { listOf(it) } ?: emptyList()
        }
        val hit =
            usable.filter { job ->
                val minLat = job.minLat ?: return@filter false
                val minLon = job.minLon ?: return@filter false
                val maxLat = job.maxLat ?: return@filter false
                val maxLon = job.maxLon ?: return@filter false
                bboxIntersects(
                    view.south,
                    view.west,
                    view.north,
                    view.east,
                    minLat,
                    minLon,
                    maxLat,
                    maxLon,
                )
            }
        if (hit.isNotEmpty()) return hit.distinctBy { it.localPath }
        // Camera centre may sit on a covering even when the stored bbox missed
        // the viewport (stale job bbox). Keep that archive.
        return selectVectorCoveringJob(coveringJobs)?.let { listOf(it) } ?: emptyList()
    }

    internal fun viewportExtendsBeyondArchives(
        view: Viewport,
        archives: List<FfiPmtilesJob>,
    ): Boolean {
        if (archives.isEmpty()) return true
        val samples =
            listOf(
                view.south to view.west,
                view.south to view.east,
                view.north to view.west,
                view.north to view.east,
                (view.south + view.north) / 2.0 to view.west,
                (view.south + view.north) / 2.0 to view.east,
                view.south to (view.west + view.east) / 2.0,
                view.north to (view.west + view.east) / 2.0,
            )
        return samples.any { (lat, lon) -> archives.none { pointInJobBbox(lat, lon, it) } }
    }

    internal fun shouldMountOnlineUnderlay(
        hasNetwork: Boolean,
        viewportExtendsBeyond: Boolean,
    ): Boolean = hasNetwork && viewportExtendsBeyond

    internal fun pointInJobBbox(
        lat: Double,
        lon: Double,
        job: FfiPmtilesJob,
    ): Boolean {
        val minLat = job.minLat ?: return false
        val minLon = job.minLon ?: return false
        val maxLat = job.maxLat ?: return false
        val maxLon = job.maxLon ?: return false
        return pointInBbox(lat, lon, minLat, minLon, maxLat, maxLon)
    }

    fun isWorldOverviewRegion(
        regionKey: String,
        localPath: String = "",
    ): Boolean {
        val key = regionKey.trim().lowercase()
        if (key == WORLD_OVERVIEW_REGION_KEY ||
            key.endsWith("/$WORLD_OVERVIEW_REGION_KEY") ||
            key.endsWith("_$WORLD_OVERVIEW_REGION_KEY")
        ) {
            return true
        }
        return File(localPath).nameWithoutExtension.equals(WORLD_OVERVIEW_REGION_KEY, ignoreCase = true)
    }

    internal fun findWorldOverview(
        allJobs: List<FfiPmtilesJob>,
        dataDir: File,
    ): FfiPmtilesJob? {
        val fromJobs =
            allJobs.firstOrNull { job ->
                job.status == "completed" &&
                    isWorldOverviewRegion(job.regionKey, job.localPath) &&
                    job.localPath.isNotBlank() &&
                    PmtilesArchiveGate.isUsable(File(job.localPath), job.regionKey)
            }
        if (fromJobs != null) return fromJobs
        val onDisk = File(File(dataDir, "pmtiles"), "world_overview.pmtiles")
        if (!PmtilesArchiveGate.isUsable(onDisk, WORLD_OVERVIEW_REGION_KEY)) return null
        return FfiPmtilesJob(
            id = WORLD_OVERVIEW_REGION_KEY,
            regionKey = WORLD_OVERVIEW_REGION_KEY,
            url = "",
            localPath = onDisk.absolutePath,
            bytesReceived = onDisk.length().toULong(),
            totalBytes = onDisk.length().toULong(),
            status = "completed",
            paused = false,
            minLat = -85.0511287,
            minLon = -180.0,
            maxLat = 85.0511287,
            maxLon = 180.0,
        )
    }

    /**
     * Up to [MAX_REGIONAL_SOURCES] intersecting archives, largest viewport
     * overlap first. The world overview is never a regional slot.
     */
    internal fun selectMountedRegionals(
        intersecting: List<FfiPmtilesJob>,
        view: Viewport?,
    ): List<FfiPmtilesJob> {
        val regionals =
            intersecting.filter { job ->
                !isWorldOverviewRegion(job.regionKey, job.localPath) &&
                    !isDemArchive(job.regionKey, job.localPath)
            }
        if (regionals.isEmpty()) return emptyList()
        val specific = preferSpecificArchives(regionals, view)
        if (view == null || specific.size <= MAX_REGIONAL_SOURCES) {
            return specific.take(MAX_REGIONAL_SOURCES)
        }
        return specific
            .sortedWith(
                compareByDescending<FfiPmtilesJob> { intersectionArea(view, it) }
                    .thenBy { it.localPath },
            ).take(MAX_REGIONAL_SOURCES)
    }

    /**
     * When a country-wide archive and a nested regional both cover the view,
     * keep the more specific one if the view sits entirely inside it. They
     * must not both paint the same features.
     */
    internal fun preferSpecificArchives(
        jobs: List<FfiPmtilesJob>,
        view: Viewport?,
    ): List<FfiPmtilesJob> {
        if (jobs.size <= 1) return jobs
        return jobs.filter { job ->
            val dropAsParent =
                jobs.any { other ->
                    other.localPath != job.localPath &&
                        isParentArchive(job, other) &&
                        (view == null || viewFullyInside(view, other))
                }
            val dropAsChild =
                view != null &&
                    jobs.any { other ->
                        other.localPath != job.localPath &&
                            isParentArchive(other, job) &&
                            !viewFullyInside(view, job)
                    }
            !dropAsParent && !dropAsChild
        }
    }

    internal fun isParentArchive(
        parent: FfiPmtilesJob,
        child: FfiPmtilesJob,
    ): Boolean {
        if (parent.localPath == child.localPath) return false
        val parentKey = parent.regionKey.trim().lowercase().replace('/', '_')
        val childKey = child.regionKey.trim().lowercase().replace('/', '_')
        return parentKey.isNotEmpty() &&
            (childKey.startsWith("${parentKey}_") || childKey.startsWith("$parentKey/"))
    }

    internal fun viewFullyInside(
        view: Viewport,
        job: FfiPmtilesJob,
    ): Boolean =
        pointInJobBbox(view.south, view.west, job) &&
            pointInJobBbox(view.south, view.east, job) &&
            pointInJobBbox(view.north, view.west, job) &&
            pointInJobBbox(view.north, view.east, job)

    internal fun shouldDrawOverview(
        overviewPresent: Boolean,
        includeOnline: Boolean,
    ): Boolean = overviewPresent && !includeOnline

    internal fun intersectionArea(
        view: Viewport,
        job: FfiPmtilesJob,
    ): Double {
        val minLat = job.minLat ?: return 0.0
        val minLon = job.minLon ?: return 0.0
        val maxLat = job.maxLat ?: return 0.0
        val maxLon = job.maxLon ?: return 0.0
        val south = max(view.south, minLat)
        val north = min(view.north, maxLat)
        val west = max(view.west, minLon)
        val east = min(view.east, maxLon)
        if (north <= south || east <= west) return 0.0
        return (north - south) * (east - west)
    }

    internal fun bboxIntersects(
        aSouth: Double,
        aWest: Double,
        aNorth: Double,
        aEast: Double,
        bSouth: Double,
        bWest: Double,
        bNorth: Double,
        bEast: Double,
    ): Boolean = aWest <= bEast && aEast >= bWest && aSouth <= bNorth && aNorth >= bSouth

    internal fun pointInBbox(
        lat: Double,
        lon: Double,
        minLat: Double,
        minLon: Double,
        maxLat: Double,
        maxLon: Double,
    ): Boolean = lat >= minLat && lat <= maxLat && lon >= minLon && lon <= maxLon

    data class MountedSources(
        val overview: FfiPmtilesJob?,
        val regionals: List<FfiPmtilesJob>,
        val includeOnline: Boolean,
    ) {
        fun key(): String {
            val ov = overview?.localPath ?: "no-overview"
            val regs =
                regionals
                    .map { it.localPath }
                    .sorted()
                    .joinToString("+")
                    .ifBlank { "no-region" }
            val net = if (includeOnline) "online" else "offline"
            return "native|$ov|$regs|$net"
        }
    }

    /**
     * Pure decision for an offline vector covering: use local DEM hillshade when
     * present, otherwise stay on flat OfflineProtomaps (never fall through to
     * online solely because the DEM companion is missing).
     *
     * DEM companion lookup is [MapterhornTerrain.localDemBesideBasemap]: same
     * parent dir, `{vectorStem}_dem.pmtiles`, and `length() > 1000`. A DEM saved
     * under a different stem than the vector file, or a truncated download under
     * 1000 bytes, reads as missing and triggers the flat-offline degrade path.
     */
    internal fun offlineCoveringFlags(
        want3d: Boolean,
        localDemPresent: Boolean,
    ): OfflineCoveringFlags {
        if (want3d && localDemPresent) {
            return OfflineCoveringFlags(
                offline3d = true,
                cameraPitch = TERRAIN_VIEW_TILT,
                note = "Offline Protomaps + Mapterhorn DEM hillshade",
            )
        }
        if (want3d) {
            return OfflineCoveringFlags(
                offline3d = false,
                cameraPitch = 0.0,
                note = "3D hillshade needs the terrain download; showing offline map in 2D",
            )
        }
        return OfflineCoveringFlags(
            offline3d = false,
            cameraPitch = 0.0,
            note = null,
        )
    }

    internal data class OfflineCoveringFlags(
        val offline3d: Boolean,
        val cameraPitch: Double,
        val note: String?,
    )

    private fun fallbackOnline(
        context: Context,
        dataDir: File,
        prefer3d: Boolean,
        vulkanAvailable: Boolean,
        note: String?,
    ): ResolvedStyle {
        val integrityNote =
            runCatching { OfflineDataIntegrity.inspect(context, dataDir).userMessage() }
                .getOrNull()
        if (prefer3d && vulkanAvailable) {
            return ResolvedStyle(
                kind = StyleKind.Online3d,
                styleUri = LIBERTY_URL,
                note =
                    integrityNote
                        ?: note
                        ?: "Liberty + Mapterhorn DEM hillshade",
                cameraPitch = TERRAIN_VIEW_TILT,
                attachMapterhornTerrain = true,
                demSourceUri = MapterhornTerrain.TILEJSON_URL,
            )
        }
        if (prefer3d && !vulkanAvailable) {
            return ResolvedStyle(
                kind = StyleKind.OnlineLiberty,
                styleUri = LIBERTY_URL,
                note = integrityNote ?: note ?: "3D unavailable without Vulkan; using 2D Liberty",
            )
        }
        return ResolvedStyle(
            kind = StyleKind.OnlineLiberty,
            styleUri = LIBERTY_URL,
            note = integrityNote ?: note,
        )
    }

    /**
     * Copy bundled sprites/glyphs once, rewrite style template to point at
     * `pmtiles://file://...` and local sprite/glyph paths.
     * When [demFor3d] is set, bake Mapterhorn DEM hillshade into the style JSON.
     */
    fun prepareOfflineStyle(
        context: Context,
        pmtilesAbsolutePath: String,
        demFor3d: File? = null,
        tilesUrl: String? = null,
        worldBounds: Boolean = false,
    ): String? {
        val pmFile = File(pmtilesAbsolutePath)
        if (!pmFile.isFile) return null

        val outRoot = File(context.filesDir, PREPARED_DIR)
        val assetEpoch = "v23-native-multisource"
        val epochFile = File(outRoot, ".asset_epoch")
        val needCopy =
            !outRoot.exists() ||
                !epochFile.isFile ||
                epochFile.readText() != assetEpoch
        if (needCopy) {
            if (outRoot.exists()) {
                outRoot.deleteRecursively()
            }
            copyAssetTree(context, ASSET_STYLE_ROOT, outRoot)
            epochFile.writeText(assetEpoch)
        }

        val template =
            context.assets
                .open("$ASSET_STYLE_ROOT/style.template.json")
                .bufferedReader()
                .use { it.readText() }

        val spriteBase = File(outRoot, "sprites/light").absolutePath
        val glyphsBase = File(outRoot, "fonts").absolutePath
        val pmtilesUrl = "pmtiles://file://${pmFile.absolutePath}"

        val rewritten =
            template
                .replace("__PMTILES_URL__", pmtilesUrl)
                .replace("__SPRITE__", "file://$spriteBase")
                .replace("__GLYPHS__", "file://$glyphsBase")

        // Ensure attribution survives any template edits.
        val json = JSONObject(rewritten)
        val sources = json.getJSONObject("sources")
        val pm = sources.getJSONObject("protomaps")
        if (!pm.has("attribution")) {
            pm.put("attribution", "© OpenStreetMap © Protomaps")
        }
        // Region extracts are maxzoom 15 (see DEFAULT_EXTRACT_MAX_ZOOM). If the
        // source advertises a higher maxzoom, MapLibre requests z16+ tiles that
        // do not exist and the map goes blank — including peak labels, which
        // then never appear. Pin maxzoom to the archive header so z16+ overzooms.
        readPmtilesMaxZoom(pmFile)?.let { pm.put("maxzoom", it) }
        if (worldBounds) {
            pm.put("minzoom", 0)
            pm.put(
                "bounds",
                JSONArray().put(-180.0).put(-85.0511287).put(180.0).put(85.0511287),
            )
        }
        var styleJson = json
        if (demFor3d != null && demFor3d.isFile) {
            val tileJsonUrl = MapterhornTerrain.ensureLocalDemTileJsonUrl(demFor3d)
            styleJson = MapterhornTerrain.augmentStyleJson(json, tileJsonUrl)
        }

        // Unique filename per archive (and DEM toggle). A fixed style.local.v3.json
        // URI made MainActivity skip setStyle when the camera moved into a second
        // downloaded region — MapLibre kept the first region's in-memory source
        // while disk JSON already pointed at the second PMTiles file.
        val outName =
            offlineStyleLeafName(
                pmtilesAbsolutePath = pmFile.absolutePath,
                withDem = demFor3d != null && demFor3d.isFile,
            )
        val outStyle = File(outRoot, outName)
        writeStyleAtomically(outStyle, styleJson)
        // MapLibre Native expects a URI scheme for local styles.
        return "file://${outStyle.absolutePath}"
    }

    /**
     * One native `pmtiles://file://` source per mounted archive, plus an optional
     * HTTP Protomaps planet underlay. Layers are cloned from the template once
     * per source. No proxy and no tile merge.
     */
    fun prepareMountedStyle(
        context: Context,
        mounted: MountedSources,
        demFor3d: File? = null,
    ): String? {
        val regionals =
            mounted.regionals.filter { job ->
                File(job.localPath).isFile &&
                    !isDemArchive(job.regionKey, job.localPath) &&
                    !isWorldOverviewRegion(job.regionKey, job.localPath)
            }
        val overview =
            mounted.overview?.takeIf { job ->
                File(job.localPath).isFile &&
                    isWorldOverviewRegion(job.regionKey, job.localPath)
            }
        if (overview == null && regionals.isEmpty() && !mounted.includeOnline) return null

        val outRoot = File(context.filesDir, PREPARED_DIR)
        val assetEpoch = "v24-kind-order-online"
        val epochFile = File(outRoot, ".asset_epoch")
        val needCopy =
            !outRoot.exists() ||
                !epochFile.isFile ||
                epochFile.readText() != assetEpoch
        if (needCopy) {
            if (outRoot.exists()) {
                outRoot.deleteRecursively()
            }
            copyAssetTree(context, ASSET_STYLE_ROOT, outRoot)
            epochFile.writeText(assetEpoch)
        }

        val template =
            context.assets
                .open("$ASSET_STYLE_ROOT/style.template.json")
                .bufferedReader()
                .use { it.readText() }
        val json = JSONObject(template)
        val spriteBase = File(outRoot, "sprites/light").absolutePath
        val glyphsBase = File(outRoot, "fonts").absolutePath
        json.put("sprite", "file://$spriteBase")
        json.put("glyphs", "file://$glyphsBase/{fontstack}/{range}.pbf")

        val sources = JSONObject()
        val templateLayers = json.getJSONArray("layers")
        val background = JSONArray()
        val earthLand = JSONArray()
        val water = JSONArray()
        val roads = JSONArray()
        val labels = JSONArray()
        for (i in 0 until templateLayers.length()) {
            val layer = templateLayers.getJSONObject(i)
            when (layerKind(layer)) {
                LayerKind.Background -> background.put(layer)
                LayerKind.EarthLand -> earthLand.put(layer)
                LayerKind.Water -> water.put(layer)
                LayerKind.Labels -> labels.put(layer)
                LayerKind.Roads, LayerKind.Other -> roads.put(layer)
            }
        }

        data class NamedSource(
            val id: String,
            val spec: JSONObject,
            val minZoom: Double?,
            val maxZoom: Double?,
        )

        val named = ArrayList<NamedSource>()
        if (mounted.includeOnline) {
            val planet =
                runCatching { uniffi.navi.pmtilesPlanetUrl() }
                    .getOrDefault(PROTOMAPS_PLANET_FALLBACK)
            val online = JSONObject()
            online.put("type", "vector")
            online.put("url", "pmtiles://$planet")
            online.put("attribution", "© OpenStreetMap © Protomaps")
            online.put("maxzoom", 15)
            named.add(NamedSource("online", online, minZoom = null, maxZoom = null))
        }
        val drawOverview = shouldDrawOverview(overview != null, mounted.includeOnline)
        if (drawOverview && overview != null) {
            val ov = sourceSpec(File(overview.localPath), worldBounds = true)
            val cap = if (regionals.isNotEmpty()) OVERVIEW_HANDOVER_MAXZOOM else null
            named.add(NamedSource("overview", ov, minZoom = null, maxZoom = cap))
        }
        regionals.forEachIndexed { index, job ->
            val spec = sourceSpec(File(job.localPath), worldBounds = false, job = job)
            val minZ = if (drawOverview) REGIONAL_HANDOVER_MINZOOM else null
            named.add(NamedSource("region$index", spec, minZoom = minZ, maxZoom = null))
        }
        if (named.isEmpty()) return null
        for (src in named) {
            sources.put(src.id, src.spec)
        }
        json.put("sources", sources)

        val primaryId =
            named.firstOrNull { it.id.startsWith("region") }?.id
                ?: named.firstOrNull { it.id == "overview" }?.id
                ?: named.firstOrNull()?.id
        val outLayers = JSONArray()
        for (i in 0 until background.length()) {
            outLayers.put(background.getJSONObject(i))
        }
        // Kind across sources: every earth/land, then every water, then roads,
        // then labels. Stacking one source's full set after another lets the
        // later source's land cover the earlier source's water (Hamburg z15).
        for (kindLayers in listOf(earthLand, water, roads, labels)) {
            for (src in named) {
                appendClonedLayers(
                    outLayers,
                    kindLayers,
                    src.id,
                    src.minZoom,
                    src.maxZoom,
                    keepOriginalIds = src.id == primaryId,
                )
            }
        }
        json.put("layers", outLayers)

        var styleJson = json
        if (demFor3d != null && demFor3d.isFile) {
            val tileJsonUrl = MapterhornTerrain.ensureLocalDemTileJsonUrl(demFor3d)
            styleJson = MapterhornTerrain.augmentStyleJson(json, tileJsonUrl)
        }

        val leaf = mountedStyleLeafName(mounted, demFor3d != null && demFor3d.isFile)
        val outStyle = File(outRoot, leaf)
        writeStyleAtomically(outStyle, styleJson)
        return "file://${outStyle.absolutePath}"
    }

    internal enum class LayerKind {
        Background,
        EarthLand,
        Water,
        Roads,
        Labels,
        Other,
    }

    internal fun layerKind(layer: JSONObject): LayerKind {
        val type = layer.optString("type")
        if (type == "background") return LayerKind.Background
        if (type == "symbol") return LayerKind.Labels
        val sourceLayer = layer.optString("source-layer")
        val id = layer.optString("id")
        if (sourceLayer == "water" || id.startsWith("water")) return LayerKind.Water
        if (sourceLayer == "earth" ||
            sourceLayer == "landcover" ||
            sourceLayer == "landuse" ||
            id.startsWith("earth") ||
            id.startsWith("landcover") ||
            id.startsWith("landuse") ||
            id.contains("glacier")
        ) {
            return LayerKind.EarthLand
        }
        if (sourceLayer == "roads" ||
            sourceLayer == "buildings" ||
            sourceLayer == "boundaries" ||
            id.startsWith("roads") ||
            id.startsWith("buildings") ||
            id.startsWith("boundaries")
        ) {
            return LayerKind.Roads
        }
        return LayerKind.Other
    }

    private fun sourceSpec(
        pmFile: File,
        worldBounds: Boolean,
        job: FfiPmtilesJob? = null,
    ): JSONObject {
        val spec = JSONObject()
        spec.put("type", "vector")
        spec.put("url", "pmtiles://file://${pmFile.absolutePath}")
        spec.put("attribution", "© OpenStreetMap © Protomaps")
        val maxz = readPmtilesMaxZoom(pmFile) ?: if (worldBounds) WORLD_OVERVIEW_MAX_ZOOM else 15
        spec.put("maxzoom", maxz)
        spec.put("minzoom", 0)
        if (worldBounds) {
            spec.put(
                "bounds",
                JSONArray().put(-180.0).put(-85.0511287).put(180.0).put(85.0511287),
            )
        } else if (job != null &&
            job.minLon != null &&
            job.minLat != null &&
            job.maxLon != null &&
            job.maxLat != null
        ) {
            spec.put(
                "bounds",
                JSONArray()
                    .put(job.minLon!!)
                    .put(job.minLat!!)
                    .put(job.maxLon!!)
                    .put(job.maxLat!!),
            )
        }
        return spec
    }

    private fun appendClonedLayers(
        dest: JSONArray,
        template: JSONArray,
        sourceId: String,
        minZoom: Double?,
        maxZoom: Double?,
        keepOriginalIds: Boolean = false,
    ) {
        for (i in 0 until template.length()) {
            val layer = JSONObject(template.getJSONObject(i).toString())
            if (layer.optString("source") == "protomaps") {
                layer.put("source", sourceId)
            }
            val oldId = layer.optString("id")
            if (oldId.isNotEmpty() && !keepOriginalIds) {
                layer.put("id", "${oldId}__$sourceId")
            }
            if (minZoom != null) {
                val existing = if (layer.has("minzoom")) layer.getDouble("minzoom") else 0.0
                layer.put("minzoom", max(existing, minZoom))
            }
            if (maxZoom != null) {
                val existing = if (layer.has("maxzoom")) layer.getDouble("maxzoom") else 24.0
                layer.put("maxzoom", min(existing, maxZoom))
            }
            dest.put(layer)
        }
    }

    private fun writeStyleAtomically(
        outStyle: File,
        json: JSONObject,
    ) {
        val tmp = File(outStyle.parentFile, "${outStyle.name}.tmp")
        tmp.writeText(json.toString())
        if (!tmp.renameTo(outStyle)) {
            tmp.copyTo(outStyle, overwrite = true)
            tmp.delete()
        }
    }

    internal fun mountedStyleLeafName(
        mounted: MountedSources,
        withDem: Boolean,
    ): String {
        val raw = mounted.key() + if (withDem) "|dem" else ""
        val stem =
            raw
                .replace(Regex("[^A-Za-z0-9._+-]"), "_")
                .take(80)
        return "style.native.v1.$stem.json"
    }

    /**
     * Leaf name for the rewritten offline style JSON. Must differ per PMTiles
     * archive so [MainActivity.applyResolvedStyle] does not treat a second
     * region's style as `sameUri` and skip [org.maplibre.android.maps.MapLibreMap.setStyle].
     */
    internal fun offlineStyleLeafName(
        pmtilesAbsolutePath: String,
        withDem: Boolean,
    ): String {
        val stem =
            File(pmtilesAbsolutePath)
                .nameWithoutExtension
                .ifBlank { "basemap" }
                .replace(Regex("[^A-Za-z0-9._-]"), "_")
        val demTag = if (withDem) ".dem" else ""
        return "style.local.v3.$stem$demTag.json"
    }

    /**
     * PMTiles v3 header: magic at 0, maxzoom uint8 at offset 101.
     * Returns null if the file is too short or not a PMTiles archive.
     */
    internal fun readPmtilesMaxZoom(pmFile: File): Int? {
        if (!pmFile.isFile || pmFile.length() < 127L) return null
        val header = ByteArray(127)
        pmFile.inputStream().use { input ->
            var off = 0
            while (off < header.size) {
                val n = input.read(header, off, header.size - off)
                if (n <= 0) return null
                off += n
            }
        }
        if (String(header, 0, 7, Charsets.US_ASCII) != "PMTiles") return null
        return header[101].toInt() and 0xFF
    }

    private fun copyAssetTree(
        context: Context,
        assetPath: String,
        destDir: File,
    ) {
        destDir.mkdirs()
        val children = context.assets.list(assetPath) ?: return
        for (name in children) {
            val childAsset = if (assetPath.isEmpty()) name else "$assetPath/$name"
            val childDest = File(destDir, name)
            val sub = context.assets.list(childAsset)
            if (sub != null && sub.isNotEmpty()) {
                copyAssetTree(context, childAsset, childDest)
            } else {
                context.assets.open(childAsset).use { input ->
                    childDest.parentFile?.mkdirs()
                    FileOutputStream(childDest).use { output ->
                        input.copyTo(output)
                    }
                }
            }
        }
    }
}
