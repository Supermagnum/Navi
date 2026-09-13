package no.navi.app

import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.navi.datexRefreshJson
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Process-lifetime DATEX refresh coordination for the first plan of a session.
 *
 * Steady-state polling (routeSamples + ~300s) stays in MainActivity; this only
 * covers the gap before the first motor/bike plan when the plugin is on.
 */
object DatexSessionRefresh {
    /**
     * Bound for waiting on a network refresh before the first plan.
     *
     * Matches pack-server / DATEX connectivity connect+probe timeout (3s).
     * A healthy navi-server typically answers source.json (+ often XML) within
     * that window; slower paths fall through to disk cache (max-age) or empty.
     */
    const val PRE_PLAN_TIMEOUT_MS: Long = 3_000L

    private const val TAG = "NaviDatex"

    private val refreshScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val mutex = Mutex()
    private var inFlight: Deferred<String>? = null

    /** Set after the first plugin-on pre-plan attempt (success or timeout). */
    private val firstPlanWarmupDone = AtomicBoolean(false)

    /** Test hook: replace UniFFI refresh. Cleared by [resetForTests]. */
    @Volatile
    var refreshOverride: (suspend () -> String)? = null

    fun resetForTests() {
        firstPlanWarmupDone.set(false)
        refreshOverride = null
        inFlight?.cancel()
        inFlight = null
    }

    fun firstPlanWarmupCompleted(): Boolean = firstPlanWarmupDone.get()

    sealed class PrePlanOutcome {
        data object SkippedPluginOff : PrePlanOutcome()

        data object SkippedAlreadyDone : PrePlanOutcome()

        data object SkippedNotApplicable : PrePlanOutcome()

        data class Refreshed(
            val rawJson: String,
        ) : PrePlanOutcome()

        data object TimedOut : PrePlanOutcome()
    }

    /**
     * Single-flight [datexRefreshJson]. Concurrent callers share one Deferred so
     * pre-plan warmup and the routeSamples poll cannot double-fetch.
     *
     * The Deferred outlives a timed-out waiter so a slow fetch can still fill
     * the disk cache for later plans / the steady-state poll.
     */
    suspend fun refreshShared(
        enabled: Boolean,
        host: String,
        port: UInt,
        routeLatLonJson: String,
        wifiOnly: Boolean,
        onWifi: Boolean,
        useDiscoveryChain: Boolean = true,
        cacheDir: String?,
    ): String {
        val deferred =
            mutex.withLock {
                val active = inFlight
                if (active != null && active.isActive) {
                    active
                } else {
                    lateinit var created: Deferred<String>
                    created =
                        refreshScope.async {
                            try {
                                refreshOverride?.invoke()
                                    ?: datexRefreshJson(
                                        enabled = enabled,
                                        host = host,
                                        port = port,
                                        routeLatLonJson = routeLatLonJson,
                                        wifiOnly = wifiOnly,
                                        onWifi = onWifi,
                                        useDiscoveryChain = useDiscoveryChain,
                                        cacheDir = cacheDir,
                                    )
                            } finally {
                                mutex.withLock {
                                    if (inFlight === created) {
                                        inFlight = null
                                    }
                                }
                            }
                        }
                    inFlight = created
                    created
                }
            }
        return deferred.await()
    }

    /**
     * Before the first motor/bike plan of this process: wait up to
     * [PRE_PLAN_TIMEOUT_MS] for a DATEX refresh when the plugin is enabled.
     *
     * Marks warmup done on attempt (including timeout) so later plans skip this
     * path. Timed-out work may still finish in the background via [refreshShared].
     */
    suspend fun ensureBeforeFirstPlan(
        pluginEnabled: Boolean,
        appliesToProfile: Boolean,
        host: String,
        port: UInt,
        routeLatLonJson: String,
        wifiOnly: Boolean,
        onWifi: Boolean,
        cacheDir: String?,
        onWaiting: () -> Unit = {},
    ): PrePlanOutcome {
        if (!pluginEnabled) {
            return PrePlanOutcome.SkippedPluginOff
        }
        if (!appliesToProfile) {
            return PrePlanOutcome.SkippedNotApplicable
        }
        if (!firstPlanWarmupDone.compareAndSet(false, true)) {
            return PrePlanOutcome.SkippedAlreadyDone
        }
        onWaiting()
        runCatching {
            Log.i(TAG, "pre-plan DATEX refresh (timeout=${PRE_PLAN_TIMEOUT_MS}ms)")
        }
        val raw =
            withTimeoutOrNull(PRE_PLAN_TIMEOUT_MS) {
                refreshShared(
                    enabled = true,
                    host = host,
                    port = port,
                    routeLatLonJson = routeLatLonJson,
                    wifiOnly = wifiOnly,
                    onWifi = onWifi,
                    cacheDir = cacheDir,
                )
            }
        return if (raw != null) {
            runCatching { Log.i(TAG, "pre-plan DATEX refresh completed") }
            PrePlanOutcome.Refreshed(raw)
        } else {
            runCatching {
                Log.i(TAG, "pre-plan DATEX refresh timed out; planning with cache/max-age fallback")
            }
            PrePlanOutcome.TimedOut
        }
    }
}
