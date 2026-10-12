package no.navi.app

/**
 * Policy for when off-route auto-replan may replace From with the live GPS fix.
 *
 * Typed / picked place starts must stay sticky regardless of distance to GPS.
 * Only empty From or an explicit GPS / "My position" start may GPS-follow
 * (preserving the [ROUTE_START_AWAY_FROM_GPS_M] "not yet joined" guard for
 * those GPS-origin plans).
 */
object RouteStartGpsPolicy {
    /**
     * True when auto-replan from the live fix is allowed for this From origin.
     */
    fun shouldAutoReplanFromGps(fromFollowsGps: Boolean): Boolean = fromFollowsGps

    /**
     * Value for the "route not joined" busy bit that suppresses
     * [OffRouteCoordinator] AutoReroute.
     *
     * - Typed/picked From: always suppress (sticky From).
     * - GPS-follow From: suppress while there is no fix or start is farther
     *   than [awayThresholdM] from GPS (c44123f intent).
     */
    fun routeNotJoinedForAutoReroute(
        fromFollowsGps: Boolean,
        hasGpsFix: Boolean,
        startToGpsM: Double,
        awayThresholdM: Double,
    ): Boolean {
        if (!shouldAutoReplanFromGps(fromFollowsGps)) return true
        return !hasGpsFix || startToGpsM > awayThresholdM
    }
}
