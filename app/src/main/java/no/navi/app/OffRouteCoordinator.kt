package no.navi.app

/**
 * Debounces cross-track off-route into a confirmed action.
 *
 * Profile policy (documented):
 * - **Motor** (Car / Truck / …): after [confirmAfterMs] of sustained off-route,
 *   automatically recompute from current GPS to remaining destination/vias.
 * - **Hiking**: after the same debounce, **prompt** first — leaving a trail is
 *   often intentional (allemannsretten, rest, deliberate off-trail). The user
 *   can accept a new plan or keep the original corridor.
 *
 * Declining / cancelling sets [suppressedUntilOnRoute] so we do not nag until
 * the fix returns within the cross-track threshold again.
 */
class OffRouteCoordinator(
    private val confirmAfterMs: Long = RouteProgressTracker.OFF_ROUTE_CONFIRM_MS,
) {
    enum class Action {
        None,

        /** Instant UI: approach box shows Off route (before debounce fires). */
        ShowOffRouteUi,

        /** Motor: start automatic replan. */
        AutoReroute,

        /** Hiking: show accept/decline dialog. */
        PromptHikingReroute,
    }

    private var offSinceMs: Long? = null
    private var confirmedEmitted = false

    /** After decline/cancel, ignore until back on route. */
    var suppressedUntilOnRoute: Boolean = false
        private set

    fun reset() {
        offSinceMs = null
        confirmedEmitted = false
        suppressedUntilOnRoute = false
    }

    fun suppressUntilOnRoute() {
        suppressedUntilOnRoute = true
        offSinceMs = null
        confirmedEmitted = false
    }

    /**
     * @param offRoute current cross-track off-route flag
     * @param hiking true → prompt policy; false → auto-reroute
     * @param busy already planning/rerouting — do not re-fire
     */
    fun onFix(
        offRoute: Boolean,
        hiking: Boolean,
        busy: Boolean,
        nowMs: Long = System.currentTimeMillis(),
        confirmMs: Long = confirmAfterMs,
    ): Action {
        if (!offRoute) {
            offSinceMs = null
            confirmedEmitted = false
            if (suppressedUntilOnRoute) {
                suppressedUntilOnRoute = false
            }
            return Action.None
        }
        if (suppressedUntilOnRoute || busy) {
            return Action.ShowOffRouteUi
        }
        val since = offSinceMs ?: nowMs.also { offSinceMs = it }
        if (nowMs - since < confirmMs) {
            return Action.ShowOffRouteUi
        }
        if (confirmedEmitted) {
            return Action.ShowOffRouteUi
        }
        confirmedEmitted = true
        return if (hiking) Action.PromptHikingReroute else Action.AutoReroute
    }
}
