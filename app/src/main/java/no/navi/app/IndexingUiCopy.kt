package no.navi.app

/**
 * Strips user-facing "index / indexing / indexed / place index" wording.
 * MWM ships search/routing data built-in; legacy place-index / indexed-maps
 * jobs may still run internally but must not surface that vocabulary in UI.
 */
object IndexingUiCopy {
    /** Rewrite progress / status lines before toast, Tools footer, or panel text. */
    fun sanitizeProgress(raw: String): String {
        if (raw.isBlank()) return raw
        var s = raw
        s =
            s
                .replace(Regex("""(?i)place\s*index\s*ready"""), "Region ready")
                .replace(Regex("""(?i)place\s*index\s*error"""), "Search data error")
                .replace(Regex("""(?i)place\s*index"""), "Preparing search")
                .replace(Regex("""(?i)indexed\s*maps\s*\(background\)"""), "Preparing maps (background)")
                .replace(Regex("""(?i)indexed\s*maps"""), "Maps")
                .replace(Regex("""(?i)\bbuilding\s+indexed\s+maps\b"""), "Preparing maps")
                .replace(Regex("""(?i)\bindexing\b"""), "preparing")
                .replace(Regex("""(?i)\bindexed\b"""), "ready")
                .replace(Regex("""(?i)\bstedsindeks\b"""), "stedsdata")
                .replace(Regex("""(?i)\bindekserte?\s+kart\b"""), "Kart")
                .replace(Regex("""(?i)\bindekserer\b"""), "forbereder")
                .replace(Regex("""(?i)\bindeksering\b"""), "forberedelse")
        return s
    }

    /** Long-trip status segment labels (enum names → user copy). */
    fun longTripStateLabel(state: String): String =
        when (state) {
            "Indexing" -> "Installing"
            "Indexed" -> "Ready"
            else -> state
        }
}
