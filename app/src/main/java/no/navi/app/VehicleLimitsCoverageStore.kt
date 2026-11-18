package no.navi.app

import android.content.Context
import org.json.JSONObject
import java.io.File

/**
 * Reads per-leaf `vehicle_limits_coverage` from the active MWM catalog snapshot
 * (`mwm/releases/{version}/catalog.json`).
 */
object VehicleLimitsCoverageStore {
    data class LeafCoverage(
        val matchRate: Double?,
        val mapRelease: String?,
    )

    fun leafCoverage(context: Context, leafId: String): LeafCoverage? {
        val catalog = catalogFile(context) ?: return null
        return runCatching {
            val root = JSONObject(catalog.readText())
            val leaves = root.optJSONArray("leaves") ?: return null
            for (i in 0 until leaves.length()) {
                val leaf = leaves.optJSONObject(i) ?: continue
                if (leaf.optString("id") != leafId) continue
                val cov = leaf.optJSONObject("vehicle_limits_coverage") ?: return LeafCoverage(null, null)
                return LeafCoverage(
                    matchRate = if (cov.has("match_rate")) cov.getDouble("match_rate") else null,
                    mapRelease = cov.optString("map_release").ifBlank { null },
                )
            }
            null
        }.getOrNull()
    }

    /** Leaves that are present but below [threshold], or have no coverage object. */
    fun partialLeaves(
        context: Context,
        leafIds: Collection<String>,
        threshold: Double = VehicleLimitsCoverageConfig.threshold(context),
    ): List<String> {
        val out = ArrayList<String>()
        for (id in leafIds) {
            if (id.isBlank()) continue
            val cov = leafCoverage(context, id)
            val rate = cov?.matchRate
            if (rate == null || rate + 1e-9 < threshold) {
                out.add(id)
            }
        }
        return out
    }

    private fun catalogFile(context: Context): File? {
        val dataDir = NaviAppData.resolve(context)
        val active = File(dataDir, "mwm/active.json")
        if (!active.isFile) return null
        val version =
            runCatching {
                JSONObject(active.readText()).optString("version")
            }.getOrNull()?.takeIf { it.isNotBlank() } ?: return null
        val catalog = File(dataDir, "mwm/releases/$version/catalog.json")
        return catalog.takeIf { it.isFile }
    }
}
