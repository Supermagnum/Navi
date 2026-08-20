package no.navi.app

/**
 * Bottom-HUD overspeed chrome: when reported GPS speed exceeds the applicable
 * posted/conditional limit by more than [MARGIN_KMH] (or the fix's own speed
 * accuracy, whichever is larger), the speed line uses the error color.
 *
 * Display-only — not an alert escalation or enforcement path.
 */
object OverspeedHud {
    /**
     * Minimum positive delta (km/h) before painting overspeed.
     *
     * **Not** a float-epsilon. The first ship used `+0.5` as an arbitrary
     * "barely over" check with no real-GPS tuning; that is tighter than typical
     * consumer GNSS speed noise (Android `getSpeedAccuracyMetersPerSecond` is a
     * 68% band often ~0.5-1+ m/s (~ 1.8-3.6+ km/h); open-sky Doppler can be better,
     * multipath / weak sky worse). A `+0.5` margin therefore flickers when the
     * vehicle is legally at/under the limit and the fix jitters.
     *
     * Chosen floor: **3.0 km/h** (~0.83 m/s) — above common open-sky noise /
     * reported accuracy bands while still a small, readable HUD cue. When the
     * fix reports [Location.hasSpeedAccuracy], the effective margin is
     * `max(MARGIN_KMH, speedAccuracyKmh)` so poor reception widens the gate.
     *
     * **Validation:** [SimOverspeedInstrumentedTest] exercises overspeed on
     * routed roads via the built-in simulator (legal speed vs injected over-limit
     * fixes). Outdoor live-GPS noise sizing ([GpsSpeedNoiseInstrumentedTest]) is
     * optional and omitted when no sky lock is available.
     */
    const val MARGIN_KMH: Double = 3.0

    /**
     * @param speedAccuracyKmh optional `Location.speedAccuracyMetersPerSecond * 3.6`
     *   when [android.location.Location.hasSpeedAccuracy] is true.
     */
    fun isOverspeed(
        speedKmh: Double?,
        limitKmh: Double?,
        speedAccuracyKmh: Double? = null,
    ): Boolean {
        val s = speedKmh?.takeIf { it.isFinite() } ?: return false
        val lim = limitKmh?.takeIf { it.isFinite() && it > 0.0 } ?: return false
        val margin =
            maxOf(
                MARGIN_KMH,
                speedAccuracyKmh?.takeIf { it.isFinite() && it > 0.0 } ?: 0.0,
            )
        return (s - lim) > margin
    }
}
