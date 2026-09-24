package no.navi.app

/**
 * Stabilizes the main-screen status toast and Tools process footer against
 * post-Task-1 concurrent download/index churn (layout jump + rapid redraw).
 */
object StatusUi {
    /** Reserved toast height so short↔long progress strings do not reflow chrome. */
    const val TOAST_MIN_HEIGHT_DP = 48

    /** Cap UI refreshes even when underlying state changes faster. */
    const val COALESCE_MIN_INTERVAL_MS = 300L

    /**
     * After this hold time on an unchanged phase label, append an elapsed
     * suffix so long PBF / sqlite phases do not look frozen.
     */
    const val ACTIVITY_PULSE_AFTER_MS = 1_000L

    private val ACTIVITY_PULSE_SUFFIX =
        Regex("""\s·\s(?:\d+m\s)?\d+s$""")

    data class CoalesceState(
        val text: String = "",
        val lastEmittedMs: Long = 0L,
        val pending: String? = null,
        val renderCount: Int = 0,
        val inputCount: Int = 0,
    )

    /** Elapsed hold for [withActivityPulse] (`12s` or `2m 05s`). */
    fun formatActivityElapsed(elapsedMs: Long): String {
        val sec = (elapsedMs / 1000L).coerceAtLeast(0L)
        val m = sec / 60L
        val s = sec % 60L
        return if (m > 0L) {
            "%dm %02ds".format(m, s)
        } else {
            "${s}s"
        }
    }

    /** Remove a prior activity-pulse suffix so phase identity stays stable. */
    fun stripActivityPulse(line: String): String = line.replace(ACTIVITY_PULSE_SUFFIX, "").trimEnd()

    /**
     * When [phaseHeldMs] exceeds [ACTIVITY_PULSE_AFTER_MS], append ` · Nm Ns`
     * so the chrome visibly advances even if the native phase label is fixed.
     */
    fun withActivityPulse(
        base: String,
        phaseHeldMs: Long,
    ): String {
        val stripped = stripActivityPulse(base)
        if (stripped.isBlank() || phaseHeldMs < ACTIVITY_PULSE_AFTER_MS) {
            return stripped
        }
        return "$stripped · ${formatActivityElapsed(phaseHeldMs)}"
    }

    /**
     * Coalesce [incoming] into at most one emit per [COALESCE_MIN_INTERVAL_MS].
     * Returns the new state; [CoalesceState.text] is what the UI should show.
     * When the interval has not elapsed, [pending] holds the latest value for a
     * later flush (caller may schedule a delayed emit).
     */
    fun coalesce(
        state: CoalesceState,
        incoming: String,
        nowMs: Long,
    ): CoalesceState {
        val nextInput = state.inputCount + 1
        if (incoming == state.text && state.pending == null) {
            return state.copy(inputCount = nextInput)
        }
        val elapsed = nowMs - state.lastEmittedMs
        return if (state.lastEmittedMs == 0L || elapsed >= COALESCE_MIN_INTERVAL_MS) {
            state.copy(
                text = incoming,
                lastEmittedMs = nowMs,
                pending = null,
                renderCount = state.renderCount + 1,
                inputCount = nextInput,
            )
        } else {
            state.copy(pending = incoming, inputCount = nextInput)
        }
    }

    /** Flush a pending coalesced value (e.g. after the min interval). */
    fun flushPending(
        state: CoalesceState,
        nowMs: Long,
    ): CoalesceState {
        val pending = state.pending ?: return state
        return state.copy(
            text = pending,
            lastEmittedMs = nowMs,
            pending = null,
            renderCount = state.renderCount + 1,
        )
    }

    /**
     * Build Tools footer lines. Drops near-duplicate tools_status when it names
     * the same region (or shares a long common prefix) as an in-progress stream.
     */
    fun toolsVisibleLines(
        regionDownloadProgress: String,
        pmtilesProgress: String,
        placeIndexUiLine: String,
        indexedMapsUiLine: String,
        toolsStatusRaw: String,
    ): List<Pair<String, String>> {
        val process =
            buildList {
                if (regionDownloadProgress.isNotBlank()) {
                    add("region_download_progress" to regionDownloadProgress)
                }
                if (pmtilesProgress.isNotBlank()) {
                    add("pmtiles_progress" to pmtilesProgress)
                }
                if (placeIndexUiLine.isNotBlank()) {
                    add("place_index_bg_status" to placeIndexUiLine)
                }
                if (indexedMapsUiLine.isNotBlank()) {
                    add("indexed_maps_bg_status" to indexedMapsUiLine)
                }
            }
        val toolsStatus = toolsStatusRaw.trim()
        if (toolsStatus.isBlank()) return process
        if (process.any { overlapsStatus(it.second, toolsStatus) }) return process
        return process + ("tools_status" to toolsStatus)
    }

    /** True when [a] and [b] are the same line or clearly the same region/phase. */
    fun overlapsStatus(
        a: String,
        b: String,
    ): Boolean {
        if (a.isBlank() || b.isBlank()) return false
        if (a == b) return true
        val na = normalize(a)
        val nb = normalize(b)
        if (na == nb) return true
        // Same region leaf mentioned in both (avoid Ostlandet x3).
        val leafA = regionLeafTokens(a)
        val leafB = regionLeafTokens(b)
        if (leafA.isNotEmpty() && leafA.any { it in leafB }) {
            // Only treat as dup when both look like progress/status, not unrelated copy.
            return looksLikeProgress(a) && looksLikeProgress(b)
        }
        // Near-duplicate percent lines: shared prefix before digits.
        val pa = na.takeWhile { !it.isDigit() }
        val pb = nb.takeWhile { !it.isDigit() }
        return pa.length >= 12 && pa == pb
    }

    private fun normalize(s: String): String =
        s
            .lowercase()
            .replace(Regex("\\s+"), " ")
            .trim()

    private fun looksLikeProgress(s: String): Boolean {
        val lower = s.lowercase()
        return lower.contains('%') ||
            lower.contains("download") ||
            lower.contains("place index") ||
            lower.contains("indexed maps") ||
            lower.contains("basemap") ||
            lower.contains("writing") ||
            lower.contains("fetching")
    }

    /**
     * Tracks the current busy phase so [pulse] can append a growing elapsed
     * suffix while the underlying progress string is unchanged.
     */
    class ActivityTracker {
        private var phaseKey: String = ""
        private var phaseSinceMs: Long = 0L

        fun pulse(
            line: String,
            nowMs: Long,
        ): String {
            if (line.isBlank()) {
                phaseKey = ""
                phaseSinceMs = 0L
                return ""
            }
            val base = stripActivityPulse(line)
            if (base != phaseKey) {
                phaseKey = base
                phaseSinceMs = nowMs
            }
            return withActivityPulse(base, nowMs - phaseSinceMs)
        }

        fun reset() {
            phaseKey = ""
            phaseSinceMs = 0L
        }
    }

    private fun regionLeafTokens(s: String): Set<String> {
        val out = mutableSetOf<String>()
        val lower = s.lowercase()
        // Geofabrik leaf path segments that appear in annotated labels.
        for (token in listOf(
            "ostlandet",
            "vestlandet",
            "trondelag",
            "nordland",
            "halland",
            "vastra",
            "gotaland",
            "skane",
            "denmark",
            "schleswig",
            "hamburg",
            "niedersachsen",
            "nordrhein",
        )) {
            if (lower.contains(token)) out += token
        }
        // Display names from RegionCoverage when present.
        Regex("""\(([1-9]\d*) of ([1-9]\d*): ([^)]+)\)""")
            .findAll(s)
            .forEach { m ->
                m.groupValues
                    .getOrNull(3)
                    ?.lowercase()
                    ?.split(Regex("[\\s\\-_/]+"))
                    ?.filter { it.length >= 4 }
                    ?.forEach { out += it }
            }
        return out
    }
}
