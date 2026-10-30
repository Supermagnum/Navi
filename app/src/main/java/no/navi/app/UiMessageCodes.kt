package no.navi.app

import android.content.res.Resources

/**
 * Maps Rust `navi.msg.*` progress / status codes to string resources.
 * Legacy English labels pass through unchanged until fully migrated.
 */
object UiMessageCodes {
    const val PREFIX = "navi.msg."

    fun resolve(
        raw: String,
        resources: Resources,
    ): String {
        if (!raw.startsWith(PREFIX)) return raw
        val parts = split(raw.removePrefix(PREFIX))
        return resolveCode(parts.code, parts.arg, resources)?.let { it + parts.suffix } ?: raw
    }

    data class Parts(
        val code: String,
        val arg: String?,
        val suffix: String,
    )

    /** `code`, `code|arg`, or `code <plain suffix>` (callers append e.g. " (region 2 of 2: X)"). */
    fun split(body: String): Parts {
        val pipe = body.indexOf('|')
        val space = body.indexOf(' ')
        if (space >= 0 && (pipe < 0 || space < pipe)) {
            return Parts(body.substring(0, space), null, body.substring(space))
        }
        if (pipe >= 0) return Parts(body.substring(0, pipe), body.substring(pipe + 1), "")
        return Parts(body, null, "")
    }

    private fun resolveCode(
        code: String,
        arg: String?,
        resources: Resources,
    ): String? =
        when (code) {
            "download_region" -> resources.getString(R.string.download_region)
            "download_dem" -> resources.getString(R.string.download_terrain_dem)
            "download_tiles" -> resources.getString(R.string.download_map_tiles)
            "download_elevation" -> resources.getString(R.string.progress_download_elevation)
            "download_pack_update" -> resources.getString(R.string.progress_download_pack_update)
            "download_place_source" ->
                resources.getString(
                    R.string.progress_download_place_source,
                    arg.orEmpty(),
                )
            "build_indexed_bounds" -> resources.getString(R.string.progress_build_indexed_bounds)
            "build_indexed_graphs" -> resources.getString(R.string.progress_build_indexed_graphs)
            "build_indexed_poi" -> resources.getString(R.string.progress_build_indexed_poi)
            "build_indexed_wetlands" ->
                resources.getString(R.string.progress_build_indexed_wetlands)
            "build_graph" ->
                resources.getString(R.string.progress_build_graph, arg.orEmpty())
            "rebuild_local" ->
                resources.getString(R.string.progress_rebuild_local, arg.orEmpty())
            "resuming_download" -> resources.getString(R.string.download_resuming)
            "highway_class" -> resources.getString(highwayClassResId(arg))
            else -> null
        }

    private fun highwayClassResId(tag: String?): Int =
        when (tag?.trim()?.lowercase()?.removeSuffix("_link")) {
            "motorway" -> R.string.highway_class_motorway
            "trunk" -> R.string.highway_class_trunk
            "primary" -> R.string.highway_class_primary
            "secondary" -> R.string.highway_class_secondary
            "tertiary" -> R.string.highway_class_tertiary
            "unclassified" -> R.string.highway_class_unclassified
            "residential" -> R.string.highway_class_residential
            "living_street" -> R.string.highway_class_living_street
            "service" -> R.string.highway_class_service
            "track" -> R.string.highway_class_track
            "path" -> R.string.highway_class_path
            "footway" -> R.string.highway_class_footway
            "cycleway" -> R.string.highway_class_cycleway
            "bridleway" -> R.string.highway_class_bridleway
            "steps" -> R.string.highway_class_steps
            "pedestrian" -> R.string.highway_class_pedestrian
            else -> R.string.highway_class_road
        }
}
