package no.navi.app

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import uniffi.navi.FfiMwmRegion
import uniffi.navi.defaultPackServerBaseUrl
import uniffi.navi.downloadProgressSnapshot
import uniffi.navi.mwmInstallLeaves
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * In-app owner of the Phase 1 leaf download (catalog -> verified mirrors -> MWM + sidecar,
 * checksum and fingerprint verified -> register). One install at a time; installs and
 * registers without an app restart: the headless router is re-opened and the live drape
 * registers the new leaves.
 *
 * Long-trip MWM installs ([LongTripMwmSupport]) also go through here so the same Tools
 * progress strip, region-readiness panel, and MainActivity download toast see live %.
 */
object MwmMapDownloader {
    private const val TAG = "MwmMapDownloader"

    data class State(
        val running: Boolean = false,
        val label: String = "",
        val percent: Int? = null,
        val message: String = "",
        val ok: Boolean? = null,
        val version: String? = null,
        val installedLeaves: List<String> = emptyList(),
        val sidecars: Int = 0,
        val regions: List<FfiMwmRegion> = emptyList(),
        val regionsError: String? = null,
        /** True while [loadRegions] is resolving the catalog (disk or network). */
        val regionsLoading: Boolean = false,
    )

    var state by mutableStateOf(State())
        private set

    private val executor =
        Executors.newSingleThreadExecutor { r -> Thread(r, "mwm-install").apply { isDaemon = true } }
    private val main = Handler(Looper.getMainLooper())

    private val runningFlag = AtomicBoolean(false)
    private val activePathRef = AtomicReference("")
    private val activeLabelRef = AtomicReference("")
    private val uiLineRef = AtomicReference("")

    private fun set(f: (State) -> State) = main.post { state = f(state) }

    /** True while a Tools or long-trip MWM install is in flight. */
    fun isRunning(): Boolean = runningFlag.get()

    /** Geofabrik corridor id for the active long-trip / labeled install, if any. */
    fun activeGeofabrikPath(): String = activePathRef.get()

    /**
     * Live progress line for MainActivity / [RegionReadiness]
     * (e.g. `Downloading maps: Østlandet 42%`).
     */
    fun uiLine(): String = uiLineRef.get()

    /**
     * UI label for a catalog bake id (`europe_norway_ostlandet` → `Østlandet`).
     * Never use raw [FfiMwmRegion.displayName] from `pretty_region_name` for
     * Norway landsdeler — that path yields ASCII "Ostlandet".
     */
    fun regionDisplayLabel(regionId: String): String {
        val path = LongTripMwmSupport.bakeIdToGeofabrikPath(regionId)
        return RegionCoverage.displayName(path).ifBlank {
            regionId.substringAfterLast('_').replaceFirstChar {
                if (it.isLowerCase()) it.titlecase() else it.toString()
            }
        }
    }

    /**
     * Leaf set for Choose-region install: alias-first + strip foreign-country
     * overlap leaves (Denmark_* under Østlandet, etc.).
     */
    fun leafIdsForRegionInstall(
        regionId: String,
        catalogLeaves: List<String>,
    ): List<String> {
        val path = LongTripMwmSupport.bakeIdToGeofabrikPath(regionId)
        val alias =
            LongTripMwmGeofabrikAliases.leafIdsForBakeId(regionId)
                ?: LongTripMwmGeofabrikAliases.leafIdsForGeofabrikPath(path)
        val raw = alias ?: catalogLeaves
        return LongTripMwmSupport.filterLeavesForPath(regionId, raw)
    }

    /**
     * Progress from the FFI installer is often the raw leaf id (`Denmark_…`).
     * When installing a named region, keep the region label — never a foreign
     * leaf country name.
     */
    internal fun mwmInstallPhase(
        rawMessage: String,
        label: String,
        geofabrikPath: String,
    ): String {
        val region =
            geofabrikPath
                .takeIf { it.isNotBlank() }
                ?.let { RegionProgressMessages.regionName(it) }
                .orEmpty()
                .ifBlank { label }
                .ifBlank { "maps" }
        val raw = rawMessage.trim()
        if (raw.isEmpty()) return "Downloading maps: $region"
        val looksLikeLeafId =
            raw.contains('_') &&
                raw.firstOrNull()?.isUpperCase() == true &&
                raw.substringBefore('_').all { it.isLetter() }
        if (looksLikeLeafId) {
            return "Downloading maps: $region"
        }
        return raw
    }

    /** Reads the active release from disk (no network). */
    fun refreshInstalled(context: Context) {
        val dir = MwmDataDir.resolve(context).absolutePath
        executor.execute {
            val inst = runCatching { uniffi.navi.mwmInstalled(dir) }.getOrNull() ?: return@execute
            set { it.copy(installedLeaves = inst.leafIds, sidecars = inst.sidecarCount.toInt(), version = inst.version) }
        }
    }

    fun loadRegions(context: Context) {
        set { it.copy(regionsLoading = true, regionsError = null) }
        val dir = MwmDataDir.resolve(context).absolutePath
        executor.execute {
            // Disk-first via data_dir so Tools populates from catalog-cache.json
            // when the pack host is slow; then live navi → CoMaps-direct.
            val r =
                runCatching {
                    uniffi.navi.mwmListRegions(defaultPackServerBaseUrl(), dir)
                }.getOrNull()
            set {
                if (r == null || !r.ok) {
                    it.copy(
                        regionsLoading = false,
                        regionsError = r?.error ?: "catalog unavailable",
                    )
                } else {
                    it.copy(
                        regionsLoading = false,
                        regions = r.regions,
                        regionsError = null,
                        version = it.version ?: r.version,
                    )
                }
            }
        }
    }

    fun cancel() = uniffi.navi.mwmInstallCancel()

    private fun formatUiLine(
        label: String,
        message: String,
        percent: Int?,
        geofabrikPath: String,
    ): String {
        val region =
            geofabrikPath
                .takeIf { it.isNotBlank() }
                ?.let { RegionProgressMessages.regionName(it) }
                .orEmpty()
                .ifBlank { label }
        val phase = mwmInstallPhase(message, label, geofabrikPath)
        val withRegion =
            if (region.isNotBlank() && !phase.contains(region, ignoreCase = true)) {
                val seq = RegionProgressMessages.sequenceFor(geofabrikPath)
                RegionProgressMessages.annotate(
                    phase,
                    geofabrikPath.ifBlank { region },
                    seq?.first,
                    seq?.second,
                )
            } else {
                phase
            }
        return if (percent != null) "$withRegion $percent%" else withRegion
    }

    private fun publishProgress(
        label: String,
        geofabrikPath: String,
        message: String,
        percent: Int?,
    ) {
        val line = formatUiLine(label, message, percent, geofabrikPath)
        uiLineRef.set(line)
        set { it.copy(percent = percent, message = message) }
    }

    /**
     * Blocking install for callers already off the main thread (long-trip corridor).
     * Updates [state] + [uiLine] so the normal download UI can poll.
     */
    fun installBlocking(
        context: Context,
        leafIds: List<String>,
        label: String,
        geofabrikPath: String = "",
    ): Boolean {
        val app = context.applicationContext
        val dir = MwmDataDir.resolve(app).absolutePath
        // Concurrent long-trip enable() must not treat an in-flight install as
        // failure and fall through to Geofabrik PBF (404 retries while MWM is
        // already downloading / on disk). Wait, then reuse installed leaves.
        if (!runningFlag.compareAndSet(false, true)) {
            Log.w(TAG, "installBlocking waiting — already running ($label)")
            val deadlineNs = System.nanoTime() + 30L * 60L * 1_000_000_000L
            while (runningFlag.get() && System.nanoTime() < deadlineNs) {
                try {
                    Thread.sleep(400)
                } catch (_: InterruptedException) {
                    Thread.currentThread().interrupt()
                    break
                }
            }
            if (leavesPresent(dir, leafIds)) {
                Log.i(TAG, "installBlocking after wait: leaves present for $label")
                return ComapsSessionBootstrap.ensureOpen(app, force = false)
            }
            if (!runningFlag.compareAndSet(false, true)) {
                Log.w(TAG, "installBlocking still busy after wait ($label)")
                return leavesPresent(dir, leafIds) &&
                    ComapsSessionBootstrap.ensureOpen(app, force = false)
            }
        }
        val path = geofabrikPath.trim().trim('/')
        activePathRef.set(path)
        activeLabelRef.set(label)
        uiLineRef.set(formatUiLine(label, "Downloading maps", null, path))
        set {
            it.copy(
                running = true,
                label = label,
                percent = null,
                message = "Downloading maps",
                ok = null,
            )
        }
        val poller =
            Thread {
                while (!Thread.currentThread().isInterrupted) {
                    val p = runCatching { downloadProgressSnapshot() }.getOrNull()
                    if (p != null && p.label.isNotBlank()) {
                        val pct = p.percent?.toInt()
                        publishProgress(label, path, p.label, pct)
                    }
                    try {
                        Thread.sleep(400)
                    } catch (_: InterruptedException) {
                        break
                    }
                }
            }.apply {
                name = "mwm-progress"
                isDaemon = true
            }
        poller.start()
        val res =
            runCatching { mwmInstallLeaves(dir, defaultPackServerBaseUrl(), leafIds) }
                .onFailure { Log.w(TAG, "install threw", it) }
                .getOrNull()
        poller.interrupt()
        var ok = res?.ok == true
        var msg = res?.error ?: ""
        if (ok) {
            // Re-scan + re-open the router (also loads the new wetland sidecars), then let the
            // live drape register the new leaves. No restart.
            ok = ComapsSessionBootstrap.ensureOpen(app, force = true)
            if (!ok) msg = "Maps installed but the routing session did not reopen"
        }
        Log.i(
            TAG,
            "install done ok=$ok leaves=$leafIds downloaded=${res?.downloadedLeaves} " +
                "already=${res?.alreadyInstalledLeaves} sidecars=${res?.sidecarsInstalled} err=$msg",
        )
        val inst = runCatching { uniffi.navi.mwmInstalled(dir) }.getOrNull()
        if (ok) {
            main.post { ComapsDrape.backend?.registerMaps() }
        }
        runningFlag.set(false)
        activePathRef.set("")
        activeLabelRef.set("")
        uiLineRef.set("")
        main.post {
            state =
                state.copy(
                    running = false,
                    ok = ok,
                    percent = if (ok) 100 else state.percent,
                    message = if (ok) "" else msg,
                    version = inst?.version ?: state.version,
                    installedLeaves = inst?.leafIds ?: state.installedLeaves,
                    sidecars = inst?.sidecarCount?.toInt() ?: state.sidecars,
                )
        }
        return ok
    }

    /** Install [leafIds]; [onDone] runs on the main thread with success. */
    fun install(
        context: Context,
        leafIds: List<String>,
        label: String,
        geofabrikPath: String = "",
        onDone: (Boolean) -> Unit = {},
    ) {
        if (runningFlag.get() || state.running) return
        val app = context.applicationContext
        executor.execute {
            val ok =
                installBlocking(
                    context = app,
                    leafIds = leafIds,
                    label = label,
                    geofabrikPath = geofabrikPath,
                )
            main.post { onDone(ok) }
        }
    }

    private fun leavesPresent(
        mwmDir: String,
        leafIds: List<String>,
    ): Boolean {
        if (leafIds.isEmpty()) return false
        val inst = runCatching { uniffi.navi.mwmInstalled(mwmDir) }.getOrNull() ?: return false
        if (inst.version.isNullOrBlank()) return false
        val have = inst.leafIds.toSet()
        return leafIds.all { it in have }
    }

    /** Leaves (of [wanted]) that are not installed in the active release. */
    fun missing(
        context: Context,
        wanted: Collection<String>,
    ): List<String> {
        val inst = runCatching { uniffi.navi.mwmInstalled(MwmDataDir.resolve(context).absolutePath) }.getOrNull()
        // No active release under this app's data dir (e.g. a test staged its own MWM root):
        // coverage is unknown, so never claim a leaf is missing.
        if (inst?.version == null) return emptyList()
        val have = inst.leafIds.toSet()
        return wanted.filter { it !in have }
    }
}

private fun formatMb(bytes: Long): String = "%.0f MB".format(java.util.Locale.US, bytes / 1_000_000.0)

/** Tools-menu section: installed release, region list with sizes, progress and cancel. */
@Composable
internal fun MwmMapsSection(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val st = MwmMapDownloader.state
    var showRegions by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        MwmMapDownloader.refreshInstalled(context)
        // Poll disk state while the section is on screen (installs finish in the background).
        while (true) {
            delay(5_000)
            if (!MwmMapDownloader.state.running) MwmMapDownloader.refreshInstalled(context)
        }
    }
    Column(
        modifier = modifier.fillMaxWidth().testTag("mwm_maps_section"),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(stringResource(R.string.mwm_maps_title), style = MaterialTheme.typography.titleSmall)
        Text(
            text =
                if (st.installedLeaves.isEmpty()) {
                    stringResource(R.string.mwm_maps_none)
                } else {
                    stringResource(R.string.mwm_maps_installed, st.version ?: "?", st.installedLeaves.size, st.sidecars)
                },
            style = MaterialTheme.typography.bodySmall,
            modifier = Modifier.testTag("mwm_maps_installed"),
        )
        if (st.running) {
            LinearProgressIndicator(
                progress = { (st.percent ?: 0) / 100f },
                modifier = Modifier.fillMaxWidth(),
            )
            Text(
                text = "${st.label}: ${st.message} ${st.percent?.let { "$it %" } ?: ""}".trim(),
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.testTag("mwm_maps_progress"),
            )
            OutlinedButton(onClick = { MwmMapDownloader.cancel() }) { Text(stringResource(R.string.action_cancel)) }
        } else {
            if (st.ok == true) {
                Text(stringResource(R.string.mwm_maps_done), style = MaterialTheme.typography.bodySmall)
            } else if (st.ok == false) {
                Text(
                    stringResource(R.string.mwm_maps_failed, st.message),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(
                    onClick = {
                        showRegions = !showRegions
                        if (showRegions) MwmMapDownloader.loadRegions(context)
                    },
                    modifier = Modifier.testTag("btn_mwm_regions"),
                ) { Text(stringResource(R.string.mwm_maps_choose)) }
                OutlinedButton(
                    onClick = { MwmMigration.showStorage(true) },
                    modifier = Modifier.testTag("btn_mwm_map_storage"),
                ) { Text(stringResource(R.string.mwm_storage_open_button)) }
            }
            if (showRegions) {
                if (st.regionsLoading) {
                    Text(
                        stringResource(R.string.mwm_maps_loading_regions),
                        style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.testTag("mwm_maps_regions_loading"),
                    )
                }
                if (st.regionsError != null) {
                    Text(
                        stringResource(R.string.mwm_maps_failed, st.regionsError ?: ""),
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                for (r in st.regions.filter { it.id.contains("norway", ignoreCase = true) }) {
                    val uiLabel = MwmMapDownloader.regionDisplayLabel(r.id)
                    val installLeaves =
                        MwmMapDownloader.leafIdsForRegionInstall(r.id, r.leafIds)
                    val installBytes =
                        if (installLeaves == r.leafIds) {
                            r.totalBytes.toLong()
                        } else {
                            // Size unknown after stripping foreign overlap leaves.
                            r.totalBytes.toLong()
                        }
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Text(
                            "$uiLabel (${formatMb(installBytes)})",
                            style = MaterialTheme.typography.bodyMedium,
                            modifier = Modifier.weight(1f),
                        )
                        TextButton(
                            onClick = {
                                val path =
                                    LongTripMwmSupport.bakeIdToGeofabrikPath(r.id)
                                MwmMapDownloader.install(
                                    context = context,
                                    leafIds = installLeaves,
                                    label = uiLabel,
                                    geofabrikPath = path,
                                )
                            },
                            modifier = Modifier.testTag("btn_mwm_install_${r.id}"),
                        ) { Text(stringResource(R.string.mwm_maps_download)) }
                    }
                }
            }
        }
    }
}

/** Offer to download the leaves a route needs (missing-maps), naming them. */
@Composable
internal fun MwmMissingMapsDialog(
    leaves: List<String>,
    onDownload: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.mwm_missing_title)) },
        text = {
            Text(
                stringResource(R.string.mwm_missing_body, leaves.joinToString(", ") { it.replace('_', ' ') }),
                modifier = Modifier.padding(top = 4.dp).testTag("mwm_missing_body"),
            )
        },
        confirmButton = {
            TextButton(onClick = onDownload, modifier = Modifier.testTag("btn_mwm_missing_download")) {
                Text(stringResource(R.string.mwm_maps_download))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss, modifier = Modifier.testTag("btn_mwm_missing_dismiss")) {
                Text(stringResource(R.string.action_not_now))
            }
        },
    )
}

/** Used by the planner: leaf names for a point, switched off the main thread. */
internal suspend fun leafForPoint(
    lat: Double,
    lon: Double,
): String? = withContext(Dispatchers.IO) { runCatching { uniffi.navi.comapsLeafForPoint(lat, lon) }.getOrNull() }
