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
        val placeIndex: PlaceIndexState,
        val placeIndexRows: Long,
    ) {
        /** Tiles that the active routing profile can actually load. */
        fun tilesLoadFor(profileKey: String): Boolean {
            val key = profileKey.lowercase()
            if (profilesLoadable.contains(key)) return true
            return key == "truck" && profilesLoadable.contains("car")
        }
    }

    data class Snapshot(
        val generatedAtMs: Long,
        val regions: Map<String, Region>,
        val missingPlaceIndex: List<MissingIndexBuild>,
        val partialFetchIds: Set<String> = emptySet(),
    )

    data class MissingIndexBuild(
        val regionId: String,
        val pbfPath: File?,
        val pbfKind: PbfKind,
        val note: String,
    )

    private val snapshot = AtomicReference<Snapshot?>(null)
    private val snapshotRoot = AtomicReference<String?>(null)

    fun snapshotIsFor(dataDir: File): Boolean = snapshotRoot.get() == dataDir.absolutePath

    fun current(): Snapshot? = snapshot.get()

    fun clearForTests() {
        snapshot.set(null)
        snapshotRoot.set(null)
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
        val placeIndexDir =
            PlaceIndexStorage.indexDir(context) ?: internal
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
        placeIndexDir: File = internalDataDir,
        placeIndexLocationLine: String? = null,
    ) {
        val byId = linkedMapOf<String, Region>()
        val scanned = LinkedHashSet<String>()
        val probes = HashMap<String, PlaceIndexIntact.Probe>()
        val partial = LinkedHashSet<String>()
        fun consider(volumeId: String, dir: File) {
            if (!dir.isDirectory) return
            val key = dir.absolutePath
            if (!scanned.add(key)) return
            scanDir(internalDataDir, placeIndexDir, volumeId, dir, byId, probes, partial)
        }
        consider(NaviStorageVolumes.INTERNAL_ID, internalDataDir)
        for ((id, dir) in packRoots) {
            consider(id, dir)
        }
        val indexUnavailable =
            placeIndexLocationLine?.contains("UNAVAILABLE") == true
        val missing = byId.values.mapNotNull { r ->
            if (r.placeIndex == PlaceIndexState.INTACT || r.placeIndex == PlaceIndexState.LEGACY_INTACT) {
                return@mapNotNull null
            }
            if (indexUnavailable) {
                return@mapNotNull MissingIndexBuild(
                    r.regionId,
                    r.pbfPath,
                    r.pbfKind,
                    "place index unavailable on pack volume (not building elsewhere)",
                )
            }
            val leafPbf =
                PackRegionAvailability.resolvePlaceIndexPbf(r.packDir, r.regionId)
                    ?: PackRegionAvailability.resolvePlaceIndexPbf(internalDataDir, r.regionId)
            val pbf = leafPbf ?: r.pbfPath
            val note =
                when {
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
            MissingIndexBuild(r.regionId, pbf, r.pbfKind, note)
        }
        snapshot.set(
            Snapshot(
                generatedAtMs = System.currentTimeMillis(),
                regions = byId,
                missingPlaceIndex = missing,
                partialFetchIds = partial,
            ),
        )
        snapshotRoot.set(internalDataDir.absolutePath)
        runCatching {
            val body =
                buildString {
                    appendLine(summaryText())
                    if (placeIndexLocationLine != null) {
                        appendLine(placeIndexLocationLine)
                    }
                }
            File(internalDataDir, "installed-maps-snapshot.txt").writeText(body)
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
            for (r in snap.regions.values.sortedBy { it.regionId }) {
                appendLine(
                    "${r.regionId} vol=${r.volumeId} gen=${r.generation.ifBlank { "-" }} " +
                        "fmt=${r.graphFormat} profiles=${r.profilesLoadable.joinToString(",")} " +
                        "pbf=${r.pbfKind} ferry_car=${r.ferrySidecarCar} ferry_truck=${r.ferrySidecarTruck} " +
                        "corridor_skel=${r.corridorSkeleton} " +
                        "tiles=${r.tilesPresent} rejected=${r.tilesRejected} " +
                        "index=${r.placeIndex} rows=${r.placeIndexRows}",
                )
            }
            if (snap.missingPlaceIndex.isNotEmpty()) {
                appendLine("would_build_place_index:")
                for (m in snap.missingPlaceIndex) {
                    appendLine("  ${m.regionId} pbf=${m.pbfPath?.absolutePath ?: "-"} ${m.note}")
                }
            }
        }
    }

    private fun scanDir(
        tilesDataDir: File,
        placeIndexDir: File,
        volumeId: String,
        dir: File,
        into: MutableMap<String, Region>,
        probes: MutableMap<String, PlaceIndexIntact.Probe>,
        partial: MutableSet<String>,
    ) {
        val files = dir.listFiles() ?: return
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
            val probe = probes.getOrPut(nid) { PlaceIndexIntact.probe(placeIndexDir, nid) }
            val q = File(placeIndexDir, "place_index.db.quarantine").isFile
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
                    corridorSkeleton = File(dir, "$stem.navi-corridor-skeleton.json").isFile,
                    tilesPresent = tilesFile.isFile,
                    tilesRejected = rejectedFile.isFile,
                    placeIndex = indexState,
                    placeIndexRows = probe.rowCount,
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
            val probe = probes.getOrPut(nid) { PlaceIndexIntact.probe(placeIndexDir, nid) }
            val pbfKind =
                if (f.length() < RegionDownloadBackground.MIN_PBF_BYTES) PbfKind.STUB else PbfKind.REAL
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
                    corridorSkeleton = File(dir, "$stem.navi-corridor-skeleton.json").isFile,
                    tilesPresent = File(tilesDataDir, "pmtiles/${PackRegionAvailability.geofabrikPathToRegionKey(nid)}.pmtiles").isFile,
                    tilesRejected = File(tilesDataDir, "pmtiles/${PackRegionAvailability.geofabrikPathToRegionKey(nid)}.pmtiles.rejected").isFile,
                    placeIndex =
                        if (probe.intact) {
                            if (probe.legacy) PlaceIndexState.LEGACY_INTACT else PlaceIndexState.INTACT
                        } else {
                            PlaceIndexState.MISSING
                        },
                    placeIndexRows = probe.rowCount,
                )
        }
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

    private fun parseManifest(man: File): Int {
        return runCatching {
            JSONObject(man.readText()).optInt("graph_format_version", 0)
        }.getOrDefault(0)
    }

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
