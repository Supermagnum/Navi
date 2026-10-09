package no.navi.app

import java.util.concurrent.atomic.AtomicBoolean

/**
 * Process-wide single-flight for native route planning. A second Plan cancels
 * the one in flight, waits for it to release, then starts.
 */
object RoutePlanGate {
    private val running = AtomicBoolean(false)

    fun isRunning(): Boolean = running.get()

    fun tryBegin(): Boolean {
        if (!running.compareAndSet(false, true)) return false
        val pause = IdlePackJobs.waitOrPauseForPlan()
        RoutingPlanLog.idleJobPause(pause)
        return true
    }

    /**
     * Take the gate, cancelling a running plan first. [cancelRunning] must ask
     * the native planner to stop; this waits until [end] is called.
     */
    fun tryBeginOrReplace(cancelRunning: () -> Unit, timeoutMs: Long = 120_000L): Boolean {
        if (tryBegin()) return true
        cancelRunning()
        val deadline = System.nanoTime() + timeoutMs * 1_000_000L
        while (running.get() && System.nanoTime() < deadline) {
            Thread.sleep(20)
        }
        return tryBegin()
    }

    fun end() {
        running.set(false)
        IdlePackJobs.onPlanEnded()
    }
}
