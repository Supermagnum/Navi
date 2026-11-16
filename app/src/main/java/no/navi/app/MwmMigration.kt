package no.navi.app

import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.Looper
import android.os.StatFs
import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.json.JSONArray
import org.json.JSONObject
import uniffi.navi.defaultPackServerBaseUrl
import java.io.File
import java.util.concurrent.Executors

/**
 * Phase 7: moves a user's `.rkyv` region downloads to CoMaps leaf sets.
 *
 * Nothing here starts without a tap: [onAppStart] only detects and (when the user has not said
 * "Later") shows the offer. The plan, the pinned catalog and the resumable state live in Rust
 * (`core/src/mwm/migrate.rs`); this object owns the Compose state, the detection of old regions,
 * and the foreground service that runs the download ([MwmMigrationService]).
 */
object MwmMigration {
    private const val TAG = "MwmMigration"

    data class Detected(
        val regionId: String,
        val oldBytes: Long,
    )

    data class PlanRow(
        val regionId: String,
        val displayName: String,
        val oldBytes: Long,
        val mapped: Boolean,
        val selected: Boolean,
        val leafIds: List<String>,
        val regionBytes: Long,
        val note: String?,
    )

    data class PlanLeaf(
        val id: String,
        val displayName: String,
        val bytes: Long,
        val installed: Boolean,
        val sharedBy: List<String>,
        val mostlyOutside: Boolean,
        val excluded: Boolean,
    )

    data class Plan(
        val version: String,
        val rows: List<PlanRow>,
        val leaves: List<PlanLeaf>,
        val unavailableLeaves: List<String>,
        val worldNeeded: Boolean,
        val downloadBytes: Long,
        val totalBytes: Long,
        val sharedSavedBytes: Long,
        val freeNeeded: Long,
        val freeAvailable: Long,
        val enoughSpace: Boolean,
        val spaceShortBy: Long,
        val oldBytesSelected: Long,
        val oldBytesUnmapped: Long,
        val large: Boolean,
        val wifiOnlyDefault: Boolean,
    )

    data class Persisted(
        val version: String,
        val status: String,
        val leaves: List<String>,
        val oldRegions: List<Detected>,
        val wifiOnly: Boolean,
        val downloadBytes: Long,
        val error: String?,
        val decision: String,
        val bytesFreed: Long,
    ) {
        val inProgress: Boolean
            get() = status in setOf("queued", "running", "paused", "waiting_wifi", "waiting_network", "failed")
        val needsDecision: Boolean
            get() = status == "verified"
    }

    data class Progress(
        val done: Long = 0,
        val total: Long = 0,
        val running: Boolean = false,
    )

    data class DoneInfo(
        val verifying: Boolean = true,
        val verified: Boolean = false,
        val error: String? = null,
        val oldBytes: Long = 0,
        val result: String? = null,
    )

    data class UiState(
        val offerVisible: Boolean = false,
        val chooseMode: Boolean = false,
        val detected: List<Detected> = emptyList(),
        val selected: Set<String>? = null,
        val excluded: Set<String> = emptySet(),
        val plan: Plan? = null,
        val planError: String? = null,
        val planLoading: Boolean = false,
        val wifiOnly: Boolean = true,
        val wifiOnlyTouched: Boolean = false,
        val persisted: Persisted? = null,
        val progress: Progress = Progress(),
        val progressVisible: Boolean = false,
        val done: DoneInfo? = null,
        val startError: String? = null,
        val storageVisible: Boolean = false,
    )

    var ui by mutableStateOf(UiState())
        private set

    private val executor =
        Executors.newSingleThreadExecutor { r -> Thread(r, "mwm-migration").apply { isDaemon = true } }
    private val main = Handler(Looper.getMainLooper())

    private fun update(f: (UiState) -> UiState) {
        if (Looper.myLooper() == Looper.getMainLooper()) ui = f(ui) else main.post { ui = f(ui) }
    }

    fun dataDir(context: Context): File = NaviAppData.resolve(context)

    // ------------------------------------------------------------------ detection

    /** `.rkyv` regions with a loadable graph, with what their files occupy on disk. */
    fun detectRkyv(context: Context): List<Detected> {
        val dataDir = dataDir(context)
        val snap =
            InstalledMaps.current() ?: run {
                runCatching { InstalledMaps.refresh(context) }
                InstalledMaps.current()
            } ?: return emptyList()
        return snap.regions.values
            .filter { it.regionId != BasemapStyleResolver.WORLD_OVERVIEW_REGION_KEY && it.profilesLoadable.isNotEmpty() }
            .map { r ->
                Detected(r.regionId, DownloadedRegionDelete.measureBytes(context, dataDir, r.regionId))
            }.sortedBy { it.regionId }
    }

    /** Old regions the user migrated and chose to keep (never offered again, listed under Map storage). */
    fun keptRegions(context: Context): List<Detected> =
        runCatching {
            val a = JSONArray(uniffi.navi.mwmMigrationKeptRegions(dataDir(context).absolutePath))
            (0 until a.length()).map { Detected(a.getJSONObject(it).getString("region_id"), a.getJSONObject(it).getLong("old_bytes")) }
        }.getOrDefault(emptyList())

    /** Old regions that still wait for a decision: detected, and not already migrated and kept. */
    fun detectPending(context: Context): List<Detected> {
        val kept = keptRegions(context).map { it.regionId }.toSet()
        return detectRkyv(context).filter { it.regionId !in kept }
    }

    fun freeBytes(context: Context): Long = runCatching { StatFs(dataDir(context).path).availableBytes }.getOrDefault(0L)

    // ------------------------------------------------------------------ JSON

    private fun detectedJson(list: List<Detected>): String =
        JSONArray()
            .apply {
                list.forEach {
                    put(JSONObject().put("region_id", it.regionId).put("old_bytes", it.oldBytes))
                }
            }.toString()

    private fun listJson(items: Collection<String>?): String = if (items == null) "" else JSONArray(items.toList()).toString()

    private fun strings(a: JSONArray?): List<String> = (0 until (a?.length() ?: 0)).map { a!!.getString(it) }

    internal fun parsePlan(j: JSONObject): Plan {
        val rows =
            (0 until j.getJSONArray("rows").length()).map {
                val r = j.getJSONArray("rows").getJSONObject(it)
                PlanRow(
                    regionId = r.getString("region_id"),
                    displayName = r.getString("display_name"),
                    oldBytes = r.getLong("old_bytes"),
                    mapped = r.getBoolean("mapped"),
                    selected = r.getBoolean("selected"),
                    leafIds = strings(r.optJSONArray("leaf_ids")),
                    regionBytes = r.getLong("region_bytes"),
                    note = r.optString("note").takeIf { n -> n.isNotBlank() && n != "null" },
                )
            }
        val leaves =
            (0 until j.getJSONArray("leaves").length()).map {
                val l = j.getJSONArray("leaves").getJSONObject(it)
                PlanLeaf(
                    id = l.getString("id"),
                    displayName = l.getString("display_name"),
                    bytes = l.getLong("bytes"),
                    installed = l.getBoolean("installed"),
                    sharedBy = strings(l.optJSONArray("shared_by")),
                    mostlyOutside = l.getBoolean("mostly_outside"),
                    excluded = l.getBoolean("excluded"),
                )
            }
        return Plan(
            version = j.getString("version"),
            rows = rows,
            leaves = leaves,
            unavailableLeaves = strings(j.optJSONArray("unavailable_leaves")),
            worldNeeded = j.getBoolean("world_needed"),
            downloadBytes = j.getLong("download_bytes"),
            totalBytes = j.getLong("total_bytes"),
            sharedSavedBytes = j.getLong("shared_saved_bytes"),
            freeNeeded = j.getLong("free_bytes_needed"),
            freeAvailable = j.getLong("free_bytes_available"),
            enoughSpace = j.getBoolean("enough_space"),
            spaceShortBy = j.getLong("space_short_by"),
            oldBytesSelected = j.getLong("old_bytes_selected"),
            oldBytesUnmapped = j.getLong("old_bytes_unmapped"),
            large = j.getBoolean("large"),
            wifiOnlyDefault = j.getBoolean("wifi_only_default"),
        )
    }

    internal fun parsePersisted(text: String): Persisted? {
        if (text.isBlank() || text == "null") return null
        val j = runCatching { JSONObject(text) }.getOrNull() ?: return null
        val old = j.optJSONArray("old_regions")
        return Persisted(
            version = j.optString("version"),
            status = j.optString("status"),
            leaves = strings(j.optJSONArray("leaves")),
            oldRegions =
                (0 until (old?.length() ?: 0)).map {
                    val o = old!!.getJSONObject(it)
                    Detected(o.getString("region_id"), o.getLong("old_bytes"))
                },
            wifiOnly = j.optBoolean("wifi_only", true),
            downloadBytes = j.optLong("download_bytes"),
            error = j.optString("error").takeIf { it.isNotBlank() && it != "null" },
            decision = j.optString("old_maps_decision", "pending"),
            bytesFreed = j.optLong("bytes_freed"),
        )
    }

    fun readPersisted(context: Context): Persisted? = parsePersisted(uniffi.navi.mwmMigrationState(dataDir(context).absolutePath))

    // ------------------------------------------------------------------ planning

    /** Replan with the current selection (network on the first call, cached afterwards). */
    fun replan(
        context: Context,
        refresh: Boolean = false,
        detectedOverride: List<Detected>? = null,
    ) {
        val app = context.applicationContext
        // UI state updates from the executor thread are posted to the main thread, so a caller that has
        // just detected the regions passes them in instead of reading a state that is not applied yet.
        val st = if (detectedOverride != null) ui.copy(detected = detectedOverride, selected = null, excluded = emptySet()) else ui
        update { it.copy(planLoading = true, planError = null) }
        executor.execute {
            val dir = dataDir(app).absolutePath
            val free = freeBytes(app)
            val res =
                runCatching {
                    JSONObject(
                        uniffi.navi.mwmMigrationPlan(
                            dir,
                            defaultPackServerBaseUrl(),
                            detectedJson(st.detected),
                            listJson(st.selected),
                            listJson(st.excluded),
                            free.toULong(),
                            refresh,
                        ),
                    )
                }.getOrElse { JSONObject().put("ok", false).put("error", it.message ?: "plan failed") }
            if (res.optBoolean("ok")) {
                val plan = parsePlan(res.getJSONObject("plan"))
                Log.i(
                    TAG,
                    "plan version=${plan.version} regions=${plan.rows.size} leaves=${plan.leaves.size} " +
                        "download=${plan.downloadBytes} needed=${plan.freeNeeded} free=${plan.freeAvailable} " +
                        "enough=${plan.enoughSpace} unmapped_old=${plan.oldBytesUnmapped}",
                )
                update {
                    it.copy(
                        plan = plan,
                        planLoading = false,
                        planError = null,
                        wifiOnly = if (it.wifiOnlyTouched) it.wifiOnly else plan.wifiOnlyDefault,
                    )
                }
            } else {
                val msg = res.optString("error", "plan failed")
                Log.w(TAG, "plan failed: $msg")
                update { it.copy(planLoading = false, planError = msg) }
            }
        }
    }

    // ------------------------------------------------------------------ flow entry points

    /**
     * Called once per app start (MWM path only). Never downloads: it detects the old regions,
     * resumes an interrupted migration the user already chose, or shows the offer.
     */
    fun onAppStart(context: Context) {
        if (!MwmClientBootstrap.isEnabled()) return
        val app = context.applicationContext
        executor.execute {
            val dir = dataDir(app).absolutePath
            val persisted = readPersisted(app)
            when {
                persisted != null && persisted.inProgress -> {
                    update { it.copy(persisted = persisted, progressVisible = true, wifiOnly = persisted.wifiOnly, wifiOnlyTouched = true) }
                    refreshProgress(app)
                    // Interrupted by an app / process restart: continue what the user chose.
                    if (persisted.status in setOf("queued", "running", "waiting_wifi", "waiting_network")) {
                        Log.i(TAG, "resume after restart status=${persisted.status}")
                        main.post { MwmMigrationService.start(app, MwmMigrationService.ACTION_RESUME) }
                    }
                }
                persisted != null && persisted.needsDecision -> {
                    update { it.copy(persisted = persisted) }
                    main.post { openDoneDialog(app) }
                }
                else -> {
                    val detected = detectPending(app)
                    val offer = detected.isNotEmpty() && uniffi.navi.mwmMigrationShouldOffer(dir)
                    Log.i(TAG, "start detected=${detected.map { it.regionId to it.oldBytes }} offer=$offer")
                    update { it.copy(detected = detected, persisted = persisted) }
                    if (offer) {
                        update { it.copy(offerVisible = true, chooseMode = false, selected = null, excluded = emptySet()) }
                        replan(app, detectedOverride = detected)
                    }
                }
            }
        }
    }

    /** Open the migration screen on demand (Map storage entry), even after "Later". */
    fun openOffer(context: Context) {
        val app = context.applicationContext
        executor.execute {
            val persisted = readPersisted(app)
            if (persisted != null && persisted.inProgress) {
                update { it.copy(persisted = persisted, progressVisible = true) }
                refreshProgress(app)
                return@execute
            }
            if (persisted != null && persisted.needsDecision) {
                main.post { openDoneDialog(app) }
                return@execute
            }
            val detected = detectPending(app)
            update {
                it.copy(
                    detected = detected,
                    offerVisible = true,
                    chooseMode = false,
                    selected = null,
                    excluded = emptySet(),
                )
            }
            if (detected.isNotEmpty()) replan(app, detectedOverride = detected)
        }
    }

    fun later(context: Context) {
        val dir = dataDir(context).absolutePath
        executor.execute { uniffi.navi.mwmMigrationRemindLater(dir) }
        update { it.copy(offerVisible = false, chooseMode = false, progressVisible = false) }
    }

    fun setChooseMode(on: Boolean) = update { it.copy(chooseMode = on) }

    fun toggleRegion(
        context: Context,
        regionId: String,
    ) {
        val all =
            ui.plan
                ?.rows
                ?.filter { it.mapped }
                ?.map { it.regionId }
                ?.toSet() ?: return
        val current = ui.selected ?: all
        val next = if (regionId in current) current - regionId else current + regionId
        update { it.copy(selected = next) }
        replan(context)
    }

    fun toggleLeaf(
        context: Context,
        leafId: String,
    ) {
        val ex = ui.excluded
        val next = if (leafId in ex) ex - leafId else ex + leafId
        update { it.copy(excluded = next) }
        replan(context)
    }

    fun setWifiOnly(
        context: Context,
        on: Boolean,
    ) {
        update { it.copy(wifiOnly = on, wifiOnlyTouched = true) }
        if (ui.persisted?.inProgress == true) {
            val dir = dataDir(context).absolutePath
            executor.execute {
                uniffi.navi.mwmMigrationSetWifiOnly(dir, on)
                main.post { MwmMigrationService.poke(context.applicationContext) }
            }
        }
    }

    /** "Migrate now": persist the choice and start the foreground service. */
    fun startNow(context: Context) {
        val app = context.applicationContext
        val st = ui
        update { it.copy(startError = null) }
        executor.execute {
            val dir = dataDir(app).absolutePath
            val res =
                runCatching {
                    JSONObject(
                        uniffi.navi.mwmMigrationStart(
                            dir,
                            defaultPackServerBaseUrl(),
                            detectedJson(st.detected),
                            listJson(st.selected),
                            listJson(st.excluded),
                            freeBytes(app).toULong(),
                            st.wifiOnly,
                        ),
                    )
                }.getOrElse { JSONObject().put("ok", false).put("error", it.message ?: "start failed") }
            if (!res.optBoolean("ok")) {
                val msg = res.optString("error", "start failed")
                Log.w(TAG, "start failed: $msg")
                update { it.copy(startError = msg) }
                return@execute
            }
            val persisted = readPersisted(app)
            Log.i(TAG, "started leaves=${persisted?.leaves?.size} wifi_only=${st.wifiOnly}")
            update { it.copy(offerVisible = false, chooseMode = false, persisted = persisted, progressVisible = true) }
            main.post { MwmMigrationService.start(app, MwmMigrationService.ACTION_START) }
        }
    }

    fun pause(context: Context) = MwmMigrationService.start(context.applicationContext, MwmMigrationService.ACTION_PAUSE)

    fun resume(context: Context) = MwmMigrationService.start(context.applicationContext, MwmMigrationService.ACTION_RESUME)

    fun cancelMigration(context: Context) {
        val app = context.applicationContext
        MwmMigrationService.start(app, MwmMigrationService.ACTION_CANCEL)
        update { it.copy(progressVisible = false, persisted = null, progress = Progress()) }
    }

    fun hideProgress() = update { it.copy(progressVisible = false) }

    fun showProgress(context: Context) {
        update { it.copy(progressVisible = true) }
        executor.execute { refreshProgress(context.applicationContext) }
    }

    /** Re-read the persisted state and the byte progress (called by the UI poller). */
    fun refreshProgress(context: Context) {
        val dir = dataDir(context).absolutePath
        val persisted = readPersisted(context)
        val p =
            runCatching { JSONObject(uniffi.navi.mwmMigrationProgress(dir)) }
                .map { Progress(it.optLong("done"), it.optLong("total"), it.optBoolean("running")) }
                .getOrDefault(Progress())
        update { it.copy(persisted = persisted, progress = p) }
        if (persisted?.needsDecision == true && ui.done == null) {
            main.post { openDoneDialog(context.applicationContext) }
        }
    }

    // ------------------------------------------------------------------ after the switch

    /** The new maps are in; verify from disk before the old maps are even mentioned. */
    fun openDoneDialog(context: Context) {
        val app = context.applicationContext
        val persisted = ui.persisted ?: readPersisted(app) ?: return
        if (ui.done != null) return
        update { it.copy(done = DoneInfo(verifying = true), progressVisible = false, persisted = persisted) }
        executor.execute {
            val dir = dataDir(app).absolutePath
            val ver = runCatching { JSONObject(uniffi.navi.mwmMigrationVerify(dir)) }.getOrNull()
            val ok = ver?.optBoolean("ok") == true
            val bytes = persisted.oldRegions.sumOf { r -> DownloadedRegionDelete.measureBytes(app, dataDir(app), r.regionId) }
            Log.i(TAG, "deep verify ok=$ok err=${ver?.optString("error")} old_bytes=$bytes")
            update { it.copy(done = DoneInfo(verifying = false, verified = ok, error = ver?.optString("error"), oldBytes = bytes)) }
        }
    }

    /** User chose "Delete old maps": only the regions that were migrated, only after verification. */
    fun deleteOldMaps(context: Context) {
        val app = context.applicationContext
        val persisted = ui.persisted ?: return
        if (ui.done?.verified != true) return
        executor.execute {
            val dir = dataDir(app)
            var freed = 0L
            val failed = mutableListOf<String>()
            for (r in persisted.oldRegions) {
                val res = DownloadedRegionDelete.delete(app, dir, r.regionId)
                if (res.ok) freed += res.bytesFreed else failed += "${r.regionId}: ${res.message}"
            }
            Log.i(TAG, "old maps deleted freed=$freed failed=$failed")
            uniffi.navi.mwmMigrationOldMapsDecision(dir.absolutePath, "deleted", freed.toULong())
            runCatching { InstalledMaps.refresh(app) }
            update {
                it.copy(
                    done = (it.done ?: DoneInfo()).copy(result = if (failed.isEmpty()) "deleted" else "partial:" + failed.joinToString("; "), oldBytes = freed),
                    persisted = readPersisted(app),
                )
            }
        }
    }

    fun keepOldMaps(context: Context) {
        val app = context.applicationContext
        executor.execute {
            uniffi.navi.mwmMigrationOldMapsDecision(dataDir(app).absolutePath, "kept", 0uL)
            Log.i(TAG, "old maps kept")
            update { it.copy(done = (it.done ?: DoneInfo()).copy(result = "kept"), persisted = readPersisted(app)) }
        }
    }

    fun closeDone() = update { it.copy(done = null) }

    // ------------------------------------------------------------------ Map storage screen

    fun showStorage(show: Boolean) = update { it.copy(storageVisible = show) }

    /** Called by the service on the main thread once the new maps are switched in and verified. */
    internal fun onVerified(context: Context) {
        val app = context.applicationContext
        Thread {
            runCatching { ComapsSessionBootstrap.ensureOpen(app, force = true) }
            main.post { ComapsDrape.backend?.registerMaps() }
            MwmMapDownloader.refreshInstalled(app)
            refreshProgress(app)
        }.start()
    }

    fun serviceIntent(
        context: Context,
        action: String,
    ): Intent = Intent(context, MwmMigrationService::class.java).setAction(action)
}
