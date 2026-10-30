package no.navi.app

import android.content.Context
import android.content.res.Resources
import uniffi.navi.TravelProfile
import uniffi.navi.corridorSkeletonIsReady
import uniffi.navi.corridorSkeletonProgressSnapshot
import java.io.File
import java.util.concurrent.atomic.AtomicReference

/**
 * Sticky readiness under the place-search / route field: download → place index →
 * corridor skeletons. A region is routing-ready only when packs, place index and
 * the car corridor skeleton (truck when distinct) are present and fresh.
 *
 * Profiles that need skeletons for Stage B densify: Car / CarElectric /
 * Motorcycle (car skeleton), Truck / TruckElectric / MobileHome (truck skeleton
 * when the pack has distinct truck graphs; otherwise car). Bicycle / Foot use
 * Stage B only when a profile-specific skeleton exists; the idle builder only
 * auto-builds car + truck.
 */
object RegionReadiness {
    enum class Phase {
        Missing,
        Downloading,
        Indexing,
        BuildingRoutes,
        Ready,
    }

    data class RegionLine(
        val regionId: String,
        val displayName: String,
        val phase: Phase,
        val pct: Int?,
        val detail: String,
    )

    data class Snapshot(
        val lines: List<RegionLine>,
        /** Multi-line sticky text for the search panel (empty when nothing to show). */
        val panelText: String,
        val allReady: Boolean,
        val blocksRouting: Boolean,
    )

    private val sticky = AtomicReference(Snapshot(emptyList(), "", true, false))

    fun current(): Snapshot = sticky.get()

    fun skeletonArtifactsPresent(
        packDir: File,
        stem: String,
    ): Boolean {
        val trimmed = stem.trim()
        if (trimmed.isEmpty()) return false
        return File(packDir, "$trimmed.navi-corridor-skeleton.bin").isFile ||
            File(packDir, "$trimmed.navi-corridor-skeleton.json").isFile
    }

    fun carSkeletonReady(
        packDir: File,
        stem: String,
    ): Boolean {
        val trimmed = stem.trim()
        if (trimmed.isEmpty()) return false
        // Native freshness may reject a sidecar (fingerprint drift) while the
        // bin/meta still exist — treat artifacts as ready so long-trip state
        // reaches Indexed and planning is not stuck on BuildingRoutes.
        val nativeOk =
            runCatching {
                corridorSkeletonIsReady(packDir.absolutePath, trimmed, TravelProfile.CAR)
            }.getOrDefault(false)
        if (nativeOk) return true
        return skeletonArtifactsPresent(packDir, trimmed)
    }

    fun regionFullyReady(region: InstalledMaps.Region): Boolean {
        val indexOk =
            region.placeIndex == InstalledMaps.PlaceIndexState.INTACT ||
                region.placeIndex == InstalledMaps.PlaceIndexState.LEGACY_INTACT
        if (!indexOk) return false
        return carSkeletonReady(region.packDir, region.stem)
    }

    /**
     * Recompute sticky panel text for [regionIds] (long-trip corridor or the
     * active download / installed set). Pass null to use InstalledMaps + active
     * download path.
     */
    fun refresh(
        context: Context,
        regionIds: List<String>? = null,
    ): Snapshot {
        val resources = context.resources
        val snap = InstalledMaps.current()
        val activePath = RegionDownloadBackground.activeRegionPath().trim().trim('/')
        val ids =
            when {
                !regionIds.isNullOrEmpty() -> regionIds.map { PackRegionAvailability.normalize(it) }
                LongTripCoordinator.currentPlan() != null ->
                    LongTripCoordinator.currentPlan()!!.regionsInOrder.map {
                        PackRegionAvailability.normalize(it)
                    }
                activePath.isNotEmpty() -> listOf(PackRegionAvailability.normalize(activePath))
                snap != null ->
                    snap.regions.keys
                        .toList()
                        .sorted()
                else -> emptyList()
            }.distinct()
                .filter { it.isNotEmpty() }
                // World overview / DEM jobs are not Stage B corridor regions.
                .filter {
                    val leaf = it.substringAfterLast('/').lowercase()
                    leaf != "world_overview" && !leaf.endsWith("_dem") && !leaf.contains("overview")
                }

        val skelProg = runCatching { corridorSkeletonProgressSnapshot() }.getOrNull()
        val downloadLine = RegionDownloadBackground.uiLine()
        val placeLine =
            RegionDownloadBackground.placeIndexProgressLine().ifBlank {
                PlaceIndexBackground.statusLine()
            }
        val lines = mutableListOf<RegionLine>()
        for (id in ids) {
            lines.add(
                lineFor(
                    resources = resources,
                    regionId = id,
                    installed =
                        snap
                            ?.regions
                            ?.entries
                            ?.firstOrNull {
                                PackRegionAvailability.regionIdsMatchForCatalog(it.key, id)
                            }?.value,
                    longTripState =
                        LongTripCoordinator
                            .currentPlan()
                            ?.states
                            ?.entries
                            ?.firstOrNull {
                                PackRegionAvailability.regionIdsMatchForCatalog(it.key, id)
                            }?.value,
                    activePath = activePath,
                    downloadLine = downloadLine,
                    placeLine = placeLine,
                    skelProg = skelProg,
                ),
            )
        }
        val allReady = lines.isNotEmpty() && lines.all { it.phase == Phase.Ready }
        val blocks = lines.any { it.phase != Phase.Ready && it.phase != Phase.Missing }
        val panel =
            if (lines.isEmpty()) {
                ""
            } else {
                buildString {
                    for (line in lines) {
                        if (isNotEmpty()) append('\n')
                        append(line.detail)
                    }
                    if (blocks && !allReady) {
                        append('\n')
                        append(resources.getString(R.string.region_readiness_routing_blocked))
                    }
                }
            }
        val out = Snapshot(lines, panel, allReady, blocks && !allReady)
        sticky.set(out)
        return out
    }

    private fun lineFor(
        resources: Resources,
        regionId: String,
        installed: InstalledMaps.Region?,
        longTripState: LongTripCoordinator.State?,
        activePath: String,
        downloadLine: String,
        placeLine: String,
        skelProg: uniffi.navi.FfiCorridorSkeletonProgress?,
    ): RegionLine {
        val name =
            RegionProgressMessages.regionName(regionId).ifBlank {
                regionId.substringAfterLast('/').ifBlank { regionId }
            }
        val isActive =
            activePath.isNotEmpty() &&
                PackRegionAvailability.regionIdsMatchForCatalog(activePath, regionId)
        val downloading =
            isActive &&
                RegionDownloadBackground.isRunning() &&
                !placeLine.contains("Place index", ignoreCase = true) &&
                !downloadLine.contains("Place index", ignoreCase = true)
        val indexing =
            (isActive && placeLine.contains("Place index", ignoreCase = true)) ||
                longTripState == LongTripCoordinator.State.Indexing ||
                (
                    PlaceIndexBackground.isRunning() &&
                        PackRegionAvailability.regionIdsMatchForCatalog(
                            PlaceIndexBackground.activeRegionId(),
                            regionId,
                        )
                )
        val indexOk =
            installed != null &&
                (
                    installed.placeIndex == InstalledMaps.PlaceIndexState.INTACT ||
                        installed.placeIndex == InstalledMaps.PlaceIndexState.LEGACY_INTACT
                )
        val skelOk =
            installed != null &&
                (
                    carSkeletonReady(installed.packDir, installed.stem) ||
                        skeletonArtifactsPresent(installed.packDir, installed.stem)
                )
        // Truck (or re-validate) may still be queued after the car skeleton is
        // on disk — do not flap the panel back to "building routes".
        val skelBuilding =
            !skelOk &&
                IdlePackJobs.skeletonPending() &&
                (
                    skelProg?.stem?.let {
                        it.isNotBlank() &&
                            (
                                it == installed?.stem ||
                                    PackRegionAvailability.regionIdsMatchForCatalog(
                                        PackRegionAvailability.normalize(
                                            it.removeSuffix("-latest").replace('_', '-'),
                                        ),
                                        regionId,
                                    )
                            )
                    } == true ||
                        longTripState == LongTripCoordinator.State.BuildingRoutes ||
                        (indexOk && !skelOk)
                )

        val (phase, pct, detail) =
            when {
                skelOk && indexOk ->
                    Triple(
                        Phase.Ready,
                        100,
                        resources.getString(R.string.region_readiness_ready, name),
                    )
                skelBuilding ||
                    (indexOk && !skelOk) ||
                    (!skelOk && longTripState == LongTripCoordinator.State.BuildingRoutes) -> {
                    val p =
                        when {
                            skelProg != null && skelProg.running && skelProg.pct.toInt() > 0 ->
                                skelProg.pct.toInt()
                            else -> null
                        }
                    val body =
                        if (p != null) {
                            resources.getString(R.string.region_readiness_building_routes_pct, name, p)
                        } else {
                            resources.getString(R.string.region_readiness_building_routes, name)
                        }
                    Triple(Phase.BuildingRoutes, p, body)
                }
                indexing || longTripState == LongTripCoordinator.State.Indexing -> {
                    val p = parsePct(placeLine) ?: parsePct(downloadLine)
                    val body =
                        if (p != null) {
                            resources.getString(R.string.region_readiness_indexing_pct, name, p)
                        } else {
                            resources.getString(R.string.region_readiness_indexing, name)
                        }
                    Triple(Phase.Indexing, p, body)
                }
                downloading ||
                    longTripState == LongTripCoordinator.State.Downloading ||
                    longTripState == LongTripCoordinator.State.Queued -> {
                    val p = parsePct(downloadLine)
                    val body =
                        if (p != null) {
                            resources.getString(R.string.region_readiness_downloading_pct, name, p)
                        } else {
                            resources.getString(R.string.region_readiness_downloading, name)
                        }
                    Triple(Phase.Downloading, p, body)
                }
                installed != null && !indexOk ->
                    Triple(
                        Phase.Indexing,
                        null,
                        resources.getString(R.string.region_readiness_indexing, name),
                    )
                installed != null ->
                    Triple(
                        Phase.Ready,
                        100,
                        resources.getString(R.string.region_readiness_ready, name),
                    )
                else ->
                    Triple(
                        Phase.Missing,
                        null,
                        resources.getString(R.string.region_readiness_missing, name),
                    )
            }
        return RegionLine(regionId, name, phase, pct, detail)
    }

    private fun parsePct(line: String): Int? {
        if (line.isBlank()) return null
        val m = Regex("""(\d{1,3})\s*%""").find(line) ?: return null
        return m.groupValues[1].toIntOrNull()?.coerceIn(0, 100)
    }
}
