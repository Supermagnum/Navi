package no.navi.app

import java.util.concurrent.atomic.AtomicBoolean

/**
 * Process-wide single-flight for native route planning. A second Plan (or a
 * second Activity after `am start`) must not start another `plan_car_route`.
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

    fun end() {
        running.set(false)
        IdlePackJobs.onPlanEnded()
    }
}
