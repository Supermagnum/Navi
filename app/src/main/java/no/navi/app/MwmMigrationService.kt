package no.navi.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import org.json.JSONObject
import uniffi.navi.defaultPackServerBaseUrl
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Runs the pinned map migration in the background (foreground service, data-sync type).
 *
 * State lives in Rust (`mwm/migration-cache/state.json`); this service only decides when a run
 * may start:
 * - Wi-Fi only on and the active network is metered or absent -> status `waiting_wifi`, resumes
 *   when an unmetered network comes back;
 * - no network, or a run that ended with a network error -> status `waiting_network`, retried
 *   when a network is available (the staging tree keeps partial files, so a retry continues);
 * - user pause -> status `paused`, nothing runs until [ACTION_RESUME];
 * - the process was killed -> the next app start (or a sticky restart) calls [ACTION_RESUME].
 */
class MwmMigrationService : Service() {
    private val worker = AtomicBoolean(false)

    @Volatile private var userPaused = false

    @Volatile private var stopRequested = false

    private val lock = Object()
    private var callback: ConnectivityManager.NetworkCallback? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(
        intent: Intent?,
        flags: Int,
        startId: Int,
    ): Int {
        val action = intent?.action ?: ACTION_RESUME
        Log.i(TAG, "onStartCommand action=$action")
        val app = applicationContext
        when (action) {
            ACTION_PAUSE -> {
                userPaused = true
                uniffi.navi.mwmMigrationPause()
                uniffi.navi.mwmMigrationSetStatus(MwmMigration.dataDir(app).absolutePath, "paused", "")
                MwmMigration.refreshProgress(app)
                synchronized(lock) { lock.notifyAll() }
                if (!worker.get()) stopSelf()
                return START_NOT_STICKY
            }
            ACTION_CANCEL -> {
                stopRequested = true
                uniffi.navi.mwmMigrationPause()
                synchronized(lock) { lock.notifyAll() }
                // Wait for the run to stop (it keeps files consistent), then forget the migration.
                Thread {
                    var waited = 0
                    while (uniffi.navi.mwmMigrationRunning() && waited < 100) {
                        Thread.sleep(100)
                        waited++
                    }
                    uniffi.navi.mwmMigrationDiscard(MwmMigration.dataDir(app).absolutePath)
                    MwmMigration.refreshProgress(app)
                    stopSelf()
                }.start()
                return START_NOT_STICKY
            }
            ACTION_POKE -> {
                synchronized(lock) { lock.notifyAll() }
                return START_STICKY
            }
            ACTION_START, ACTION_RESUME -> {
                val st = MwmMigration.readPersisted(app)
                if (st == null || st.status in setOf("verified", "done")) {
                    stopSelf()
                    return START_NOT_STICKY
                }
                // A sticky restart (null intent) never revives a migration the user paused or
                // that ended in an error; only an explicit tap or an app start does.
                if (intent == null && st.status in setOf("paused", "failed")) {
                    stopSelf()
                    return START_NOT_STICKY
                }
                userPaused = false
                stopRequested = false
                // An explicit resume / retry (a tap, or the app start) lifts the user's pause or the error.
                if (st.status in setOf("paused", "failed", "waiting_wifi", "waiting_network")) {
                    uniffi.navi.mwmMigrationSetStatus(MwmMigration.dataDir(app).absolutePath, "queued", "")
                }
                if (!startForegroundSafely(initialNotification(st.leaves.size))) {
                    // Android refused a background foreground-service start; the next app launch resumes.
                    stopSelf()
                    return START_NOT_STICKY
                }
                registerNetworkWatcher()
                if (worker.compareAndSet(false, true)) {
                    Thread({ loop(app) }, "mwm-migration-run").apply { isDaemon = true }.start()
                } else {
                    synchronized(lock) { lock.notifyAll() }
                }
                return START_STICKY
            }
        }
        return START_NOT_STICKY
    }

    private fun startForegroundSafely(n: Notification): Boolean =
        try {
            if (Build.VERSION.SDK_INT >= 29) {
                startForeground(NOTIF_ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
            } else {
                startForeground(NOTIF_ID, n)
            }
            true
        } catch (t: Throwable) {
            Log.w(TAG, "startForeground refused: ${t.message}")
            false
        }

    private fun online(cm: ConnectivityManager): Boolean {
        val caps = cm.getNetworkCapabilities(cm.activeNetwork ?: return false) ?: return false
        return caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) &&
            caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
    }

    private fun unmetered(cm: ConnectivityManager): Boolean {
        val caps = cm.getNetworkCapabilities(cm.activeNetwork ?: return false) ?: return false
        return online(cm) && caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
    }

    private fun registerNetworkWatcher() {
        if (callback != null) return
        val cm = getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
        val cb =
            object : ConnectivityManager.NetworkCallback() {
                override fun onAvailable(network: Network) = wake()

                override fun onLost(network: Network) = wake()

                override fun onCapabilitiesChanged(
                    network: Network,
                    caps: NetworkCapabilities,
                ) = wake()

                private fun wake() {
                    // Wi-Fi only: leaving Wi-Fi stops a running download within a quarter second.
                    val app = applicationContext
                    val st = MwmMigration.readPersisted(app)
                    if (st?.wifiOnly == true && st.status == "running" && !unmetered(cm)) {
                        Log.i(TAG, "left unmetered network: pausing for Wi-Fi")
                        uniffi.navi.mwmMigrationPause()
                    }
                    synchronized(lock) { lock.notifyAll() }
                }
            }
        runCatching { cm.registerDefaultNetworkCallback(cb) }
            .onSuccess { callback = cb }
            .onFailure { Log.w(TAG, "network callback failed: ${it.message}") }
    }

    private fun waitForWake(ms: Long) {
        synchronized(lock) { runCatching { lock.wait(ms) } }
    }

    private fun loop(app: Context) {
        val cm = getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
        val dir = MwmMigration.dataDir(app).absolutePath
        var ticker: Thread? = null
        try {
            ticker =
                Thread {
                    while (!Thread.currentThread().isInterrupted) {
                        MwmMigration.refreshProgress(app)
                        updateNotification(app)
                        try {
                            Thread.sleep(1500)
                        } catch (_: InterruptedException) {
                            break
                        }
                    }
                }.apply {
                    isDaemon = true
                    start()
                }
            while (!stopRequested) {
                val st = MwmMigration.readPersisted(app) ?: break
                if (st.status in setOf("verified", "done")) break
                if (userPaused || st.status == "paused") {
                    Log.i(TAG, "paused by user")
                    break
                }
                if (st.wifiOnly && !unmetered(cm)) {
                    if (online(cm)) {
                        Log.i(TAG, "waiting for Wi-Fi (wifi_only, active network is metered)")
                    } else {
                        Log.i(TAG, "waiting for Wi-Fi (no network)")
                    }
                    uniffi.navi.mwmMigrationSetStatus(dir, "waiting_wifi", "")
                    waitForWake(15_000)
                    continue
                }
                if (!online(cm)) {
                    Log.i(TAG, "no network, waiting")
                    uniffi.navi.mwmMigrationSetStatus(dir, "waiting_network", "")
                    waitForWake(15_000)
                    continue
                }
                Log.i(TAG, "run start status=${st.status} leaves=${st.leaves.size}")
                val res =
                    runCatching { JSONObject(uniffi.navi.mwmMigrationRun(dir, defaultPackServerBaseUrl())) }
                        .getOrElse { JSONObject().put("kind", "failed").put("message", it.message ?: "run threw") }
                val kind = res.optString("kind")
                Log.i(TAG, "run end kind=$kind message=${res.optString("message")}")
                when (kind) {
                    "verified" -> {
                        MwmMigration.onVerified(app)
                        notifyDone(app)
                        break
                    }
                    "paused" -> {
                        if (userPaused || stopRequested) break
                        // Stopped by the Wi-Fi watcher: stay queued and wait for Wi-Fi.
                        uniffi.navi.mwmMigrationSetStatus(dir, "waiting_wifi", "")
                        waitForWake(15_000)
                    }
                    "waiting_network" -> waitForWake(15_000)
                    else -> break // no_space / failed: the state keeps the error for the screen
                }
            }
        } finally {
            ticker?.interrupt()
            MwmMigration.refreshProgress(app)
            worker.set(false)
            callback?.let { cb ->
                runCatching {
                    (getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager).unregisterNetworkCallback(cb)
                }
            }
            callback = null
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
        }
    }

    private fun channel(): String {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (Build.VERSION.SDK_INT >= 26 && nm.getNotificationChannel(CHANNEL) == null) {
            nm.createNotificationChannel(
                NotificationChannel(CHANNEL, getString(R.string.mwm_mig_channel), NotificationManager.IMPORTANCE_LOW),
            )
        }
        return CHANNEL
    }

    private fun contentIntent(): PendingIntent =
        PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

    private fun initialNotification(leaves: Int): Notification =
        NotificationCompat
            .Builder(this, channel())
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setContentTitle(getString(R.string.mwm_mig_notif_title))
            .setContentText(getString(R.string.mwm_mig_notif_starting, leaves))
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setContentIntent(contentIntent())
            .build()

    private fun updateNotification(app: Context) {
        val st = MwmMigration.readPersisted(app) ?: return
        val p = MwmMigration.ui.progress
        val text =
            when (st.status) {
                "waiting_wifi" -> getString(R.string.mwm_mig_status_waiting_wifi)
                "waiting_network" -> getString(R.string.mwm_mig_status_waiting_network)
                else -> getString(R.string.mwm_mig_notif_progress, formatBytes(p.done), formatBytes(p.total))
            }
        val b =
            NotificationCompat
                .Builder(this, channel())
                .setSmallIcon(android.R.drawable.stat_sys_download)
                .setContentTitle(getString(R.string.mwm_mig_notif_title))
                .setContentText(text)
                .setOngoing(true)
                .setOnlyAlertOnce(true)
                .setContentIntent(contentIntent())
        if (p.total > 0) b.setProgress(100, ((p.done * 100) / p.total).toInt().coerceIn(0, 100), false)
        (getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager).notify(NOTIF_ID, b.build())
    }

    private fun notifyDone(app: Context) {
        val n =
            NotificationCompat
                .Builder(this, channel())
                .setSmallIcon(android.R.drawable.stat_sys_download_done)
                .setContentTitle(getString(R.string.mwm_mig_notif_title))
                .setContentText(getString(R.string.mwm_mig_notif_done))
                .setAutoCancel(true)
                .setContentIntent(contentIntent())
                .build()
        (app.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager).notify(NOTIF_ID_DONE, n)
    }

    override fun onDestroy() {
        callback?.let { cb ->
            runCatching {
                (getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager).unregisterNetworkCallback(cb)
            }
        }
        callback = null
        super.onDestroy()
    }

    companion object {
        private const val TAG = "MwmMigrationSvc"
        private const val CHANNEL = "mwm_migration"
        private const val NOTIF_ID = 7107
        private const val NOTIF_ID_DONE = 7108
        const val ACTION_START = "no.navi.app.MWM_MIGRATION_START"
        const val ACTION_RESUME = "no.navi.app.MWM_MIGRATION_RESUME"
        const val ACTION_PAUSE = "no.navi.app.MWM_MIGRATION_PAUSE"
        const val ACTION_CANCEL = "no.navi.app.MWM_MIGRATION_CANCEL"
        const val ACTION_POKE = "no.navi.app.MWM_MIGRATION_POKE"

        fun start(
            context: Context,
            action: String,
        ) {
            val i = Intent(context, MwmMigrationService::class.java).setAction(action)
            runCatching {
                if (action == ACTION_PAUSE || action == ACTION_CANCEL || action == ACTION_POKE) {
                    context.startService(i)
                } else if (Build.VERSION.SDK_INT >= 26) {
                    context.startForegroundService(i)
                } else {
                    context.startService(i)
                }
            }.onFailure { Log.w(TAG, "start $action refused: ${it.message}") }
        }

        fun poke(context: Context) = start(context, ACTION_POKE)
    }
}

internal fun formatBytes(bytes: Long): String =
    when {
        bytes >= 1_000_000_000L -> "%.2f GB".format(java.util.Locale.US, bytes / 1_000_000_000.0)
        bytes >= 1_000_000L -> "%.0f MB".format(java.util.Locale.US, bytes / 1_000_000.0)
        bytes >= 1_000L -> "%.0f kB".format(java.util.Locale.US, bytes / 1_000.0)
        else -> "$bytes B"
    }
