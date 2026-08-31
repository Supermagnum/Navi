package no.navi.app

import uniffi.navi.pmtilesGetTile
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import kotlin.math.max
import kotlin.math.min

/**
 * Fetch Mapterhorn terrarium DEM tiles for on-the-fly contour generation.
 *
 * Uses the same elevation source as [MapterhornTerrain] hillshade: online CDN
 * WebP, local PMTiles via [pmtilesGetTile], or loopback PNG from
 * [LocalDemTileServer]. All paths decode through [DemElevationGridDecoder]
 * (terrarium RGB) so interval/LOD logic is identical online and offline.
 */
class DemTileFetcher(
    private val config: Config,
) {
    data class Config(
        /** Online Mapterhorn template or loopback PNG template. */
        val tileTemplate: String?,
        /** Local `{region}_dem.pmtiles` when offline; preferred over HTTP. */
        val localDemFile: File? = null,
    ) {
        companion object {
            fun online(): Config = Config(tileTemplate = "https://tiles.mapterhorn.com/{z}/{x}/{y}.webp")

            fun fromResolved(
                context: android.content.Context,
                resolved: BasemapStyleResolver.ResolvedStyle,
            ): Config? {
                resolved.demSourceUri?.let { uri ->
                    return fromDemUri(uri, resolved)
                }
                resolved.coveringJob?.localPath?.let { basemapPath ->
                    MapterhornTerrain.localDemBesideBasemap(basemapPath)?.let { dem ->
                        MapterhornTerrain.ensureLocalDemTileJsonUrl(dem)
                        val template = LocalDemTileServer.activeTileTemplate()
                        return Config(tileTemplate = template, localDemFile = dem)
                    }
                }
                if (BasemapStyleResolver.hasNetwork(context)) {
                    return online()
                }
                return null
            }

            private fun fromDemUri(
                uri: String,
                resolved: BasemapStyleResolver.ResolvedStyle,
            ): Config {
                val localDem =
                    resolved.coveringJob?.localPath?.let { path ->
                        MapterhornTerrain.localDemBesideBasemap(path)
                    }
                when {
                    uri.startsWith("pmtiles://") && localDem != null -> {
                        MapterhornTerrain.ensureLocalDemTileJsonUrl(localDem)
                        return Config(tileTemplate = LocalDemTileServer.activeTileTemplate(), localDemFile = localDem)
                    }
                    uri.contains("127.0.0.1") -> {
                        localDem?.let { MapterhornTerrain.ensureLocalDemTileJsonUrl(it) }
                        val template = LocalDemTileServer.activeTileTemplate()
                        return Config(tileTemplate = template, localDemFile = localDem)
                    }
                    else -> return Config(tileTemplate = MapterhornTerrain.ONLINE_TILE_TEMPLATE, localDemFile = localDem)
                }
            }
        }
    }

    companion object {
        private const val DEM_GRID_CACHE_MAX = 64
        private val demGridCache =
            object : LinkedHashMap<String, DemElevationGrid>(DEM_GRID_CACHE_MAX, 0.75f, true) {
                override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, DemElevationGrid>): Boolean = size > DEM_GRID_CACHE_MAX
            }
        private val demGridLock = Any()

        fun clearDemGridCache() {
            synchronized(demGridLock) { demGridCache.clear() }
        }

        private fun demKey(
            z: Int,
            x: Int,
            y: Int,
        ): String = "$z/$x/$y"

        /** Shared zoom→sample grid dimension (online and offline). */
        fun sampleDimForZoom(zoom: Int): Int =
            when {
                zoom <= 9 -> 48
                zoom <= 10 -> 64
                zoom <= 11 -> 96
                zoom <= 13 -> 128
                else -> 160
            }
    }

    /** Load a full 512x512 terrarium tile at DEM zoom (max 12), with LRU cache. */
    fun fetchTile(
        z: Int,
        x: Int,
        y: Int,
    ): DemElevationGrid? {
        val demZ = min(z, 12)
        val n = 1 shl demZ
        val tx = x.coerceIn(0, n - 1)
        val ty = y.coerceIn(0, n - 1)
        val key = demKey(demZ, tx, ty)
        synchronized(demGridLock) {
            demGridCache[key]?.let {
                NaviMapTestHooks.contourDemCacheHits++
                return it
            }
        }
        NaviMapTestHooks.contourDemCacheMiss++
        val bounds = DemTileBounds.xyz(demZ, tx, ty)
        val bytes = fetchTerrariumBytes(demZ, tx, ty) ?: return null
        val grid = DemElevationGridDecoder.fromTerrariumBytes(bytes, bounds) ?: return null
        synchronized(demGridLock) { demGridCache[key] = grid }
        return grid
    }

    /**
     * Build an elevation grid for [bounds] at camera [zoomLevel].
     *
     * Stitches all intersecting DEM tiles before resampling so marching squares
     * does not split at DEM tile seams (a common artifact when each viewport
     * tile picked a different parent DEM cell).
     */
    fun gridForBounds(
        bounds: DemTileLatLngBounds,
        zoomLevel: Int,
    ): DemElevationGrid? {
        val demZ = min(max(zoomLevel, 0), 12)
        val demTiles = DemTileCover.tilesIntersecting(bounds, demZ)
        val loaded =
            demTiles.mapNotNull { (tx, ty) ->
                fetchTile(demZ, tx, ty)
            }
        if (loaded.isEmpty()) return null
        val mosaic = DemElevationGridDecoder.stitch(loaded) ?: return null
        val maxDim = sampleDimForZoom(zoomLevel)
        return DemElevationGridDecoder.resample(mosaic, bounds, maxDim)
    }

    private fun fetchTerrariumBytes(
        z: Int,
        x: Int,
        y: Int,
    ): ByteArray? {
        config.localDemFile?.let { dem ->
            val raw =
                runCatching {
                    pmtilesGetTile(dem.absolutePath, z.toUByte(), x.toUInt(), y.toUInt())
                }.getOrNull()
            if (raw != null && raw.isNotEmpty()) return raw
        }
        val template = config.tileTemplate ?: return null
        val url =
            template
                .replace("{z}", z.toString())
                .replace("{x}", x.toString())
                .replace("{y}", y.toString())
        return httpGet(url)
    }

    private fun httpGet(url: String): ByteArray? {
        val conn =
            (URL(url).openConnection() as HttpURLConnection).apply {
                connectTimeout = 8_000
                readTimeout = 12_000
                instanceFollowRedirects = true
            }
        return try {
            conn.inputStream.use { it.readBytes() }.takeIf { conn.responseCode == 200 }
        } catch (_: Exception) {
            null
        } finally {
            conn.disconnect()
        }
    }
}
