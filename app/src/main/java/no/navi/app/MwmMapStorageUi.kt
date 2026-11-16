package no.navi.app

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import org.json.JSONObject
import uniffi.navi.defaultPackServerBaseUrl
import java.util.concurrent.Executors

/** Data behind the "Map storage" screen: installed leaf sets per country group and what else uses space. */
object MwmStorage {
    private const val TAG = "MwmStorage"

    data class Leaf(
        val id: String,
        val name: String,
        val bytes: Long,
        val hasSidecar: Boolean,
    )

    data class Group(
        val id: String,
        val name: String,
        val bytes: Long,
        val leaves: List<Leaf>,
    )

    data class Update(
        val installed: String?,
        val current: String,
        val available: Boolean,
        val bytes: Long,
    )

    data class State(
        val loading: Boolean = true,
        val version: String? = null,
        val groups: List<Group> = emptyList(),
        val leafCount: Int = 0,
        val worldBytes: Long = 0,
        val stagingBytes: Long = 0,
        val otherReleaseBytes: Long = 0,
        val totalBytes: Long = 0,
        val update: Update? = null,
        val online: Boolean = false,
        val message: String? = null,
        val oldRegions: List<MwmMigration.Detected> = emptyList(),
        /** Old regions that still wait for a migration decision (the migrate entry is shown only then). */
        val pendingOldRegions: Int = 0,
    )

    var state by mutableStateOf(State())
        private set

    private val executor = Executors.newSingleThreadExecutor { r -> Thread(r, "mwm-storage").apply { isDaemon = true } }
    private val main = Handler(Looper.getMainLooper())

    internal fun parse(text: String): State {
        val j = JSONObject(text)
        val s = j.getJSONObject("summary")
        val groups =
            (0 until s.getJSONArray("groups").length()).map { gi ->
                val g = s.getJSONArray("groups").getJSONObject(gi)
                Group(
                    id = g.getString("id"),
                    name = g.getString("display_name"),
                    bytes = g.getLong("total_bytes"),
                    leaves =
                        (0 until g.getJSONArray("leaves").length()).map { li ->
                            val l = g.getJSONArray("leaves").getJSONObject(li)
                            Leaf(
                                l.getString("id"),
                                l.getString("display_name"),
                                l.getLong("mwm_bytes") + l.getLong("sidecar_bytes"),
                                l.getBoolean("has_sidecar"),
                            )
                        },
                )
            }
        val u = j.optJSONObject("update")
        return State(
            loading = false,
            version = s.optString("version").takeIf { it.isNotBlank() && it != "null" },
            groups = groups,
            leafCount = s.getInt("leaf_count"),
            worldBytes = s.getLong("world_bytes"),
            stagingBytes = s.getLong("staging_bytes"),
            otherReleaseBytes = s.getLong("other_release_bytes"),
            totalBytes = s.getLong("total_bytes"),
            update =
                u?.let {
                    Update(
                        it.optString("installed_version").takeIf { v -> v.isNotBlank() && v != "null" },
                        it.getString("current_version"),
                        it.getBoolean("update_available"),
                        it.getLong("update_bytes"),
                    )
                },
            online = j.optBoolean("online"),
        )
    }

    fun refresh(
        context: Context,
        network: Boolean = false,
    ) {
        val app = context.applicationContext
        main.post { state = state.copy(loading = true) }
        executor.execute {
            val dir = MwmMigration.dataDir(app)
            val res =
                runCatching { parse(uniffi.navi.mwmStorageSummary(dir.absolutePath, defaultPackServerBaseUrl(), network)) }
                    .onFailure { Log.w(TAG, "summary failed", it) }
                    .getOrElse { State(loading = false, message = it.message) }
            val old =
                MwmMigration
                    .keptRegions(app)
                    .map { r -> MwmMigration.Detected(r.regionId, DownloadedRegionDelete.measureBytes(app, dir, r.regionId)) }
                    .filter { it.oldBytes > 0 }
            val pending = MwmMigration.detectPending(app).size
            Log.i(
                TAG,
                "summary version=${res.version} leaves=${res.leafCount} groups=${res.groups.map { it.name to it.bytes }} " +
                    "world=${res.worldBytes} staging=${res.stagingBytes} older_releases=${res.otherReleaseBytes} " +
                    "update=${res.update} old_rkyv=${old.map { it.regionId to it.oldBytes }}",
            )
            main.post { state = res.copy(oldRegions = old, pendingOldRegions = pending) }
        }
    }

    fun deleteLeaf(
        context: Context,
        leafId: String,
    ) {
        val app = context.applicationContext
        executor.execute {
            val dir = MwmMigration.dataDir(app).absolutePath
            val res = JSONObject(uniffi.navi.mwmStorageDeleteLeaf(dir, leafId))
            Log.i(TAG, "delete leaf $leafId -> $res")
            main.post {
                if (res.optBoolean("ok")) {
                    runCatching { ComapsDrape.backend?.deregisterMap(leafId) }
                    Thread { runCatching { ComapsSessionBootstrap.ensureOpen(app, force = true) } }.start()
                    MwmMapDownloader.refreshInstalled(app)
                } else {
                    state = state.copy(message = res.optString("error"))
                }
            }
            refresh(app)
        }
    }

    fun deleteOldReleases(context: Context) {
        val app = context.applicationContext
        executor.execute {
            val res = JSONObject(uniffi.navi.mwmStorageDeleteOldReleases(MwmMigration.dataDir(app).absolutePath))
            Log.i(TAG, "delete old releases -> $res")
            refresh(app)
        }
    }

    /** Delete one old `.rkyv` region the user already migrated and chose to keep; verifies the new maps first. */
    fun deleteOldRegion(
        context: Context,
        regionId: String,
    ) {
        val app = context.applicationContext
        executor.execute {
            val dir = MwmMigration.dataDir(app)
            val ver = JSONObject(uniffi.navi.mwmMigrationVerify(dir.absolutePath))
            if (!ver.optBoolean("ok")) {
                main.post { state = state.copy(message = "Old map kept: new maps did not verify: ${ver.optString("error")}") }
                return@execute
            }
            val res = DownloadedRegionDelete.delete(app, dir, regionId)
            Log.i(TAG, "delete old region $regionId ok=${res.ok} freed=${res.bytesFreed} msg=${res.message}")
            if (res.ok) uniffi.navi.mwmMigrationForgetKept(dir.absolutePath, regionId)
            runCatching { InstalledMaps.refresh(app) }
            main.post { state = state.copy(message = res.message) }
            refresh(app)
        }
    }

    fun update(context: Context) {
        val installed =
            state.groups
                .flatMap { it.leaves }
                .map { it.id }
        if (installed.isEmpty()) return
        MwmMapDownloader.install(context, installed, "Update maps") { refresh(context, network = true) }
    }
}

@Composable
internal fun MwmMapStorageScreen() {
    val context = LocalContext.current
    val st = MwmStorage.state
    val dl = MwmMapDownloader.state
    val mig = MwmMigration.ui
    var confirmLeaf by remember { mutableStateOf<MwmStorage.Leaf?>(null) }
    var confirmReleases by remember { mutableStateOf(false) }
    var confirmOld by remember { mutableStateOf<MwmMigration.Detected?>(null) }
    LaunchedEffect(Unit) { MwmStorage.refresh(context, network = true) }
    LaunchedEffect(dl.running) { if (!dl.running) MwmStorage.refresh(context) }
    Dialog(
        onDismissRequest = { MwmMigration.showStorage(false) },
        properties = DialogProperties(usePlatformDefaultWidth = false, dismissOnClickOutside = false),
    ) {
        Surface(Modifier.fillMaxSize().testTag("mwm_storage_screen")) {
            Column(Modifier.fillMaxSize().padding(16.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(stringResource(R.string.mwm_storage_title), style = MaterialTheme.typography.headlineSmall, modifier = Modifier.weight(1f))
                    TextButton(onClick = { MwmMigration.showStorage(false) }, modifier = Modifier.testTag("btn_mwm_storage_close")) {
                        Text(stringResource(R.string.mwm_mig_close))
                    }
                }
                Text(
                    if (st.version != null) {
                        stringResource(R.string.mwm_storage_summary, st.version ?: "", st.leafCount, formatBytes(st.totalBytes))
                    } else {
                        stringResource(R.string.mwm_maps_none)
                    },
                    modifier = Modifier.testTag("mwm_storage_summary"),
                )
                st.message?.let { Text(it, style = MaterialTheme.typography.bodySmall, modifier = Modifier.testTag("mwm_storage_message")) }
                if (dl.running) {
                    LinearProgressIndicator(progress = { (dl.percent ?: 0) / 100f }, modifier = Modifier.fillMaxWidth())
                    Text("${dl.label}: ${dl.message} ${dl.percent?.let { "$it %" } ?: ""}".trim(), style = MaterialTheme.typography.bodySmall)
                }
                LazyColumn(Modifier.weight(1f).fillMaxWidth()) {
                    val persisted = mig.persisted
                    if (persisted != null && persisted.inProgress) {
                        item {
                            Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                                Text(
                                    stringResource(R.string.mwm_storage_migration_running, statusText(persisted.status, persisted.error)),
                                    modifier = Modifier.weight(1f),
                                )
                                OutlinedButton(onClick = { MwmMigration.showProgress(context) }, modifier = Modifier.testTag("btn_mwm_storage_open_migration")) {
                                    Text(stringResource(R.string.mwm_storage_open))
                                }
                            }
                        }
                    } else if (st.pendingOldRegions > 0) {
                        item {
                            OutlinedButton(
                                onClick = {
                                    MwmMigration.showStorage(false)
                                    MwmMigration.openOffer(context)
                                },
                                modifier = Modifier.padding(vertical = 4.dp).testTag("btn_mwm_storage_migrate"),
                            ) { Text(stringResource(R.string.mwm_storage_migrate)) }
                        }
                    }
                    val up = st.update
                    if (up != null && up.available) {
                        item {
                            Column(Modifier.fillMaxWidth().padding(vertical = 6.dp).testTag("mwm_storage_update")) {
                                Text(
                                    stringResource(R.string.mwm_storage_update_available, up.current, up.installed ?: "?", formatBytes(up.bytes)),
                                    fontWeight = FontWeight.Medium,
                                )
                                Button(onClick = { MwmStorage.update(context) }, enabled = !dl.running, modifier = Modifier.testTag("btn_mwm_storage_update")) {
                                    Text(stringResource(R.string.mwm_storage_update))
                                }
                            }
                        }
                    }
                    for (g in st.groups) {
                        item(key = "g:" + g.id) {
                            Column(Modifier.fillMaxWidth().testTag("mwm_storage_group_" + g.id)) {
                                HorizontalDivider(Modifier.padding(vertical = 6.dp))
                                Text("${g.name} (${formatBytes(g.bytes)})", style = MaterialTheme.typography.titleSmall)
                            }
                        }
                        for (l in g.leaves) {
                            item(key = "l:" + l.id) {
                                Row(Modifier.fillMaxWidth().testTag("mwm_storage_leaf_" + l.id), verticalAlignment = Alignment.CenterVertically) {
                                    Column(Modifier.weight(1f)) {
                                        Text("${l.name} (${formatBytes(l.bytes)})")
                                        if (!l.hasSidecar) {
                                            Text(stringResource(R.string.mwm_storage_no_sidecar), style = MaterialTheme.typography.bodySmall)
                                        }
                                    }
                                    TextButton(onClick = { confirmLeaf = l }, modifier = Modifier.testTag("btn_mwm_storage_delete_" + l.id)) {
                                        Text(stringResource(R.string.mwm_storage_delete))
                                    }
                                }
                            }
                        }
                    }
                    if (st.worldBytes > 0) {
                        item {
                            HorizontalDivider(Modifier.padding(vertical = 6.dp))
                            Text(stringResource(R.string.mwm_storage_world, formatBytes(st.worldBytes)))
                        }
                    }
                    if (st.stagingBytes > 0) {
                        item { Text(stringResource(R.string.mwm_storage_staging, formatBytes(st.stagingBytes)), modifier = Modifier.testTag("mwm_storage_staging")) }
                    }
                    if (st.otherReleaseBytes > 0) {
                        item {
                            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                                Text(stringResource(R.string.mwm_storage_old_releases, formatBytes(st.otherReleaseBytes)), Modifier.weight(1f))
                                TextButton(onClick = { confirmReleases = true }, modifier = Modifier.testTag("btn_mwm_storage_delete_releases")) {
                                    Text(stringResource(R.string.mwm_storage_delete))
                                }
                            }
                        }
                    }
                    if (st.oldRegions.isNotEmpty()) {
                        item {
                            HorizontalDivider(Modifier.padding(vertical = 6.dp))
                            Text(stringResource(R.string.mwm_storage_old_maps), style = MaterialTheme.typography.titleSmall)
                            Text(stringResource(R.string.mwm_storage_old_maps_hint), style = MaterialTheme.typography.bodySmall)
                        }
                        for (r in st.oldRegions) {
                            item(key = "o:" + r.regionId) {
                                Row(Modifier.fillMaxWidth().testTag("mwm_storage_old_" + r.regionId.substringAfterLast('/')), verticalAlignment = Alignment.CenterVertically) {
                                    Text("${r.regionId.substringAfterLast('/').replaceFirstChar { c -> c.uppercase() }} (${formatBytes(r.oldBytes)})", Modifier.weight(1f))
                                    TextButton(onClick = { confirmOld = r }, modifier = Modifier.testTag("btn_mwm_storage_delete_old_" + r.regionId.substringAfterLast('/'))) {
                                        Text(stringResource(R.string.mwm_storage_delete))
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    confirmLeaf?.let { l ->
        AlertDialog(
            onDismissRequest = { confirmLeaf = null },
            title = { Text(stringResource(R.string.mwm_storage_delete_title, l.name)) },
            text = { Text(stringResource(R.string.mwm_storage_delete_body, formatBytes(l.bytes))) },
            confirmButton = {
                TextButton(onClick = {
                    confirmLeaf = null
                    MwmStorage.deleteLeaf(context, l.id)
                }, modifier = Modifier.testTag("btn_mwm_storage_delete_confirm")) { Text(stringResource(R.string.mwm_storage_delete)) }
            },
            dismissButton = { TextButton(onClick = { confirmLeaf = null }) { Text(stringResource(R.string.action_cancel)) } },
        )
    }
    if (confirmReleases) {
        AlertDialog(
            onDismissRequest = { confirmReleases = false },
            title = { Text(stringResource(R.string.mwm_storage_old_releases_title)) },
            text = { Text(stringResource(R.string.mwm_storage_old_releases_body, formatBytes(st.otherReleaseBytes))) },
            confirmButton = {
                TextButton(onClick = {
                    confirmReleases = false
                    MwmStorage.deleteOldReleases(context)
                }) { Text(stringResource(R.string.mwm_storage_delete)) }
            },
            dismissButton = { TextButton(onClick = { confirmReleases = false }) { Text(stringResource(R.string.action_cancel)) } },
        )
    }
    confirmOld?.let { r ->
        AlertDialog(
            onDismissRequest = { confirmOld = null },
            title = { Text(stringResource(R.string.mwm_storage_delete_title, r.regionId.substringAfterLast('/'))) },
            text = { Text(stringResource(R.string.mwm_storage_delete_old_body, formatBytes(r.oldBytes))) },
            confirmButton = {
                TextButton(onClick = {
                    confirmOld = null
                    MwmStorage.deleteOldRegion(context, r.regionId)
                }, modifier = Modifier.testTag("btn_mwm_storage_delete_old_confirm")) { Text(stringResource(R.string.mwm_storage_delete)) }
            },
            dismissButton = { TextButton(onClick = { confirmOld = null }) { Text(stringResource(R.string.action_cancel)) } },
        )
    }
}
