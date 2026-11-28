package no.navi.app

import uniffi.navi.FfiNearbyServiceCategory
import uniffi.navi.FfiNearbyServicesResult
import uniffi.navi.nearbyServices

/** Default radius matches Rust [NEARBY_SERVICES_RADIUS_M]. */
const val NEARBY_SERVICES_RADIUS_M = 150_000.0

/** Re-run search when the fix has moved at least this far. */
const val NEARBY_SERVICES_REFRESH_MOVE_M = 1_000.0

fun allNearbyServiceCategories(): Set<FfiNearbyServiceCategory> =
    FfiNearbyServiceCategory.entries.toSet()

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
