package no.navi.app

/**
 * Translates OSM-update FFI / planner strings into short on-screen copy.
 *
 * Internal reports still contain sequence numbers, `method=`, `reason=`, and
 * `USER_VISIBLE=` flags for diagnostics. Those must not appear in Tools or the
 * status toast.
 */
object OsmUpdateUserCopy {
    const val UP_TO_DATE = "Map data is up to date."
    const val AVAILABLE = "New map data is available. Tap Apply pending OSM update to download."
    const val APPLYING = "Download in progress…"
    const val UPDATED = "Map data updated."
    const val UPDATED_INDEXING = "Map data updated. Preparing search and routes in the background."
    const val NO_REGION = "Can't check for updates until a map region is downloaded."
    const val NEED_CHECK = "Check for map updates first, or the map is already up to date."
    const val NO_BINDING = "This map isn't set up for updates. Download a region in Tools first."
    const val FAILED = "Couldn't update map data. Try again."

    fun looksTechnical(raw: String): Boolean {
        val t = raw.lowercase()
        if (t.isBlank()) return false
        return t.contains("user_visible=") ||
            t.contains("local_sequence=") ||
            t.contains("remote_sequence=") ||
            t.contains("method=") ||
            t.contains("reason=") ||
            t.contains("days_behind=") ||
            t.contains("geofabrik") ||
            t.contains("osc.gz") ||
            t.contains("osmium") ||
            t.contains("region_meta") ||
            t.contains("full re-download") ||
            t.contains("full_redownload") ||
            t.contains("osm update check unsupported") ||
            t.contains("confirm apply") ||
            (t.contains("pass") && t.contains("method"))
    }

    fun forCheckReport(raw: String): String {
        val t = raw.lowercase()
        if (t.contains("up to date")) return UP_TO_DATE
        if (t.contains("unsupported") || t.contains("no region_meta") || t.contains("empty geofabrik")) {
            return NO_REGION
        }
        if (t.lineSequence().any { it.trim().startsWith("fail") }) return FAILED
        if (t.contains("update available") ||
            t.contains("full re-download") ||
            t.contains("full_redownload") ||
            t.contains("confirm apply")
        ) {
            return AVAILABLE
        }
        return if (looksTechnical(raw)) FAILED else raw.trim().ifBlank { FAILED }
    }

    fun forApplyReport(raw: String): String {
        val t = raw.lowercase()
        if (t.contains("already up to date") || (t.contains("up to date") && t.contains("nothing"))) {
            return UP_TO_DATE
        }
        if (t.contains("cannot apply") || t.contains("unsupported")) return NO_BINDING
        if (t.lineSequence().any { it.trim().startsWith("fail") }) return FAILED
        if (t.contains("pass")) return UPDATED
        return if (looksTechnical(raw)) FAILED else raw.trim().ifBlank { FAILED }
    }

    /** Safety net for any status string that might still carry planner dump text. */
    fun sanitize(raw: String): String {
        if (!looksTechnical(raw)) return raw
        val t = raw.lowercase()
        if (t.contains("already up to date") || t.contains("nothing applied")) return UP_TO_DATE
        if (t.contains("up to date")) return UP_TO_DATE
        if (t.contains("unsupported") || t.contains("no region_meta")) return NO_REGION
        if (t.contains("pass")) return UPDATED
        if (t.contains("update available") || t.contains("full re-download") || t.contains("full_redownload")) {
            return AVAILABLE
        }
        if (t.lineSequence().any { it.trim().startsWith("fail") }) return FAILED
        return FAILED
    }
}
