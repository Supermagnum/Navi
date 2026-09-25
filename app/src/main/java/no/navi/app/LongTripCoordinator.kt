package no.navi.app

import android.content.Context
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import uniffi.navi.geofabrikLatestPbfUrl
import uniffi.navi.longTripOrderedRegionsJson
import java.io.File
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * Phase C: wire Phase 3 orchestration onto the real [RegionDownloadBackground]
 * queue and place-index path ([RegionDownloadBackground.claimWorker] / core
 * `PLACE_INDEX_BUILD_LOCK`).
 *
 * Toggle ON: start-region check → adjacency corridor → [LongTripPackStorage]
 * target → sequential enqueue with the Wi‑Fi/Ethernet gate applied inside
 * [RegionDownloadBackground.ensureStarted] (`requireUnmetered = true`).
 * Toggle OFF: [RegionDownloadBackground.cancelPending] (keeps installed data).
 *
 * Volume eject/scrub → [State.Unavailable] (mirrors core
 * `RegionTripState::Unavailable`); the UI watch in MainActivity forwards stems
 * via [onVolumeUnavailable].
 */
object LongTripCoordinator {
    private const val TAG = "LongTripCoord"

    enum class State {
        Needed,
        Queued,
        Downloading,
        Installed,
        Indexing,
        Indexed,
        Failed,
        Paused,
        Unavailable,
    }

    data class Plan(
        val startRegion: String,
        val regionsInOrder: List<String>,
        val states: MutableMap<String, State>,
    )

    fun interface CorridorProvider {
        fun orderedRegions(
            waypoints: List<Pair<Double, Double>>,
            installed: List<String>,
            countryIso: String?,
        ): Result<List<String>>
    }

    /**
     * Starts one region on the download queue. Production routes to
     * [RegionDownloadBackground.ensureStarted] with [requireUnmetered] = true.
     */
    fun interface DownloadStarter {
        fun start(
            context: Context?,
            dataDir: File,
            url: String,
            filename: String,
            geofabrikPath: String,
            packDir: File?,
            requireUnmetered: Boolean,
        )
    }

    fun interface PackTargetResolver {
        fun resolve(
            context: Context?,
            regionId: String,
        ): LongTripPackStorage.PackTarget
    }

    /** Production corridor via UniFFI adjacency graph. */
    val defaultCorridorProvider =
        CorridorProvider { waypoints, installed, countryIso ->
            runCatching {
                val wpJson =
                    JSONArray()
                        .also { arr ->
                            for ((lat, lon) in waypoints) {
                                arr.put(JSONArray().put(lat).put(lon))
                            }
                        }.toString()
                val instJson = JSONArray(installed).toString()
                val raw =
                    longTripOrderedRegionsJson(
                        waypointsLatLonJson = wpJson,
                        installedRegionIdsJson = instJson,
                        countryIso = countryIso,
                    )
                val obj = JSONObject(raw)
                if (!obj.optBoolean("ok", false)) {
                    error(obj.optString("error", "missing corridor"))
                }
                val regions = obj.getJSONArray("regions")
                buildList {
                    for (i in 0 until regions.length()) {
                        add(regions.getString(i))
                    }
                }
            }
        }

    private val defaultDownloadStarter =
        DownloadStarter { context, dataDir, url, filename, geofabrikPath, packDir, requireUnmetered ->
            val ctx =
                context
                    ?: error("LongTripCoordinator download starter requires Context")
            // Real wiring point for the Wi‑Fi/Ethernet-only gate.
            RegionDownloadBackground.ensureStarted(
                context = ctx,
                dataDir = dataDir,
                url = url,
                filename = filename,
                geofabrikPath = geofabrikPath,
                packDir = packDir,
                requireUnmetered = requireUnmetered,
            )
        }

    private val defaultPackTargetResolver =
        PackTargetResolver { context, regionId ->
            val ctx =
                context
                    ?: error("LongTripCoordinator pack target requires Context")
            LongTripPackStorage.resolvePackTarget(ctx, regionId)
        }

    private val enabled = AtomicBoolean(false)
    private val planRef = AtomicReference<Plan?>(null)
    private val statusLine = AtomicReference("")
    private var corridorProvider: CorridorProvider = defaultCorridorProvider
    private var downloadStarter: DownloadStarter = defaultDownloadStarter
    private var packTargetResolver: PackTargetResolver = defaultPackTargetResolver
    private val phaseListener =
        RegionDownloadBackground.PhaseListener { path, phase ->
            onBackgroundPhase(path, phase)
        }

    fun isEnabled(): Boolean = enabled.get()

    fun statusLine(): String = statusLine.get()

    fun currentPlan(): Plan? = planRef.get()

    fun setCorridorProviderForTests(provider: CorridorProvider?) {
        corridorProvider = provider ?: defaultCorridorProvider
    }

    fun setDownloadStarterForTests(starter: DownloadStarter?) {
        downloadStarter = starter ?: defaultDownloadStarter
    }

    fun setPackTargetResolverForTests(resolver: PackTargetResolver?) {
        packTargetResolver = resolver ?: defaultPackTargetResolver
    }

    /**
     * Toggle ON: register phase listener, compute corridor, enqueue downloads on
     * the real queue (unmetered-gated). [waypoints] are trip O/D/vias.
     */
    fun enable(
        context: Context,
        waypoints: List<Pair<Double, Double>>,
        countryIso: String? = null,
        installedHint: List<String> = emptyList(),
    ): String {
        val internal = NaviAppData.resolve(context)
        return enableWithDataDir(
            context = context,
            dataDir = internal,
            waypoints = waypoints,
            countryIso = countryIso,
            installedHint = installedHint,
        )
    }

    /**
     * Host-test entry: same sequencing as [enable] without [NaviAppData] /
     * volume watch (MainActivity owns the watch in production).
     */
    fun enableWithDataDir(
        context: Context?,
        dataDir: File,
        waypoints: List<Pair<Double, Double>>,
        countryIso: String? = null,
        installedHint: List<String> = emptyList(),
    ): String {
        RegionDownloadBackground.addPhaseListener(phaseListener)
        enabled.set(true)

        if (waypoints.size < 2) {
            statusLine.set("Long trip on — set origin and destination")
            return statusLine.get()
        }

        val installed = installedHint
        val corridor =
            corridorProvider.orderedRegions(waypoints, installed, countryIso).getOrElse { e ->
                statusLine.set("Long trip: ${e.message}")
                Log.w(TAG, "corridor failed", e)
                return statusLine.get()
            }
        if (corridor.isEmpty()) {
            statusLine.set("Long trip on — no regions needed")
            return statusLine.get()
        }

        val start = corridor.first()
        val states = ConcurrentHashMap<String, State>()
        for (r in corridor) states[r] = State.Needed
        // Step 1: start region already has packs → Installed (or Indexed when
        // place-search is ready). Do not require place-index to skip re-download.
        when (val target = packTargetResolver.resolve(context, start)) {
            is LongTripPackStorage.PackTarget.ReuseInternal -> {
                markReadyIfPacksPresent(
                    states,
                    start,
                    target.dataDir,
                    dataDir,
                )
            }
            is LongTripPackStorage.PackTarget.DownloadTo -> {
                markReadyIfPacksPresent(
                    states,
                    start,
                    target.packDir,
                    dataDir,
                )
            }
        }

        val plan = Plan(start, corridor, states)
        planRef.set(plan)
        refreshStatusLine()

        // Enqueue regions that still need packs (Installed/Indexed are done).
        for (regionId in corridor) {
            val st = states[regionId]
            if (st == State.Indexed || st == State.Installed) continue
            enqueueRegion(context, dataDir, regionId, states)
        }
        // Drop a persisted re-download job when every corridor region already has
        // Ready packs (e.g. stub PBF left a JOB_FILE that would resume HTTP).
        if (corridorReadyForPlanning()) {
            RegionDownloadBackground.cancelPending(dataDir)
            Log.i(TAG, "corridor packs ready — cancelled leftover download job/queue")
        }
        return statusLine.get()
    }

    fun disable(context: Context): String {
        enabled.set(false)
        RegionDownloadBackground.removePhaseListener(phaseListener)
        val internal = NaviAppData.resolve(context)
        RegionDownloadBackground.cancelPending(internal)
        planRef.get()?.states?.forEach { (id, st) ->
            if (st == State.Needed ||
                st == State.Downloading ||
                st == State.Indexing ||
                st == State.Paused
            ) {
                planRef.get()?.states?.put(id, State.Paused)
            }
        }
        statusLine.set("Long trip off (installed packs kept)")
        return statusLine.get()
    }

    /** Host-test disable that cancels the queue under [dataDir] (no Context). */
    fun disableWithDataDir(dataDir: File): String {
        enabled.set(false)
        RegionDownloadBackground.removePhaseListener(phaseListener)
        RegionDownloadBackground.cancelPending(dataDir)
        planRef.get()?.states?.forEach { (id, st) ->
            if (st == State.Needed ||
                st == State.Downloading ||
                st == State.Indexing ||
                st == State.Paused
            ) {
                planRef.get()?.states?.put(id, State.Paused)
            }
        }
        statusLine.set("Long trip off (installed packs kept)")
        return statusLine.get()
    }

    /**
     * Map mid-write scrub stems from [LongTripPackStorage] onto
     * [State.Unavailable] (core `RegionTripState::Unavailable`).
     */
    fun onVolumeUnavailable(
        volumeId: String,
        stems: List<String>,
    ): String {
        markUnavailable(stems, volumeId)
        return statusLine.get()
    }

    private fun enqueueRegion(
        context: Context?,
        internal: File,
        regionId: String,
        states: MutableMap<String, State>,
    ) {
        val target = packTargetResolver.resolve(context, regionId)
        when (target) {
            is LongTripPackStorage.PackTarget.ReuseInternal -> {
                if (markReadyIfPacksPresent(states, regionId, target.dataDir, internal)) {
                    return
                }
                // Manifest alone is not enough when the PBF is a stub/missing —
                // re-download so place-index / local bake can finish.
                val packPath = GeofabrikDownloadCatalog.canonicalizePath(regionId)
                val extractPath = GeofabrikDownloadCatalog.extractPathForPbf(packPath)
                val leaf = extractPath.substringAfterLast('/')
                val filename = "$leaf-latest.osm.pbf"
                val url =
                    runCatching { geofabrikLatestPbfUrl(packPath) }
                        .getOrElse { "https://download.geofabrik.de/$extractPath-latest.osm.pbf" }
                states[regionId] = State.Downloading
                downloadStarter.start(
                    context = context,
                    dataDir = internal,
                    url = url,
                    filename = filename,
                    geofabrikPath = packPath,
                    packDir = target.dataDir,
                    requireUnmetered = true,
                )
                refreshStatusLine()
            }
            is LongTripPackStorage.PackTarget.DownloadTo -> {
                if (markReadyIfPacksPresent(states, regionId, target.packDir, internal)) {
                    return
                }
                val packPath = GeofabrikDownloadCatalog.canonicalizePath(regionId)
                val extractPath = GeofabrikDownloadCatalog.extractPathForPbf(packPath)
                val leaf = extractPath.substringAfterLast('/')
                val filename = "$leaf-latest.osm.pbf"
                val url =
                    runCatching { geofabrikLatestPbfUrl(packPath) }
                        .getOrElse { "https://download.geofabrik.de/$extractPath-latest.osm.pbf" }
                states[regionId] = State.Downloading
                downloadStarter.start(
                    context = context,
                    dataDir = internal,
                    url = url,
                    filename = filename,
                    geofabrikPath = packPath,
                    packDir = target.packDir,
                    requireUnmetered = true,
                )
                refreshStatusLine()
            }
        }
    }

    /**
     * If routing packs are already on disk under [packDir] (Ready manifest),
     * mark [State.Installed] (or [State.Indexed] when place-search is ready) and
     * skip HTTP re-download.
     *
     * A stub/missing Geofabrik `.osm.pbf` must **not** force re-download when
     * pack-server tiles + manifest are present — corridor planning loads rkyv
     * packs, and place-index is tracked separately via [PlaceIndexReady].
     *
     * @return true when the region was marked ready and the caller should stop.
     */
    private fun markReadyIfPacksPresent(
        states: MutableMap<String, State>,
        regionId: String,
        packDir: File,
        placeIndexDataDir: File,
    ): Boolean {
        if (!PackRegionAvailability.localBakeReady(packDir, regionId)) {
            return false
        }
        states[regionId] =
            if (PlaceIndexReady.isReady(placeIndexDataDir, regionId)) {
                State.Indexed
            } else {
                State.Installed
            }
        refreshStatusLine()
        return true
    }

    private fun onBackgroundPhase(
        path: String,
        phase: String,
    ) {
        val plan = planRef.get() ?: return
        val key =
            plan.regionsInOrder.firstOrNull {
                PackRegionAvailability.regionIdsMatchForCatalog(it, path)
            } ?: return
        when {
            phase == "queued" -> plan.states[key] = State.Queued
            phase == "downloading" -> plan.states[key] = State.Downloading
            phase == "installed" -> plan.states[key] = State.Installed
            phase == "indexing" -> {
                // Packs Ready (Installed) must stay planning-ready while place
                // index builds in the background — never downgrade to Indexing.
                val cur = plan.states[key]
                if (cur != State.Installed && cur != State.Indexed) {
                    plan.states[key] = State.Indexing
                }
            }
            phase == "indexed" -> plan.states[key] = State.Indexed
            phase == "paused_unmetered" -> plan.states[key] = State.Paused
            phase.startsWith("unavailable") -> plan.states[key] = State.Unavailable
            phase == "failed" -> plan.states[key] = State.Failed
        }
        refreshStatusLine()
    }

    private fun markUnavailable(
        stems: List<String>,
        volumeId: String,
    ) {
        val plan =
            planRef.get() ?: run {
                statusLine.set("Storage unavailable ($volumeId)")
                return
            }
        for ((id, st) in plan.states.entries.toList()) {
            if (st == State.Downloading || st == State.Indexing) {
                plan.states[id] = State.Unavailable
            }
        }
        for (stem in stems) {
            val stemNorm = stem.lowercase().replace('_', '-')
            for (id in plan.regionsInOrder) {
                val leaf =
                    id
                        .substringAfterLast('/')
                        .lowercase()
                        .replace('_', '-')
                if (leaf.isNotEmpty() && (stemNorm.contains(leaf) || leaf.contains(stemNorm))) {
                    plan.states[id] = State.Unavailable
                }
            }
        }
        refreshStatusLine()
        // Keep an Unavailable token in the status line for UI / tests (refresh alone
        // already embeds per-region State.Unavailable).
        val detail = statusLine.get()
        statusLine.set("Storage unavailable ($volumeId) — $detail")
    }

    private fun refreshStatusLine() {
        val plan = planRef.get() ?: return
        val total = plan.regionsInOrder.size
        val parts =
            plan.regionsInOrder.mapIndexed { i, id ->
                RegionProgressMessages.longTripPart(
                    regionId = id,
                    state = (plan.states[id] ?: State.Needed).name,
                    index = i + 1,
                    total = total,
                )
            }
        val line = parts.joinToString(" · ")
        statusLine.set(line)
        Log.i(TAG, "status: $line")
    }

    /**
     * True when [regionId] is Indexed/Installed so the planner may use it while
     * other regions still download or index (non-blocking proof helper).
     *
     * Installed = routing graph packs Ready (and extract on disk). Indexed adds
     * place-search readiness; corridor planning does not require Indexed for
     * non-start regions.
     */
    fun regionReadyForPlanning(regionId: String): Boolean {
        val st =
            planRef
                .get()
                ?.states
                ?.entries
                ?.firstOrNull {
                    PackRegionAvailability.regionIdsMatchForCatalog(it.key, regionId)
                }?.value ?: return false
        return st == State.Indexed || st == State.Installed
    }

    /**
     * True when every corridor region has reached Installed or Indexed so a
     * multi-stem plan may load Ready packs along the whole trip. Place-index
     * work may still be running for Installed regions.
     */
    fun corridorReadyForPlanning(): Boolean {
        val plan = planRef.get() ?: return false
        if (plan.regionsInOrder.isEmpty()) return false
        return plan.regionsInOrder.all { regionReadyForPlanning(it) }
    }

    /** Reset listeners/providers between host tests. */
    fun resetForTests() {
        enabled.set(false)
        planRef.set(null)
        statusLine.set("")
        RegionDownloadBackground.removePhaseListener(phaseListener)
        corridorProvider = defaultCorridorProvider
        downloadStarter = defaultDownloadStarter
        packTargetResolver = defaultPackTargetResolver
    }
}
