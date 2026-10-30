package no.navi.app

/**
 * When indexed graph packs are missing/stale, every plan rebuilds from the PBF
 * (~tens of seconds). Surface that so "extreme slowness" is actionable.
 *
 * Must not match sidecar flags such as `poi_pack_hit=false` /
 * `wetland_pack_hit=false` / `overnight_buildings_pack_hit=false` — those share
 * the `pack_hit=false` substring but do not mean PBF graph fallback.
 */
internal fun reportIndicatesIndexedPackMiss(report: String): Boolean = Regex("""(?<![\w])pack_hit=false""").containsMatchIn(report)

internal fun indexedPackMissHint(report: String): String? {
    if (!reportIndicatesIndexedPackMiss(report)) return null
    return "Slow plan: indexed maps not ready (PBF fallback). " +
        "Wait for background indexing, or Tools → Rebuild indexed maps."
}

internal fun withIndexedPackMissHint(
    statusLine: String,
    report: String,
): String {
    val hint = indexedPackMissHint(report) ?: return statusLine
    return if (statusLine.isBlank()) hint else "$statusLine\n$hint"
}
