package no.navi.app

import uniffi.navi.FfiNearbyServiceCategory
import uniffi.navi.FfiNearbyServicesResult
import uniffi.navi.nearbyServices

/** Default radius matches Rust [NEARBY_SERVICES_RADIUS_M]. */
const val NEARBY_SERVICES_RADIUS_M = 150_000.0

/** Re-run search when the fix has moved at least this far. */
const val NEARBY_SERVICES_REFRESH_MOVE_M = 1_000.0

fun allNearbyServiceCategories(): Set<FfiNearbyServiceCategory> = FfiNearbyServiceCategory.entries.toSet()

/**
 * Offline nearby-services query. Call from [Dispatchers.IO] only.
 */
fun queryNearbyServices(
    indexDbPath: String,
    lat: Double,
    lon: Double,
    categories: Collection<FfiNearbyServiceCategory>,
    installedRegions: Collection<String>,
    limit: UInt = 200u,
): FfiNearbyServicesResult {
    val cats =
        if (categories.isEmpty()) {
            FfiNearbyServiceCategory.entries.toList()
        } else {
            categories.toList()
        }
    return nearbyServices(
        indexDbPath = indexDbPath,
        lat = lat,
        lon = lon,
        radiusM = NEARBY_SERVICES_RADIUS_M,
        categories = cats,
        installedRegions = installedRegions.toList(),
        limit = limit,
    )
}

fun nearbyServicesGapMessage(
    missingRegions: List<String>,
    resources: android.content.res.Resources,
): String {
    if (missingRegions.isEmpty()) return ""
    // Prefer naming Norway landsdel leaves the user recognizes.
    val labels =
        missingRegions
            .map { RegionCoverage.displayName(it) }
            .distinct()
            .take(3)
    if (labels.isEmpty()) return ""
    return resources.getString(
        R.string.nearby_services_region_gap,
        labels.joinToString(", "),
    )
}

fun logNearbyServicesResult(
    label: String,
    lat: Double,
    lon: Double,
    result: FfiNearbyServicesResult,
) {
    android.util.Log.i(
        "NearbyServices",
        "RESULT label=$label lat=$lat lon=$lon query_ms=${result.queryTimeMs} " +
            "hits=${result.hits.size} missing=${result.missingRegions.joinToString("|")}",
    )
    val byCat = result.hits.groupBy { it.category }
    for (cat in FfiNearbyServiceCategory.entries) {
        val top = byCat[cat].orEmpty().sortedBy { it.distanceM }.take(5)
        for ((i, h) in top.withIndex()) {
            android.util.Log.i(
                "NearbyServices",
                "TOP5 label=$label cat=${cat.name} i=$i name=${h.name} " +
                    "dist_m=${"%.1f".format(h.distanceM)} compass=${h.compass} kind=${h.kind}",
            )
        }
        if (top.isEmpty()) {
            android.util.Log.i("NearbyServices", "TOP5 label=$label cat=${cat.name} (none)")
        }
    }
}
