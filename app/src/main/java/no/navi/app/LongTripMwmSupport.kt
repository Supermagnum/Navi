package no.navi.app

import android.content.Context
import android.util.Log
import uniffi.navi.defaultPackServerBaseUrl
import uniffi.navi.mwmInstalled
import uniffi.navi.mwmListRegions

/**
 * Long-trip corridor downloads prefer CoMaps **MWM leaves** (catalog regions).
 * Catalog `region_to_leaves` is tried first; [LongTripMwmGeofabrikAliases] fills
 * gaps when CoMaps leaves exist but the bake id is missing from the catalog.
 * Geofabrik `.osm.pbf` remains the fallback when the MWM client is off, the
 * catalog/alias resolve fails, or an MWM install fails — not merely because a
 * non-Nordic bake id is absent from `region_to_leaves`.
 *
 * Installs go through [MwmMapDownloader.installBlocking] so the normal in-app
 * download progress UI (Tools strip, readiness panel, toast) sees live %.
 */
object LongTripMwmSupport {
    private const val TAG = "LongTripMwm"

    /** `europe/norway/ostlandet` → `europe_norway_ostlandet`. */
    fun geofabrikPathToBakeId(geofabrikPath: String): String =
        geofabrikPath
            .trim()
            .trim('/')
            .lowercase()
            .replace('/', '_')

    /** Hyphen and underscore variants for catalog id matching. */
    fun bakeIdCandidates(geofabrikPath: String): List<String> {
        val base = geofabrikPathToBakeId(geofabrikPath)
        val underscored = base.replace('-', '_')
        return listOf(base, underscored).distinct()
    }

    /**
     * Resolve CoMaps leaf ids for a long-trip corridor region
     * (`europe/sweden/norrbotten`, …). Null when MWM is unavailable or unmapped.
     */
    fun resolveLeafIds(
        context: Context,
        geofabrikPath: String,
    ): List<String>? {
        if (!MwmClientBootstrap.isEnabled()) return null
        val origin = defaultPackServerBaseUrl()
        val list =
            runCatching { mwmListRegions(origin) }
                .onFailure { Log.w(TAG, "mwmListRegions failed for $geofabrikPath", it) }
                .getOrNull()
                ?: return null
        if (!list.ok || list.regions.isEmpty()) {
            Log.w(TAG, "mwm catalog unavailable for $geofabrikPath: ${list.error}")
            return null
        }
        val want = bakeIdCandidates(geofabrikPath).map { it.lowercase() }.toSet()
        val hit =
            list.regions.firstOrNull { r ->
                val id = r.id.lowercase()
                id in want || id.replace('-', '_') in want
            }
        if (hit != null && hit.leafIds.isNotEmpty()) {
            Log.i(
                TAG,
                "mapped $geofabrikPath -> ${hit.id} leaves=${hit.leafIds.size} bytes=${hit.totalBytes}",
            )
            return hit.leafIds
        }
        val alias =
            bakeIdCandidates(geofabrikPath)
                .firstNotNullOfOrNull { LongTripMwmGeofabrikAliases.leafIdsForBakeId(it) }
                ?: LongTripMwmGeofabrikAliases.leafIdsForGeofabrikPath(geofabrikPath)
        if (alias != null) {
            Log.i(
                TAG,
                "mapped $geofabrikPath -> alias (${alias.size} leaves, catalog had no $want)",
            )
            return alias
        }
        Log.i(TAG, "no MWM region mapping for $geofabrikPath (tried $want)")
        return null
    }

    /** True when every [leafIds] leaf is in the active MWM release. */
    fun leavesInstalled(
        context: Context,
        leafIds: Collection<String>,
    ): Boolean {
        if (leafIds.isEmpty()) return false
        val dir = MwmDataDir.resolve(context).absolutePath
        val inst =
            runCatching { mwmInstalled(dir) }
                .getOrNull()
                ?: return false
        if (inst.version.isNullOrBlank()) return false
        val have = inst.leafIds.toSet()
        return leafIds.all { it in have }
    }

    /**
     * Ensure MWM leaves for [geofabrikPath] are installed. Returns true on
     * success (already present or freshly installed + session open).
     */
    fun ensureInstalled(
        context: Context,
        geofabrikPath: String,
    ): Boolean {
        val leaves = resolveLeafIds(context, geofabrikPath) ?: return false
        if (leavesInstalled(context, leaves)) {
            Log.i(TAG, "MWM already installed for $geofabrikPath (${leaves.size} leaves)")
            return ComapsSessionBootstrap.ensureOpen(context, force = false)
        }
        val label =
            RegionProgressMessages.regionName(geofabrikPath).ifBlank {
                geofabrikPath.substringAfterLast('/').ifBlank { geofabrikPath }
            }
        Log.i(TAG, "MWM install start $geofabrikPath leaves=$leaves")
        val ok =
            MwmMapDownloader.installBlocking(
                context = context,
                leafIds = leaves,
                label = label,
                geofabrikPath = geofabrikPath,
            )
        if (!ok) {
            Log.w(TAG, "MWM install failed for $geofabrikPath — PBF fallback")
            return false
        }
        Log.i(TAG, "MWM install ok $geofabrikPath")
        return true
    }
}
