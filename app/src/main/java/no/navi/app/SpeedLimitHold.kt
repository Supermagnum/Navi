package no.navi.app

/**
 * Holds the last known posted/fallback speed limit across brief road-near gaps
 * and adjacent-way toggles so the HUD box does not flicker at a fixed GPS fix.
 *
 * Real limit changes still apply promptly once the same new value is seen for
 * [SWITCH_CONFIRM] consecutive updates (or immediately when the previous hold
 * expired).
 */
class SpeedLimitHold(
    private val gapHoldMs: Long = GAP_HOLD_MS,
    private val switchConfirm: Int = SWITCH_CONFIRM,
    private val nowMs: () -> Long = { android.os.SystemClock.elapsedRealtime() },
) {
    private var heldKmh: Double? = null
    private var heldAtMs: Long = 0L
    private var pendingKmh: Double? = null
    private var pendingHits: Int = 0

    /** Last value shown to the HUD (may be held through a null gap). */
    fun current(): Double? = heldKmh

    /**
     * @param observed limit from road-near / route sample, or null when unknown
     *   for this poll.
     * @return value to publish on [DriveHudState.currentSpeedLimitKmh].
     */
    fun update(observed: Double?): Double? {
        val now = nowMs()
        val next = observed?.takeIf { it.isFinite() && it > 0.0 }
        if (next != null) {
            val cur = heldKmh
            if (cur == null) {
                heldKmh = next
                heldAtMs = now
                pendingKmh = null
                pendingHits = 0
                return heldKmh
            }
            if (kotlin.math.abs(cur - next) < SAME_EPS_KMH) {
                heldAtMs = now
                pendingKmh = null
                pendingHits = 0
                return heldKmh
            }
            // Distinct limit — require sustained confirmation before flipping.
            if (pendingKmh != null && kotlin.math.abs(pendingKmh!! - next) < SAME_EPS_KMH) {
                pendingHits += 1
            } else {
                pendingKmh = next
                pendingHits = 1
            }
            if (pendingHits >= switchConfirm) {
                heldKmh = next
                heldAtMs = now
                pendingKmh = null
                pendingHits = 0
            }
            return heldKmh
        }
        // Brief gap: keep last known limit.
        if (heldKmh != null && now - heldAtMs <= gapHoldMs) {
            return heldKmh
        }
        heldKmh = null
        pendingKmh = null
        pendingHits = 0
        return null
    }

    fun reset() {
        heldKmh = null
        heldAtMs = 0L
        pendingKmh = null
        pendingHits = 0
    }

    companion object {
        /**
         * Keep showing the last limit through unknown/edge gaps. Idle road-near
         * can take several seconds on a cold PBF bbox; 4s was shorter than that
         * and made the HUD limit disappear between polls.
         */
        const val GAP_HOLD_MS: Long = 30_000L

        /** Consecutive polls required before switching to a different limit. */
        const val SWITCH_CONFIRM: Int = 2

        const val SAME_EPS_KMH: Double = 0.5
    }
}
