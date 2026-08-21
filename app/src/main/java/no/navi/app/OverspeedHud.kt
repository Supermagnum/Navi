package no.navi.app

/**
 * Bottom-HUD overspeed chrome: when reported GPS speed exceeds the applicable
 * posted/conditional limit by more than the hybrid margin below, the speed line
 * uses the error color.
 *
 * Display-only — not an alert escalation or enforcement path.
 */
object OverspeedHud {
    /**
     * Absolute floor (km/h) inside the hybrid margin.
     *
     * **Not** a float-epsilon. The first ship used `+0.5` as an arbitrary
     * "barely over" check with no real-GPS tuning; that is tighter than typical
     * consumer GNSS speed noise (Android `getSpeedAccuracyMetersPerSecond` is a
     * 68% band often ~0.5-1+ m/s (~ 1.8-3.6+ km/h); open-sky Doppler can be better,
     * multipath / weak sky worse). A `+0.5` margin therefore flickers when the
     * vehicle is legally at/under the limit and the fix jitters.
     *
     * Chosen floor: **3.0 km/h** (~0.83 m/s) — above common open-sky noise /
     * reported accuracy bands while still a small, readable HUD cue. Effective
     * margin is the hybrid
     * `max(limit × 0.05, speedAccuracyKmh, MARGIN_KMH)`:
     * - **5% of the posted limit** so higher-speed roads get a proportional gate
     *   (at 80 km/h that is 4 km/h; at 110 km/h, 5.5 km/h);
     * - **GNSS speed accuracy** when the fix reports
     *   [Location.hasSpeedAccuracy], so poor reception widens the gate;
     * - **3.0 km/h floor** so low limits (e.g. 30 km/h where 5% is only 1.5)
     *   still reject typical open-sky jitter.
     *
     * **Validation:** [SimOverspeedInstrumentedTest] exercises overspeed on
     * routed roads via the built-in simulator (legal speed vs injected over-limit
     * fixes). Outdoor live-GPS noise sizing ([GpsSpeedNoiseInstrumentedTest]) is
     * optional and omitted when no sky lock is available.
     */
    const val MARGIN_KMH: Double = 3.0

    /** Percent of [limitKmh] contributing to [effectiveMarginKmh]. */
    const val LIMIT_FRACTION: Double = 0.05

    /**
     * Hybrid overspeed margin (km/h):
     * `max(limit × [LIMIT_FRACTION], speedAccuracyKmh, [MARGIN_KMH])`.
     */
    fun effectiveMarginKmh(
        limitKmh: Double,
        speedAccuracyKmh: Double? = null,
    ): Double {
        val lim = limitKmh.takeIf { it.isFinite() && it > 0.0 } ?: return MARGIN_KMH
        val acc = speedAccuracyKmh?.takeIf { it.isFinite() && it > 0.0 } ?: 0.0
        return maxOf(lim * LIMIT_FRACTION, acc, MARGIN_KMH)
    }

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
        val margin = effectiveMarginKmh(lim, speedAccuracyKmh)
        return (s - lim) > margin
    }
}
