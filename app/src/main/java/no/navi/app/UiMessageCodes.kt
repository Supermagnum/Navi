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
        val body = raw.removePrefix(PREFIX)
        val pipe = body.indexOf('|')
        val code = if (pipe >= 0) body.substring(0, pipe) else body
        val arg = if (pipe >= 0) body.substring(pipe + 1) else null
        return when (code) {
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
            else -> raw
        }
    }
}
