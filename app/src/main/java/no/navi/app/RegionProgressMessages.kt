package no.navi.app

/**
 * Formats download / index / long-trip progress strings so every phase names
 * the region being processed. When corridor position is known, keeps the
 * existing N-of-M sequence and adds the display name
 * (e.g. `Writing database for region 2 of 4: Västra Götaland`).
 */
object RegionProgressMessages {
    fun regionName(regionId: String): String {
        val id = regionId.trim().trim('/')
        if (id.isEmpty()) return ""
        return RegionCoverage.displayName(id)
    }

    /**
     * Build a phase label that always includes the region name.
     * [index]/[total] are 1-based corridor positions when known.
     */
    fun phaseForRegion(
        phase: String,
        regionId: String,
        index: Int? = null,
        total: Int? = null,
    ): String {
        val name = regionName(regionId).ifBlank { regionId.trim().trim('/') }
        val base = phase.trim().trimEnd('…', '.', ' ')
        return if (index != null && total != null && total > 0 && index > 0) {
            "$base for region $index of $total: $name"
        } else if (name.isNotEmpty()) {
            "$base: $name"
        } else {
            phase
        }
    }

    /**
     * Annotate an existing progress label that lacks a region identity.
     * Preserves the original phase text; appends ` (region N of M: Name)` or
     * ` (Name)` when the name is not already present.
     */
    fun annotate(
        label: String,
        regionId: String,
        index: Int? = null,
        total: Int? = null,
    ): String {
        val id = regionId.trim().trim('/')
        if (id.isEmpty() || label.isBlank()) return label
        val name = regionName(id)
        val leaf = id.substringAfterLast('/')
        val hasName =
            (name.isNotEmpty() && label.contains(name, ignoreCase = true)) ||
                (leaf.isNotEmpty() && label.contains(leaf, ignoreCase = true)) ||
                label.contains(id, ignoreCase = true)
        val hasSeq =
            index != null &&
                total != null &&
                total > 0 &&
                label.contains("$index of $total")
        if (hasName && (index == null || total == null || hasSeq)) return label
        if (hasName && index != null && total != null && total > 0) {
            return "$label (region $index of $total)"
        }
        val tag =
            if (index != null && total != null && total > 0 && index > 0) {
                "region $index of $total: ${name.ifBlank { leaf.ifBlank { id } }}"
            } else {
                name.ifBlank { leaf.ifBlank { id } }
            }
        return "$label ($tag)"
    }

    /** One segment of [LongTripCoordinator] status: `Name (N of M)=State`. */
    fun longTripPart(
        regionId: String,
        state: String,
        index: Int,
        total: Int,
    ): String {
        val name =
            regionName(regionId).ifBlank {
                regionId.substringAfterLast('/').ifBlank { regionId }
            }
        return "$name ($index of $total)=$state"
    }

    /**
     * Corridor 1-based index/total for [regionId] when a long-trip plan is active.
     */
    fun sequenceFor(regionId: String): Pair<Int, Int>? {
        val plan = LongTripCoordinator.currentPlan() ?: return null
        val id = regionId.trim().trim('/')
        if (id.isEmpty()) return null
        val idx =
            plan.regionsInOrder.indexOfFirst {
                PackRegionAvailability.regionIdsMatchForCatalog(it, id)
            }
        if (idx < 0) return null
        return (idx + 1) to plan.regionsInOrder.size
    }
}
