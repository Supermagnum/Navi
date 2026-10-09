package no.navi.app

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import org.json.JSONObject
import uniffi.navi.FfiPmtilesJob
import uniffi.navi.pmtilesListCovering
import uniffi.navi.pmtilesListJobs
import java.io.File
import java.io.FileOutputStream

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
        /** True when Liberty is mounted because the viewport extends past archives. */
        val onlineUnderlay: Boolean = false,
        /** Offline archives that intersect the viewport (empty for forced online). */
        val overlayArchives: List<FfiPmtilesJob> = emptyList(),
    )

    fun hasNetwork(context: Context): Boolean {
        val cm =
            context.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
                ?: return false
        val network = cm.activeNetwork ?: return false
        val caps = cm.getNetworkCapabilities(network) ?: return false
        // AAOS / emulator often reports Wi‑Fi without VALIDATED; INTERNET alone
        // is also missing on some secondary-user profiles. Treat any IP transport
        // as online enough to attempt Mapterhorn TileJSON.
        if (caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) ||
            caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
        ) {
            return true
        }
        return caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) ||
            caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) ||
            caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) ||
            caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN)
    }

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
            val uri =
                prepareOfflineComposite(context, listOf(forcedJob), demFor3d = null)
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

            val archives = if (intersecting.isNotEmpty()) intersecting else listOfNotNull(covering)
            val extendsBeyond =
                view != null &&
                    viewportExtendsBeyondArchives(view, archives)
            val network = hasNetwork(context)
            val mountOnline = shouldMountOnlineUnderlay(network, extendsBeyond)

            if (mountOnline) {
                val online =
                    fallbackOnline(
                        context,
                        dataDir,
                        prefer3d,
                        vulkanAvailable,
                        note =
                            if (archives.isNotEmpty()) {
                                "Online map fills the view beyond offline archives"
                            } else {
                                null
                            },
                    )
                return online.copy(
                    onlineUnderlay = true,
                    overlayArchives = archives,
                    coveringJob = covering,
                )
            }

            if (archives.isNotEmpty()) {
                val primary = covering ?: archives.first()
                val localDem = MapterhornTerrain.localDemBesideBasemap(primary.localPath)
                val offlineFlags =
                    offlineCoveringFlags(
                        want3d = want3d,
                        localDemPresent = localDem != null,
                    )
                val uri =
                    prepareOfflineComposite(
                        context,
                        archives,
                        demFor3d = if (offlineFlags.offline3d) localDem else null,
                        // Several extracts, or a view larger than one bbox:
                        // loopback composite (MapLibre.setConnected keeps
                        // 127.0.0.1 alive in airplane mode). A single archive
                        // that covers the view is read with pmtiles://file://
                        // so the extract's own z/x/y tiles are used.
                        useLoopback = archives.size > 1 || extendsBeyond,
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
                    note = offlineFlags.note,
                    cameraPitch = offlineFlags.cameraPitch,
                    attachMapterhornTerrain = false,
                    demSourceUri =
                        if (offlineFlags.offline3d) {
                            MapterhornTerrain.ensureLocalDemTileJsonUrl(localDem!!)
                        } else {
                            null
                        },
                    overlayArchives = archives,
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
                LocalVectorPmtilesServer.bboxIntersects(
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
        return LocalVectorPmtilesServer.pointInBbox(lat, lon, minLat, minLon, maxLat, maxLon)
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
        val assetEpoch = "v22-named-building-overlay"
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
        patchProtomapsSource(json, tilesUrl, worldBounds)
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
     * Offline style that reads every intersecting archive through the local
     * vector tile server. Source bounds are the world so tiles that only partly
     * overlap a region bbox are still requested at low zoom.
     */
    fun prepareOfflineComposite(
        context: Context,
        archives: List<FfiPmtilesJob>,
        demFor3d: File? = null,
        useLoopback: Boolean = true,
    ): String? {
        val usable =
            archives.filter { job ->
                File(job.localPath).isFile &&
                    !isDemArchive(job.regionKey, job.localPath)
            }
        if (usable.isEmpty()) return null
        val primary = File(usable.first().localPath)
        val tilesUrl =
            if (useLoopback) {
                val serverArchives =
                    usable.map { job ->
                        LocalVectorPmtilesServer.Archive(
                            path = job.localPath,
                            minLat = job.minLat ?: -85.0,
                            minLon = job.minLon ?: -180.0,
                            maxLat = job.maxLat ?: 85.0,
                            maxLon = job.maxLon ?: 180.0,
                        )
                    }
                LocalVectorPmtilesServer.ensureServing(serverArchives)
            } else {
                null
            }
        // Airplane / no radio: keep pmtiles://file:// (MapLibre will not
        // fetch 127.0.0.1). World bounds so low-zoom ancestors still paint.
        return prepareOfflineStyle(
            context,
            primary.absolutePath,
            demFor3d,
            tilesUrl = tilesUrl,
            worldBounds = true,
        )
    }

    private fun patchProtomapsSource(
        json: JSONObject,
        tilesUrl: String?,
        worldBounds: Boolean,
    ) {
        val pm = json.getJSONObject("sources").getJSONObject("protomaps")
        if (!tilesUrl.isNullOrBlank()) {
            pm.remove("url")
            pm.put("tiles", org.json.JSONArray().put(tilesUrl))
            val maxz = pm.optInt("maxzoom", 15)
            pm.put("maxzoom", if (maxz in 1..15) maxz else 15)
        }
        if (worldBounds || !tilesUrl.isNullOrBlank()) {
            pm.put("minzoom", 0)
            // Do not clip to one extract bbox: a tile that lies only partly
            // inside the region must still be requested.
            pm.put(
                "bounds",
                org.json.JSONArray()
                    .put(-180.0)
                    .put(-85.0511287)
                    .put(180.0)
                    .put(85.0511287),
            )
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

    internal fun rewriteSourceWorldBounds(styleFile: File) {
        rewritePreparedStyle(styleFile, tilesUrl = null, worldBounds = true)
    }

    internal fun rewriteSourceToCompositeTiles(
        styleFile: File,
        tilesUrl: String,
    ) {
        rewritePreparedStyle(styleFile, tilesUrl = tilesUrl, worldBounds = true)
    }

    private fun rewritePreparedStyle(
        styleFile: File,
        tilesUrl: String?,
        worldBounds: Boolean,
    ) {
        if (!styleFile.isFile) return
        val text =
            runCatching { styleFile.readText() }.getOrNull()?.takeIf { it.isNotBlank() }
                ?: return
        val json =
            runCatching { JSONObject(text) }.getOrNull() ?: return
        patchProtomapsSource(json, tilesUrl, worldBounds)
        writeStyleAtomically(styleFile, json)
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
