package no.navi.app

/**
 * Holds the visible road-sign / children / speed-limit approach box steady so
 * alternating nearest hazards (or cone plates that clear every GPS tick) do not
 * flicker. Real closer/urgency changes still switch after [SWITCH_CONFIRM]
 * consecutive polls, or immediately when the held plate expires.
 */
class RoadSignWarningHold(
    private val gapHoldMs: Long = GAP_HOLD_MS,
    private val switchConfirm: Int = SWITCH_CONFIRM,
    private val nowMs: () -> Long = { android.os.SystemClock.elapsedRealtime() },
) {
    private var held: RoadSignWarningState = RoadSignWarningState()
    private var heldAtMs: Long = 0L
    private var pendingKey: String? = null
    private var pendingHits: Int = 0

    fun current(): RoadSignWarningState = held

    fun update(observed: RoadSignWarningState): RoadSignWarningState {
        val now = nowMs()
        if (!observed.active || observed.phase == "hidden" || observed.iconKey.isBlank()) {
            if (held.active && now - heldAtMs <= gapHoldMs) {
                return held
            }
            held = RoadSignWarningState()
            pendingKey = null
            pendingHits = 0
            return held
        }
        val key = plateKey(observed)
        if (!held.active) {
            held = observed
            heldAtMs = now
            pendingKey = null
            pendingHits = 0
            return held
        }
        if (plateKey(held) == key) {
            // Same plate — refresh distance / phase text.
            held = observed
            heldAtMs = now
            pendingKey = null
            pendingHits = 0
            return held
        }
        if (pendingKey == key) {
            pendingHits += 1
        } else {
            pendingKey = key
            pendingHits = 1
        }
        if (pendingHits >= switchConfirm) {
            held = observed
            heldAtMs = now
            pendingKey = null
            pendingHits = 0
        }
        return held
    }

    fun reset() {
        held = RoadSignWarningState()
        heldAtMs = 0L
        pendingKey = null
        pendingHits = 0
    }

    private fun plateKey(s: RoadSignWarningState): String = "${s.code}|${s.iconKey}|${s.label}"

    companion object {
        const val GAP_HOLD_MS: Long = 4_000L
        const val SWITCH_CONFIRM: Int = 2
    }
}
