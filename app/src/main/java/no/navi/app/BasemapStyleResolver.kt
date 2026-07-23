package no.navi.app

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import org.json.JSONObject
import uniffi.navi.FfiPmtilesJob
import uniffi.navi.pmtilesListCovering
import java.io.File
import java.io.FileOutputStream

/**
 * Resolves which MapLibre style to load: live OpenFreeMap Liberty, Liberty with
 * Mapterhorn DEM hillshade (opt-in 3D), or a local Protomaps PMTiles style when
 * a completed extract covers the camera.
 *
 * See [docs/map-styles.md].
 */
object BasemapStyleResolver {
    const val LIBERTY_URL = "https://tiles.openfreemap.org/styles/liberty"

    /**
     * Opt-in “3D” is Mapterhorn DEM **hillshade** only. MapLibre Native has no
     * mesh `terrain` / `sky` — see [MapterhornTerrain]. Camera stays flat (no tilt).
     */
    @Deprecated("3D no longer tilts the camera; always 0", ReplaceWith("0.0"))
    const val TERRAIN_VIEW_TILT = 0.0

    @Deprecated("Renamed; unused", ReplaceWith("0.0"))
    const val OPENFREEMAP_3D_PITCH = 0.0

    @Deprecated("Hardcoded liberty-3d bearing removed; camera follows user rotation modes")
    const val OPENFREEMAP_3D_BEARING = 0.0

    private const val ASSET_STYLE_ROOT = "map-styles/protomaps-light"
    private const val PREPARED_DIR = "map-styles/protomaps-light"

    enum class StyleKind {
        OnlineLiberty,
        Online3d,
        OfflineProtomaps,
    }

    data class ResolvedStyle(
        val kind: StyleKind,
        /** Path or URL passed to [org.maplibre.android.maps.MapLibreMap.setStyle]. */
        val styleUri: String,
        val coveringJob: FfiPmtilesJob? = null,
        val note: String? = null,
        /** Camera tilt when hillshade 3D is active (0 for flat 2D). */
        val cameraPitch: Double = 0.0,
        val cameraBearing: Double? = null,
        /** When true, [MapterhornTerrain.attach] after the vector style loads. */
        val attachMapterhornTerrain: Boolean = false,
        /**
         * DEM source for hillshade: online TileJSON or local `pmtiles://file://…`.
         * Null when 3D is not requested / unavailable.
         */
        val demSourceUri: String? = null,
    )

    fun hasNetwork(context: Context): Boolean {
        val cm = context.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
            ?: return false
        val network = cm.activeNetwork ?: return false
        val caps = cm.getNetworkCapabilities(network) ?: return false
        // AAOS / emulator often reports Wi‑Fi without VALIDATED; INTERNET alone
        // is also missing on some secondary-user profiles. Treat any IP transport
        // as online enough to attempt Mapterhorn TileJSON.
        if (caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) ||
            caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
        ) {
            return true
        }
        return caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) ||
            caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) ||
            caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) ||
            caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN)
    }

    /**
     * Prefer local PMTiles when a completed job covers [lat]/[lon].
     * 3D hillshade: online Mapterhorn TileJSON when networked, or local
     * `{region}_dem.pmtiles` beside the offline basemap when present.
     */
    fun resolve(
        context: Context,
        dataDir: File,
        lat: Double,
        lon: Double,
        prefer3d: Boolean,
        vulkanAvailable: Boolean,
        forceOnline2d: Boolean = false,
    ): ResolvedStyle {
        if (!forceOnline2d) {
            val covering = runCatching {
                pmtilesListCovering(dataDir.absolutePath, lat, lon)
            }.getOrDefault(emptyList())
                .firstOrNull { File(it.localPath).isFile }

            if (covering != null) {
                val localDem = MapterhornTerrain.localDemBesideBasemap(covering.localPath)
                val want3d = prefer3d && vulkanAvailable
                val offline3d = want3d && localDem != null
                // Always attempt online Mapterhorn DEM when local extract is missing.
                // Gating on hasNetwork left opt-in 3D looking like a no-op on AAOS.
                val onlineDem3d = want3d && localDem == null
                val uri = prepareOfflineStyle(
                    context,
                    covering.localPath,
                    demFor3d = if (offline3d) localDem else null,
                )
                    ?: return fallbackOnline(context, prefer3d, vulkanAvailable, "offline style prepare failed")
                return ResolvedStyle(
                    kind = StyleKind.OfflineProtomaps,
                    styleUri = uri,
                    coveringJob = covering,
                    note = when {
                        offline3d -> "Offline Protomaps + Mapterhorn DEM hillshade"
                        onlineDem3d ->
                            "Offline Protomaps + online Mapterhorn DEM (no local ${covering.regionKey}_dem.pmtiles)"
                        else -> null
                    },
                    cameraPitch = if (want3d) TERRAIN_VIEW_TILT else 0.0,
                    attachMapterhornTerrain = offline3d || onlineDem3d,
                    demSourceUri = when {
                        offline3d -> MapterhornTerrain.demPmtilesUri(localDem!!)
                        onlineDem3d -> MapterhornTerrain.TILEJSON_URL
                        else -> null
                    },
                )
            }
        }

        return fallbackOnline(context, prefer3d, vulkanAvailable, note = null)
    }

    private fun fallbackOnline(
        context: Context,
        prefer3d: Boolean,
        vulkanAvailable: Boolean,
        note: String?,
    ): ResolvedStyle {
        if (prefer3d && vulkanAvailable) {
            return ResolvedStyle(
                kind = StyleKind.Online3d,
                styleUri = LIBERTY_URL,
                note = note ?: "Liberty + Mapterhorn DEM hillshade",
                cameraPitch = TERRAIN_VIEW_TILT,
                attachMapterhornTerrain = true,
                demSourceUri = MapterhornTerrain.TILEJSON_URL,
            )
        }
        if (prefer3d && !vulkanAvailable) {
            return ResolvedStyle(
                kind = StyleKind.OnlineLiberty,
                styleUri = LIBERTY_URL,
                note = note ?: "3D unavailable without Vulkan; using 2D Liberty",
            )
        }
        return ResolvedStyle(
            kind = StyleKind.OnlineLiberty,
            styleUri = LIBERTY_URL,
            note = note,
        )
    }

    /**
     * Copy bundled sprites/glyphs once, rewrite style template to point at
     * `pmtiles://file://...` and local sprite/glyph paths.
     * When [demFor3d] is set, bake Mapterhorn DEM hillshade into the style JSON.
     */
    fun prepareOfflineStyle(
        context: Context,
        pmtilesAbsolutePath: String,
        demFor3d: File? = null,
    ): String? {
        val pmFile = File(pmtilesAbsolutePath)
        if (!pmFile.isFile) return null

        val outRoot = File(context.filesDir, PREPARED_DIR)
        if (!outRoot.exists()) {
            copyAssetTree(context, ASSET_STYLE_ROOT, outRoot)
        }

        val template = context.assets.open("$ASSET_STYLE_ROOT/style.template.json")
            .bufferedReader()
            .use { it.readText() }

        val spriteBase = File(outRoot, "sprites/light").absolutePath
        val glyphsBase = File(outRoot, "fonts").absolutePath
        val pmtilesUrl = "pmtiles://file://${pmFile.absolutePath}"

        val rewritten = template
            .replace("__PMTILES_URL__", pmtilesUrl)
            .replace("__SPRITE__", "file://$spriteBase")
            .replace("__GLYPHS__", "file://$glyphsBase")

        // Ensure attribution survives any template edits.
        val json = JSONObject(rewritten)
        val sources = json.getJSONObject("sources")
        val pm = sources.getJSONObject("protomaps")
        if (!pm.has("attribution")) {
            pm.put("attribution", "© OpenStreetMap © Protomaps")
        }
        if (demFor3d != null && demFor3d.isFile) {
            MapterhornTerrain.augmentStyleJson(
                json,
                MapterhornTerrain.demPmtilesUri(demFor3d),
            )
        }

        val outName = if (demFor3d != null) "style.local.3d.json" else "style.local.json"
        val outStyle = File(outRoot, outName)
        outStyle.writeText(json.toString())
        // MapLibre Native expects a URI scheme for local styles.
        return "file://${outStyle.absolutePath}"
    }

    private fun copyAssetTree(context: Context, assetPath: String, destDir: File) {
        destDir.mkdirs()
        val children = context.assets.list(assetPath) ?: return
        for (name in children) {
            val childAsset = if (assetPath.isEmpty()) name else "$assetPath/$name"
            val childDest = File(destDir, name)
            val sub = context.assets.list(childAsset)
            if (sub != null && sub.isNotEmpty()) {
                copyAssetTree(context, childAsset, childDest)
            } else {
                context.assets.open(childAsset).use { input ->
                    childDest.parentFile?.mkdirs()
                    FileOutputStream(childDest).use { output ->
                        input.copyTo(output)
                    }
                }
            }
        }
    }
}
