package no.navi.app

import android.content.Context
import android.util.Log
import org.json.JSONObject
import java.io.File
import java.util.concurrent.atomic.AtomicReference

/**
 * Inventory of installed map packs, extracts, tiles, ferry sidecars,
 * corridor skeletons, and place-index state. Router, place-index gate, and
 * download UI read this snapshot instead of ad-hoc directory probes.
 *
 * Refresh on download / delete / SD insert-remove — no process restart.
 */
object InstalledMaps {
    const val TAG = "InstalledMaps"
    const val STUB_PBF_MAX_BYTES = 32_768L

    enum class PbfKind {
        REAL,
        STUB,
        MISSING,
    }

    enum class PlaceIndexState {
        INTACT,
        LEGACY_INTACT,
        MISSING,
        QUARANTINED,
    }

    enum class PlaceIndexSource {
        PLACE_SOURCE,
        OWN_EXTRACT,
        NONE,
    }

    data class Region(
        val regionId: String,
        val stem: String,
        val volumeId: String,
        val packDir: File,
        val generation: String,
        val graphFormat: Int,
        val profilesLoadable: List<String>,
        val pbfKind: PbfKind,
        val pbfPath: File?,
        val ferrySidecarCar: Boolean,
        val ferrySidecarTruck: Boolean,
        val corridorSkeleton: Boolean,
        val tilesPresent: Boolean,
        val tilesRejected: Boolean,
        /** Why [tilesRejected] is set; empty when the archive is present or never downloaded. */
        val tilesRejectReason: String = "",
        val placeIndex: PlaceIndexState,
        val placeIndexRows: Long,
        val placeIndexSource: PlaceIndexSource = PlaceIndexSource.NONE,
        val placeIndexSourceSha: String = "",
    ) {
        /** Tiles that the active routing profile can actually load. */
        fun tilesLoadFor(profileKey: String): Boolean {
            val key = profileKey.lowercase()
            if (profilesLoadable.contains(key)) return true
            return key == "truck" && profilesLoadable.contains("car")
        }

        /** User-facing line when this region has no usable offline basemap. */
        fun noOfflineMapMessage(): String? {
            if (tilesPresent) return null
            if (!tilesRejected && tilesRejectReason.isBlank()) return null
            val why = tilesRejectReason.ifBlank { "offline archive rejected" }
            return "This region has no offline map ($why)"
        }
    }

    data class Snapshot(
        val generatedAtMs: Long,
        val regions: Map<String, Region>,
        val missingPlaceIndex: List<MissingIndexBuild>,
        val partialFetchIds: Set<String> = emptySet(),
        /** Why place search is unavailable for every region, or null when the index file is usable. */
        val placeIndexProblem: String? = null,
        /** Where the place index lives (`place_index vol=... path=...`), when known. */
        val placeIndexLocation: String? = null,
    )

    data class MissingIndexBuild(
        val regionId: String,
        val pbfPath: File?,
        val pbfKind: PbfKind,
        val note: String,
    )

    private val snapshot = AtomicReference<Snapshot?>(null)
    private val snapshotRoot = AtomicReference<String?>(null)
    private val lastPlaceIndexDir = AtomicReference<File?>(null)

    fun placeIndexDir(): File? = lastPlaceIndexDir.get()

    fun snapshotIsFor(dataDir: File): Boolean = snapshotRoot.get() == dataDir.absolutePath

    fun current(): Snapshot? = snapshot.get()

    fun clearForTests() {
        snapshot.set(null)
        snapshotRoot.set(null)
        lastPlaceIndexDir.set(null)
        IdlePackJobs.resetForTests()
    }

    fun region(
        id: String,
        dataDir: File? = null,
    ): Region? {
        val n = PackRegionAvailability.normalize(id)
        val snap = snapshot.get() ?: return null
        if (dataDir != null && snapshotRoot.get() != dataDir.absolutePath) return null
        snap.regions[n]?.let { return it }
        return snap.regions.values.firstOrNull {
            PackRegionAvailability.regionIdsMatchForCatalog(it.regionId, n)
        }
    }

    fun packReadyForProfile(
        geofabrikPath: String,
        profileKey: String,
    ): Boolean {
        val r = region(geofabrikPath) ?: return false
        return r.tilesLoadFor(profileKey)
    }

    /**
     * Tools/delete enablement from the last [refresh] snapshot. Does not
     * [listFiles] — Compose must not call [DownloadedRegionDelete.hasAnyInstall].
     */
    fun hasInstallForUi(
        geofabrikPath: String,
        dataDir: File,
    ): Boolean {
        val n = PackRegionAvailability.normalize(geofabrikPath)
        if (n.isEmpty()) return false
        if (!snapshotIsFor(dataDir)) return false
        if (region(n, dataDir) != null) return true
        val snap = snapshot.get() ?: return false
        return snap.partialFetchIds.any {
            PackRegionAvailability.regionIdsMatchForCatalog(it, n)
        }
    }

    fun refresh(context: Context) {
        PlaceIndexStorage.ensureOnPackVolume(context)
        val internal = NaviAppData.resolve(context)
        val placeIndexDir = PlaceIndexStorage.indexDir(context)
        val extras = mutableListOf<Pair<String, File>>()
        extras.add(NaviStorageVolumes.INTERNAL_ID to File(internal, LongTripPackStorage.PACKS_SUBDIR))
        for (vol in NaviStorageVolumes.list(context)) {
            val app = vol.appFilesDir ?: continue
            if (!vol.mounted) continue
            extras.add(vol.id to File(app, LongTripPackStorage.PACKS_SUBDIR))
        }
        refreshFromDirs(internal, extras, placeIndexDir, PlaceIndexStorage.locationSummary(context))
    }

    fun refreshFromDirs(
        internalDataDir: File,
        packRoots: List<Pair<String, File>>,
        placeIndexDir: File? = internalDataDir,
        placeIndexLocationLine: String? = null,
    ) {
        val byId = linkedMapOf<String, Region>()
        val scanned = LinkedHashSet<String>()
        val probes = HashMap<String, PlaceIndexIntact.Probe>()
        val partial = LinkedHashSet<String>()

        fun consider(
            volumeId: String,
            dir: File,
        ) {
            if (!dir.isDirectory) return
            val key = dir.absolutePath
            if (!scanned.add(key)) return
            scanDir(internalDataDir, placeIndexDir, volumeId, dir, byId, probes, partial)
        }
        consider(NaviStorageVolumes.INTERNAL_ID, internalDataDir)
        for ((id, dir) in packRoots) {
            consider(id, dir)
        }
        addWorldOverview(internalDataDir, byId)
        val indexUnavailable =
            placeIndexDir == null || placeIndexLocationLine?.contains("UNAVAILABLE") == true
        val problem =
            when {
                indexUnavailable || placeIndexDir == null -> "pack volume unavailable"
                else -> PlaceIndexIntact.fileProblem(placeIndexDir)
            }
        val missing =
            byId.values.mapNotNull { r ->
                if (r.regionId == BasemapStyleResolver.WORLD_OVERVIEW_REGION_KEY) {
                    return@mapNotNull null
                }
                if (r.placeIndex == PlaceIndexState.INTACT || r.placeIndex == PlaceIndexState.LEGACY_INTACT) {
                    return@mapNotNull null
                }
                if (PackRegionAvailability.isPublishedPackParent(r.regionId)) {
                    return@mapNotNull null
                }
                if (indexUnavailable) {
                    return@mapNotNull MissingIndexBuild(
                        r.regionId,
                        r.pbfPath,
                        r.pbfKind,
                        "search unavailable: place index on pack volume unavailable (not building elsewhere)",
                    )
                }
                val searchNote =
                    if (problem != null) {
                        "search unavailable: place index $problem; "
                    } else {
                        "search unavailable: no rows for this region; "
                    }
                val leafPbf =
                    PackRegionAvailability.resolvePlaceIndexPbf(r.packDir, r.regionId)
                        ?: PackRegionAvailability.resolvePlaceIndexPbf(internalDataDir, r.regionId)
                val pbf = leafPbf ?: r.pbfPath
                val stamp = File(r.packDir, "${r.stem}.navi-server-install.json")
                val note =
                    when {
                        leafPbf == null && stamp.isFile ->
                            "no own extract; idle will try the pack-server place-source file, " +
                                "otherwise " + OfflineIndexGate.CANNOT_INDEX_YET
                        leafPbf == null ->
                            OfflineIndexGate.CANNOT_INDEX_YET +
                                " (need ${PackRegionAvailability.localStem(r.regionId)}.osm.pbf)"
                        r.pbfKind == PbfKind.STUB && leafPbf.length() < RegionDownloadBackground.MIN_PBF_BYTES ->
                            OfflineIndexGate.CANNOT_INDEX_YET + "; stub PBF only"
                        else -> {
                            val mb = leafPbf.length() / 1_000_000L
                            val hours =
                                when {
                                    mb >= 400 -> "about 1-4 hours on-device"
                                    mb >= 100 -> "about 30-90 minutes on-device"
                                    else -> "under 30 minutes on-device"
                                }
                            "would index ${leafPbf.name} (${leafPbf.length()} bytes); not started; $hours"
                        }
                    }
                MissingIndexBuild(r.regionId, pbf, r.pbfKind, searchNote + note)
            }
        snapshot.set(
            Snapshot(
                generatedAtMs = System.currentTimeMillis(),
                regions = byId,
                missingPlaceIndex = missing,
                partialFetchIds = partial,
                placeIndexProblem = problem,
                placeIndexLocation = placeIndexLocationLine,
            ),
        )
        snapshotRoot.set(internalDataDir.absolutePath)
        lastPlaceIndexDir.set(placeIndexDir)
        IdlePackJobs.onInstalledMapsChanged()
        val intactIds =
            byId.values
                .filter { it.placeIndex == PlaceIndexState.INTACT || it.placeIndex == PlaceIndexState.LEGACY_INTACT }
                .map { it.regionId }
                .toSet()
        runCatching { PlaceIndexReady.syncStampFromIndex(internalDataDir, intactIds) }
        runCatching {
            File(internalDataDir, "installed-maps-snapshot.txt").writeText(summaryText())
        }
        Log.i(
            TAG,
            "snapshot regions=${byId.size} missing_index=${missing.size} " +
                "ids=${byId.keys.sorted()} ${placeIndexLocationLine ?: ""}",
        )
    }

    fun summaryText(): String {
        val snap = snapshot.get() ?: return "InstalledMaps: no snapshot"
        return buildString {
            appendLine("InstalledMaps regions=${snap.regions.size} missing_index=${snap.missingPlaceIndex.size}")
            snap.placeIndexProblem?.let {
                appendLine("place search unavailable for all regions: place index $it")
            }
            for (r in snap.regions.values.sortedBy { it.regionId }) {
                appendLine(
                    "${r.regionId} vol=${r.volumeId} gen=${r.generation.ifBlank { "-" }} " +
                        "fmt=${r.graphFormat} profiles=${r.profilesLoadable.joinToString(",")} " +
                        "pbf=${r.pbfKind} ferry_car=${r.ferrySidecarCar} ferry_truck=${r.ferrySidecarTruck} " +
                        "corridor_skel=${r.corridorSkeleton} " +
                        "tiles=${r.tilesPresent} rejected=${r.tilesRejected}" +
                        (if (r.tilesRejected) {
                            " no_offline_map=true reason=${r.tilesRejectReason.ifBlank { "offline archive rejected" }}"
                        } else {
                            ""
                        }) +
                        " " +
                        "index=${r.placeIndex} index_source=${r.placeIndexSource.name.lowercase().replace('_', '-')} " +
                        "rows=${r.placeIndexRows}",
                )
            }
            if (snap.missingPlaceIndex.isNotEmpty()) {
                appendLine("would_build_place_index:")
                for (m in snap.missingPlaceIndex) {
                    appendLine("  ${m.regionId} pbf=${m.pbfPath?.absolutePath ?: "-"} ${m.note}")
                }
            }
            snap.placeIndexLocation?.let { appendLine(it) }
        }
    }

    private fun addWorldOverview(
        tilesDataDir: File,
        into: MutableMap<String, Region>,
    ) {
        val key = BasemapStyleResolver.WORLD_OVERVIEW_REGION_KEY
        if (into.containsKey(key)) return
        val tilesFile = File(tilesDataDir, "pmtiles/$key.pmtiles")
        val rejectedFile = File(tilesDataDir, "pmtiles/$key.pmtiles.rejected")
        val tiles = tilesStatus(tilesFile, rejectedFile, key)
        if (!tiles.present && !tiles.rejected && !tilesFile.isFile) return
        into[key] =
            Region(
                regionId = key,
                stem = key,
                volumeId = NaviStorageVolumes.INTERNAL_ID,
                packDir = File(tilesDataDir, "pmtiles"),
                generation = "",
                graphFormat = 0,
                profilesLoadable = emptyList(),
                pbfKind = PbfKind.MISSING,
                pbfPath = null,
                ferrySidecarCar = false,
                ferrySidecarTruck = false,
                corridorSkeleton = false,
                tilesPresent = tiles.present,
                tilesRejected = tiles.rejected,
                tilesRejectReason = tiles.reason,
                placeIndex = PlaceIndexState.MISSING,
                placeIndexRows = 0,
            )
    }

    private fun scanDir(
        tilesDataDir: File,
        placeIndexDir: File?,
        volumeId: String,
        dir: File,
        into: MutableMap<String, Region>,
        probes: MutableMap<String, PlaceIndexIntact.Probe>,
        partial: MutableSet<String>,
    ) {
        val files = dir.listFiles() ?: return

        fun probeFor(nid: String): PlaceIndexIntact.Probe =
            probes.getOrPut(nid) {
                if (placeIndexDir == null) {
                    PlaceIndexIntact.unavailable("pack_volume_unavailable")
                } else {
                    PlaceIndexIntact.probe(placeIndexDir, nid)
                }
            }
        for (f in files) {
            val name = f.name
            if (!name.startsWith(".pack-fetch-")) continue
            val stem = name.removePrefix(".pack-fetch-").substringBefore('.')
            if (stem.isEmpty()) continue
            val rid = regionIdForStem(dir, stem) ?: continue
            partial.add(PackRegionAvailability.normalize(rid))
        }
        val manifests = files.filter { it.isFile && it.name.endsWith(".navi-manifest.json") }
        for (man in manifests) {
            val stem = man.name.removeSuffix(".navi-manifest.json")
            val regionId = regionIdForStem(dir, stem) ?: continue
            val nid = PackRegionAvailability.normalize(regionId)
            val parsed = parseManifest(man)
            val profiles = loadableProfilesFromNames(files, stem)
            val pbf = File(dir, "$stem.osm.pbf")
            val pbfKind =
                when {
                    !pbf.isFile -> PbfKind.MISSING
                    pbf.length() < RegionDownloadBackground.MIN_PBF_BYTES -> PbfKind.STUB
                    else -> PbfKind.REAL
                }
            val install = File(dir, "$stem.navi-server-install.json")
            val generation =
                runCatching {
                    if (install.isFile) JSONObject(install.readText()).optString("generation") else ""
                }.getOrDefault("")
            val pmKey = PackRegionAvailability.geofabrikPathToRegionKey(nid)
            val tilesFile = File(tilesDataDir, "pmtiles/$pmKey.pmtiles")
            val rejectedFile = File(tilesDataDir, "pmtiles/$pmKey.pmtiles.rejected")
            val tiles = tilesStatus(tilesFile, rejectedFile, pmKey)
            val rejectReason = tiles.reason
            val probe = probeFor(nid)
            val q = placeIndexDir != null && File(placeIndexDir, "place_index.db.quarantine").isFile
            val indexState =
                when {
                    probe.intact && probe.legacy -> PlaceIndexState.LEGACY_INTACT
                    probe.intact -> PlaceIndexState.INTACT
                    q && probe.reason == "open_failed" -> PlaceIndexState.QUARANTINED
                    else -> PlaceIndexState.MISSING
                }
            val region =
                Region(
                    regionId = nid,
                    stem = stem,
                    volumeId = volumeId,
                    packDir = dir,
                    generation = generation,
                    graphFormat = parsed,
                    profilesLoadable = profiles,
                    pbfKind = pbfKind,
                    pbfPath = pbf.takeIf { it.isFile },
                    ferrySidecarCar = File(dir, "$stem.navi-ferry-overlay-car.rkyv").isFile,
                    ferrySidecarTruck = File(dir, "$stem.navi-ferry-overlay-truck.rkyv").isFile,
                    corridorSkeleton =
                        File(dir, "$stem.navi-corridor-skeleton.bin").isFile ||
                            File(dir, "$stem.navi-corridor-skeleton.json").isFile,
                    tilesPresent = tiles.present,
                    tilesRejected = tiles.rejected,
                    tilesRejectReason = rejectReason,
                    placeIndex = indexState,
                    placeIndexRows = probe.rowCount,
                    placeIndexSource = indexSourceOf(indexState, probe),
                    placeIndexSourceSha = probe.sourceSha256,
                )
            val prev = into[nid]
            if (prev == null || prefer(region, prev)) {
                into[nid] = region
            }
        }
        // PBFs without a manifest still matter for place-index / resolvePlanPbf.
        for (f in files) {
            if (!f.isFile || !f.name.endsWith(".osm.pbf")) continue
            val stem = f.name.removeSuffix(".osm.pbf")
            val regionId = regionIdForStem(dir, stem) ?: continue
            val nid = PackRegionAvailability.normalize(regionId)
            if (into.containsKey(nid)) continue
            val probe = probeFor(nid)
            val pbfKind =
                if (f.length() < RegionDownloadBackground.MIN_PBF_BYTES) PbfKind.STUB else PbfKind.REAL
            val pbfTiles =
                tilesStatus(
                    File(tilesDataDir, "pmtiles/${PackRegionAvailability.geofabrikPathToRegionKey(nid)}.pmtiles"),
                    File(tilesDataDir, "pmtiles/${PackRegionAvailability.geofabrikPathToRegionKey(nid)}.pmtiles.rejected"),
                    PackRegionAvailability.geofabrikPathToRegionKey(nid),
                )
            into[nid] =
                Region(
                    regionId = nid,
                    stem = stem,
                    volumeId = volumeId,
                    packDir = dir,
                    generation = "",
                    graphFormat = 0,
                    profilesLoadable = emptyList(),
                    pbfKind = pbfKind,
                    pbfPath = f,
                    ferrySidecarCar = File(dir, "$stem.navi-ferry-overlay-car.rkyv").isFile,
                    ferrySidecarTruck = File(dir, "$stem.navi-ferry-overlay-truck.rkyv").isFile,
                    corridorSkeleton =
                        File(dir, "$stem.navi-corridor-skeleton.bin").isFile ||
                            File(dir, "$stem.navi-corridor-skeleton.json").isFile,
                    tilesPresent = pbfTiles.present,
                    tilesRejected = pbfTiles.rejected,
                    tilesRejectReason = pbfTiles.reason,
                    placeIndex =
                        if (probe.intact) {
                            if (probe.legacy) PlaceIndexState.LEGACY_INTACT else PlaceIndexState.INTACT
                        } else {
                            PlaceIndexState.MISSING
                        },
                    placeIndexRows = probe.rowCount,
                    placeIndexSource = indexSourceOf(
                        if (probe.intact) {
                            if (probe.legacy) PlaceIndexState.LEGACY_INTACT else PlaceIndexState.INTACT
                        } else {
                            PlaceIndexState.MISSING
                        },
                        probe,
                    ),
                    placeIndexSourceSha = probe.sourceSha256,
                )
        }
    }

    private data class TilesStatus(
        val present: Boolean,
        val rejected: Boolean,
        val reason: String,
    )

    private fun tilesStatus(
        tilesFile: File,
        rejectedFile: File,
        regionKey: String = "",
    ): TilesStatus {
        if (rejectedFile.isFile) {
            return TilesStatus(false, true, readRejectReason(rejectedFile))
        }
        if (!tilesFile.isFile) {
            val sidecar = File(tilesFile.absolutePath + ".reason")
            if (sidecar.isFile) {
                return TilesStatus(false, true, sidecar.readText().trim().ifBlank { "missing archive" })
            }
            return TilesStatus(false, false, "")
        }
        val why = PmtilesArchiveGate.rejectionReason(tilesFile, regionKey)
        if (why != null) {
            return TilesStatus(false, true, why)
        }
        return TilesStatus(true, false, "")
    }

    private fun readRejectReason(rejectedFile: File): String {
        if (!rejectedFile.isFile) return ""
        val reason = File(rejectedFile.absolutePath + ".reason")
        return if (reason.isFile) {
            reason.readText().trim()
        } else {
            "offline archive rejected"
        }
    }

    private fun indexSourceOf(
        state: PlaceIndexState,
        probe: PlaceIndexIntact.Probe,
    ): PlaceIndexSource {
        if (state == PlaceIndexState.MISSING || state == PlaceIndexState.QUARANTINED) {
            return PlaceIndexSource.NONE
        }
        if (probe.indexSource == "place-source") return PlaceIndexSource.PLACE_SOURCE
        return PlaceIndexSource.OWN_EXTRACT
    }

    private fun prefer(
        a: Region,
        b: Region,
    ): Boolean {
        val aTiles = a.profilesLoadable.isNotEmpty()
        val bTiles = b.profilesLoadable.isNotEmpty()
        if (aTiles != bTiles) return aTiles
        val aReal = a.pbfKind == PbfKind.REAL
        val bReal = b.pbfKind == PbfKind.REAL
        if (aReal != bReal) return aReal
        return a.volumeId != NaviStorageVolumes.INTERNAL_ID && b.volumeId == NaviStorageVolumes.INTERNAL_ID
    }

    private fun parseManifest(man: File): Int =
        runCatching {
            JSONObject(man.readText()).optInt("graph_format_version", 0)
        }.getOrDefault(0)

    private fun loadableProfilesFromNames(
        files: Array<File>,
        stem: String,
    ): List<String> {
        val keys = listOf("car", "foot", "truck", "bicycle")
        val names = files.map { it.name }
        return keys.filter { key ->
            val prefix = "$stem.navi-graph-$key."
            names.any { n ->
                n == "$stem.navi-graph-$key.rkyv" ||
                    (n.startsWith(prefix) && n.endsWith(".rkyv"))
            }
        }
    }

    internal fun regionIdForStem(
        dir: File,
        stem: String,
    ): String? {
        val install = File(dir, "$stem.navi-server-install.json")
        if (install.isFile) {
            val id =
                runCatching { JSONObject(install.readText()).optString("region_id") }.getOrNull()
            if (!id.isNullOrBlank()) return PackRegionAvailability.normalize(id)
        }
        val fromName = RegionCoverage.geofabrikPathForPbfName("$stem.osm.pbf")
        if (!fromName.isNullOrBlank()) return PackRegionAvailability.normalize(fromName)
        return stemFallback(stem)
    }

    internal fun stemFallback(stem: String): String? {
        val leaf = stem.removeSuffix("-latest").lowercase()
        return when (leaf) {
            "denmark" -> "europe/denmark"
            "hamburg" -> "europe/germany/hamburg"
            "niedersachsen" -> "europe/germany/niedersachsen"
            "schleswig-holstein" -> "europe/germany/schleswig-holstein"
            "mecklenburg-vorpommern" -> "europe/germany/mecklenburg-vorpommern"
            "ostlandet" -> "europe/norway/ostlandet"
            "vestlandet" -> "europe/norway/vestlandet"
            "sorlandet" -> "europe/norway/sorlandet"
            "sweden" -> "europe/sweden"
            "halland" -> "europe/sweden/halland"
            "skane" -> "europe/sweden/skane"
            "vastra_gotaland", "vastra-gotaland" -> "europe/sweden/vastra_gotaland"
            else -> null
        }
    }
}
