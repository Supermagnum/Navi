package no.navi.app

import android.Manifest
import android.app.Activity
import android.content.pm.PackageManager
import android.os.Build
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
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
import androidx.core.content.ContextCompat
import kotlinx.coroutines.delay

/**
 * Root of all Phase 7 screens (offer, progress, old-maps question, Map storage). Placed once in
 * the map screen; shows nothing unless the MWM path is active and something needs the user.
 */
@Composable
internal fun MwmMigrationHost() {
    if (!MwmClientBootstrap.isEnabled()) return
    val context = LocalContext.current
    val ui = MwmMigration.ui
    LaunchedEffect(Unit) { MwmMigration.onAppStart(context) }
    LaunchedEffect(ui.progressVisible || ui.storageVisible) {
        while (ui.progressVisible || ui.storageVisible) {
            MwmMigration.refreshProgress(context)
            delay(1000)
        }
    }
    when {
        ui.offerVisible -> MwmMigrationOfferScreen(ui)
        ui.progressVisible && ui.persisted != null -> MwmMigrationProgressScreen(ui)
    }
    ui.done?.let { MwmMigrationDoneDialog(it, ui.persisted) }
    if (ui.storageVisible) MwmMapStorageScreen()
}

@Composable
private fun FullScreen(
    onBack: () -> Unit,
    tag: String,
    content: @Composable () -> Unit,
) {
    Dialog(
        onDismissRequest = onBack,
        properties =
            DialogProperties(
                usePlatformDefaultWidth = false,
                dismissOnClickOutside = false,
            ),
    ) {
        Surface(modifier = Modifier.fillMaxSize().testTag(tag)) { content() }
    }
}

@Composable
private fun Heading(text: String) {
    Text(text, style = MaterialTheme.typography.titleMedium, modifier = Modifier.padding(top = 12.dp, bottom = 4.dp))
}

@Composable
internal fun MwmMigrationOfferScreen(ui: MwmMigration.UiState) {
    val context = LocalContext.current
    val plan = ui.plan
    FullScreen(onBack = { MwmMigration.later(context) }, tag = "mwm_migration_offer") {
        Column(Modifier.fillMaxSize().padding(16.dp)) {
            Text(
                stringResource(R.string.mwm_mig_title),
                style = MaterialTheme.typography.headlineSmall,
                modifier = Modifier.testTag("mwm_mig_title"),
            )
            if (ui.detected.isEmpty()) {
                Text(stringResource(R.string.mwm_mig_nothing), modifier = Modifier.padding(top = 12.dp))
            }
            LazyColumn(Modifier.weight(1f).fillMaxWidth()) {
                item { Text(stringResource(R.string.mwm_mig_intro), modifier = Modifier.padding(top = 8.dp)) }
                if (ui.planLoading && plan == null) {
                    item { Text(stringResource(R.string.mwm_mig_planning), modifier = Modifier.padding(top = 12.dp)) }
                }
                ui.planError?.let { err ->
                    item {
                        Text(
                            stringResource(R.string.mwm_mig_plan_error, err),
                            color = MaterialTheme.colorScheme.error,
                            modifier = Modifier.padding(top = 12.dp).testTag("mwm_mig_plan_error"),
                        )
                    }
                }
                if (plan != null) {
                    item { Heading(stringResource(R.string.mwm_mig_now_have)) }
                    items(plan.rows, key = { "r:" + it.regionId }) { r ->
                        Row(
                            Modifier.fillMaxWidth().testTag("mwm_mig_region_" + r.regionId.substringAfterLast('/')),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            if (ui.chooseMode && r.mapped) {
                                Checkbox(checked = r.selected, onCheckedChange = { MwmMigration.toggleRegion(context, r.regionId) })
                            }
                            Column(Modifier.weight(1f)) {
                                Text("${r.displayName} (${formatBytes(r.oldBytes)})", fontWeight = FontWeight.Medium)
                                when {
                                    !r.mapped ->
                                        Text(
                                            stringResource(R.string.mwm_mig_unmapped),
                                            style = MaterialTheme.typography.bodySmall,
                                            color = MaterialTheme.colorScheme.error,
                                        )
                                    !r.selected ->
                                        Text(
                                            stringResource(R.string.mwm_mig_not_selected),
                                            style = MaterialTheme.typography.bodySmall,
                                        )
                                    else ->
                                        Text(
                                            stringResource(R.string.mwm_mig_region_areas, r.leafIds.size),
                                            style = MaterialTheme.typography.bodySmall,
                                        )
                                }
                            }
                        }
                    }
                    item { Heading(stringResource(R.string.mwm_mig_will_get)) }
                    if (plan.leaves.isEmpty()) {
                        item { Text(stringResource(R.string.mwm_mig_no_areas)) }
                    }
                    items(plan.leaves, key = { "l:" + it.id }) { l ->
                        Row(Modifier.fillMaxWidth().testTag("mwm_mig_leaf_" + l.id), verticalAlignment = Alignment.CenterVertically) {
                            if (ui.chooseMode) {
                                Checkbox(checked = !l.excluded, onCheckedChange = { MwmMigration.toggleLeaf(context, l.id) })
                            }
                            Column(Modifier.weight(1f)) {
                                Text("${l.displayName} (${formatBytes(l.bytes)})")
                                val notes =
                                    buildList {
                                        if (l.sharedBy.size > 1) add(stringResource(R.string.mwm_mig_shared, l.sharedBy.size))
                                        if (l.installed) add(stringResource(R.string.mwm_mig_already))
                                        if (l.mostlyOutside) add(stringResource(R.string.mwm_mig_outside))
                                    }
                                if (notes.isNotEmpty()) {
                                    Text(notes.joinToString("; "), style = MaterialTheme.typography.bodySmall)
                                }
                            }
                        }
                    }
                    if (plan.unavailableLeaves.isNotEmpty()) {
                        item {
                            Text(
                                stringResource(R.string.mwm_mig_unavailable, plan.unavailableLeaves.joinToString(", ") { it.replace('_', ' ') }),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.error,
                            )
                        }
                    }
                    item { HorizontalDivider(Modifier.padding(vertical = 8.dp)) }
                    item {
                        Column(Modifier.testTag("mwm_mig_totals")) {
                            Text(stringResource(R.string.mwm_mig_download_size, formatBytes(plan.downloadBytes)), fontWeight = FontWeight.Medium)
                            if (plan.sharedSavedBytes > 0) {
                                Text(
                                    stringResource(R.string.mwm_mig_saved, formatBytes(plan.sharedSavedBytes)),
                                    style = MaterialTheme.typography.bodySmall,
                                )
                            }
                            Text(
                                stringResource(R.string.mwm_mig_space, formatBytes(plan.freeNeeded), formatBytes(plan.freeAvailable)),
                                modifier = Modifier.testTag("mwm_mig_space"),
                            )
                            if (!plan.enoughSpace) {
                                Text(
                                    stringResource(R.string.mwm_mig_space_short, formatBytes(plan.spaceShortBy)),
                                    color = MaterialTheme.colorScheme.error,
                                    modifier = Modifier.padding(top = 4.dp).testTag("mwm_mig_space_short"),
                                )
                            }
                            Text(
                                stringResource(R.string.mwm_mig_old_notice),
                                style = MaterialTheme.typography.bodySmall,
                                modifier = Modifier.padding(top = 8.dp).testTag("mwm_mig_old_notice"),
                            )
                            if (plan.oldBytesUnmapped > 0) {
                                Text(
                                    stringResource(R.string.mwm_mig_unmapped_total, formatBytes(plan.oldBytesUnmapped)),
                                    style = MaterialTheme.typography.bodySmall,
                                )
                            }
                        }
                    }
                    item {
                        Row(Modifier.fillMaxWidth().padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                            Column(Modifier.weight(1f)) {
                                Text(stringResource(R.string.mwm_mig_wifi_only))
                                if (plan.large) {
                                    Text(
                                        stringResource(R.string.mwm_mig_wifi_large, formatBytes(plan.downloadBytes)),
                                        style = MaterialTheme.typography.bodySmall,
                                    )
                                }
                            }
                            Switch(
                                checked = ui.wifiOnly,
                                onCheckedChange = { MwmMigration.setWifiOnly(context, it) },
                                modifier = Modifier.testTag("mwm_mig_wifi_only"),
                            )
                        }
                    }
                }
                ui.startError?.let { err ->
                    item { Text(err, color = MaterialTheme.colorScheme.error, modifier = Modifier.testTag("mwm_mig_start_error")) }
                }
            }
            Row(Modifier.fillMaxWidth().padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                val canStart = plan != null && plan.enoughSpace && plan.leaves.any { !it.excluded } && !ui.planLoading
                Button(
                    onClick = {
                        requestNotificationPermission(context)
                        MwmMigration.startNow(context)
                    },
                    enabled = canStart,
                    modifier = Modifier.weight(1f).testTag("btn_mwm_mig_now"),
                ) { Text(stringResource(R.string.mwm_mig_now)) }
                OutlinedButton(
                    onClick = { MwmMigration.setChooseMode(!ui.chooseMode) },
                    enabled = plan != null,
                    modifier = Modifier.weight(1f).testTag("btn_mwm_mig_choose"),
                ) { Text(stringResource(if (ui.chooseMode) R.string.mwm_mig_choose_done else R.string.mwm_mig_choose)) }
                TextButton(
                    onClick = { MwmMigration.later(context) },
                    modifier = Modifier.testTag("btn_mwm_mig_later"),
                ) { Text(stringResource(R.string.mwm_mig_later)) }
            }
        }
    }
}

internal fun requestNotificationPermission(context: android.content.Context) {
    if (Build.VERSION.SDK_INT < 33) return
    val act = context as? Activity ?: return
    if (ContextCompat.checkSelfPermission(act, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
        androidx.core.app.ActivityCompat
            .requestPermissions(act, arrayOf(Manifest.permission.POST_NOTIFICATIONS), 7107)
    }
}

@Composable
internal fun statusText(
    status: String,
    error: String?,
): String =
    when (status) {
        "queued" -> stringResource(R.string.mwm_mig_status_queued)
        "running" -> stringResource(R.string.mwm_mig_status_running)
        "paused" -> stringResource(R.string.mwm_mig_status_paused)
        "waiting_wifi" -> stringResource(R.string.mwm_mig_status_waiting_wifi)
        "waiting_network" -> stringResource(R.string.mwm_mig_status_waiting_network)
        "failed" ->
            if (error?.contains("insufficient space") == true) {
                stringResource(R.string.mwm_mig_status_no_space)
            } else {
                stringResource(R.string.mwm_mig_status_failed, error ?: "")
            }
        "verified" -> stringResource(R.string.mwm_mig_status_verified)
        else -> status
    }

@Composable
internal fun MwmMigrationProgressScreen(ui: MwmMigration.UiState) {
    val context = LocalContext.current
    val st = ui.persisted ?: return
    var confirmCancel by remember { mutableStateOf(false) }
    FullScreen(onBack = { MwmMigration.hideProgress() }, tag = "mwm_migration_progress") {
        Column(Modifier.fillMaxSize().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.mwm_mig_progress_title), style = MaterialTheme.typography.headlineSmall)
            Text(
                statusText(st.status, st.error),
                modifier = Modifier.testTag("mwm_mig_status"),
                color = if (st.status == "failed") MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface,
            )
            val p = ui.progress
            LinearProgressIndicator(
                progress = { if (p.total > 0) (p.done.toFloat() / p.total).coerceIn(0f, 1f) else 0f },
                modifier = Modifier.fillMaxWidth().testTag("mwm_mig_bar"),
            )
            Text(
                stringResource(R.string.mwm_mig_progress_bytes, formatBytes(p.done), formatBytes(p.total)),
                modifier = Modifier.testTag("mwm_mig_bytes"),
            )
            Text(stringResource(R.string.mwm_mig_progress_leaves, st.leaves.size), style = MaterialTheme.typography.bodySmall)
            Text(stringResource(R.string.mwm_mig_old_notice), style = MaterialTheme.typography.bodySmall)
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(stringResource(R.string.mwm_mig_wifi_only), Modifier.weight(1f))
                Switch(
                    checked = st.wifiOnly,
                    onCheckedChange = { MwmMigration.setWifiOnly(context, it) },
                    modifier = Modifier.testTag("mwm_mig_wifi_only"),
                )
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                val paused = st.status in setOf("paused", "failed")
                if (paused) {
                    Button(onClick = { MwmMigration.resume(context) }, modifier = Modifier.testTag("btn_mwm_mig_resume")) {
                        Text(stringResource(if (st.status == "failed") R.string.mwm_mig_retry else R.string.mwm_mig_resume))
                    }
                } else {
                    Button(onClick = { MwmMigration.pause(context) }, modifier = Modifier.testTag("btn_mwm_mig_pause")) {
                        Text(stringResource(R.string.mwm_mig_pause))
                    }
                }
                OutlinedButton(onClick = { MwmMigration.hideProgress() }, modifier = Modifier.testTag("btn_mwm_mig_background")) {
                    Text(stringResource(R.string.mwm_mig_background))
                }
                TextButton(onClick = { confirmCancel = true }, modifier = Modifier.testTag("btn_mwm_mig_cancel")) {
                    Text(stringResource(R.string.mwm_mig_cancel))
                }
            }
        }
    }
    if (confirmCancel) {
        AlertDialog(
            onDismissRequest = { confirmCancel = false },
            title = { Text(stringResource(R.string.mwm_mig_cancel_title)) },
            text = { Text(stringResource(R.string.mwm_mig_cancel_body)) },
            confirmButton = {
                TextButton(onClick = {
                    confirmCancel = false
                    MwmMigration.cancelMigration(context)
                }, modifier = Modifier.testTag("btn_mwm_mig_cancel_yes")) { Text(stringResource(R.string.mwm_mig_cancel_yes)) }
            },
            dismissButton = { TextButton(onClick = { confirmCancel = false }) { Text(stringResource(R.string.mwm_mig_cancel_no)) } },
        )
    }
}

/** The "delete the old maps?" question. Nothing is deleted without the tap, and only after verification. */
@Composable
internal fun MwmMigrationDoneDialog(
    info: MwmMigration.DoneInfo,
    persisted: MwmMigration.Persisted?,
) {
    val context = LocalContext.current
    AlertDialog(
        onDismissRequest = { if (info.result != null || !info.verified) MwmMigration.closeDone() },
        modifier = Modifier.testTag("mwm_mig_done_dialog"),
        title = { Text(stringResource(R.string.mwm_mig_done_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                when {
                    info.verifying -> Text(stringResource(R.string.mwm_mig_done_verifying))
                    !info.verified ->
                        Text(
                            stringResource(R.string.mwm_mig_done_verify_failed, info.error ?: ""),
                            color = MaterialTheme.colorScheme.error,
                        )
                    info.result == "deleted" -> Text(stringResource(R.string.mwm_mig_done_deleted, formatBytes(info.oldBytes)))
                    info.result == "kept" -> Text(stringResource(R.string.mwm_mig_done_kept))
                    info.result?.startsWith("partial:") == true ->
                        Text(stringResource(R.string.mwm_mig_done_partial, formatBytes(info.oldBytes), info.result.removePrefix("partial:")))
                    else -> {
                        Text(stringResource(R.string.mwm_mig_done_ready, persisted?.leaves?.size ?: 0))
                        Text(
                            stringResource(R.string.mwm_mig_done_ask, formatBytes(info.oldBytes)),
                            modifier = Modifier.testTag("mwm_mig_done_ask"),
                        )
                    }
                }
            }
        },
        confirmButton = {
            if (info.verified && info.result == null) {
                TextButton(onClick = { MwmMigration.deleteOldMaps(context) }, modifier = Modifier.testTag("btn_mwm_mig_delete_old")) {
                    Text(stringResource(R.string.mwm_mig_delete_old, formatBytes(info.oldBytes)))
                }
            } else if (!info.verifying) {
                TextButton(onClick = { MwmMigration.closeDone() }, modifier = Modifier.testTag("btn_mwm_mig_done_close")) {
                    Text(stringResource(R.string.mwm_mig_close))
                }
            }
        },
        dismissButton = {
            if (info.verified && info.result == null) {
                TextButton(onClick = { MwmMigration.keepOldMaps(context) }, modifier = Modifier.testTag("btn_mwm_mig_keep_old")) {
                    Text(stringResource(R.string.mwm_mig_keep_old))
                }
            }
        },
    )
}
