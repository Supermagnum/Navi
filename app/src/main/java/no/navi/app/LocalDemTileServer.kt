package no.navi.app

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ImageDecoder
import android.os.Build
import android.util.Log
import uniffi.navi.pmtilesGetTile
import java.io.BufferedOutputStream
import java.io.BufferedReader
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.InputStreamReader
import java.io.OutputStream
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.nio.ByteBuffer
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import kotlin.math.abs

/**
 * Loopback HTTP server for local Mapterhorn DEM PMTiles.
 *
 * Serves terrarium from the local PMTiles extract at
 * `http://127.0.0.1:PORT/{z}/{x}/{y}.png` (512×512 terrarium RGB in lossless PNG;
 * PMTiles stores WebP). TileJSON mirrors Mapterhorn CDN shape
 * (`encoding` terrarium, `scheme` xyz). Mapbox Terrain-RGB conversion remains available
 * for diagnostics via [NaviMapTestHooks.localDemMapboxConversion].
 */
object LocalDemTileServer {
    private const val TAG = "LocalDemTileServer"
    private const val CACHE_MAX = 96
    const val TILE_SIZE = 512
    private const val ROUNDTRIP_MAX_ABS_ERROR_M = 1.0

    private val lock = Any()
    private val running = AtomicBoolean(false)
    private val demPath = AtomicReference<String?>(null)
    private var serverSocket: ServerSocket? = null
    private var acceptThread: Thread? = null
    private val pool =
        Executors.newCachedThreadPool { r ->
            Thread(r, "local-dem-tile").apply { isDaemon = true }
        }
    private val tileCache = linkedMapOf<String, CachedTile>()

    private val logDecodeDims = AtomicInteger(0)
    private val verifyFirstTile = AtomicBoolean(true)
    private val lastServeRaw = AtomicBoolean(false)

    @Volatile
    private var boundPort: Int = -1

    @Volatile
    var hitsOk: Long = 0
        private set

    @Volatile
    var hitsMiss: Long = 0
        private set

    /** Last served tile terrarium elev range (meters), for tests/diagnostics. */
    @Volatile
    var lastElevMin: Double = Double.NaN
        private set

    @Volatile
    var lastElevMax: Double = Double.NaN
        private set

    @Volatile
    var lastDecodedWidth: Int = -1
        private set

    @Volatile
    var lastDecodedHeight: Int = -1
        private set

    /** Max |elev| error after mapbox pack on first verified tile (meters). */
    @Volatile
    var lastRoundtripMaxError: Double = Double.NaN
        private set

    /** Max |elev| error after PNG compress + decode on first verified tile (meters). */
    @Volatile
    var lastPngRoundtripMaxError: Double = Double.NaN
        private set

    fun resetHitCounts() {
        hitsOk = 0
        hitsMiss = 0
        NaviMapTestHooks.localDemHitsOk = 0
        NaviMapTestHooks.localDemHitsMiss = 0
    }

    /** Default: pass-through terrarium WebP (mirrors online Mapterhorn). */
    private fun serveRawTerrarium(): Boolean = !NaviMapTestHooks.localDemMapboxConversion

    /** `http://127.0.0.1:PORT/{z}/{x}/{y}.png` or `.webp` when serving raw terrarium. */
    fun ensureServing(demFile: File): String {
        require(demFile.isFile) { "DEM missing: ${demFile.absolutePath}" }
        synchronized(lock) {
            val path = demFile.absolutePath
            val raw = serveRawTerrarium()
            if (running.get() && demPath.get() == path && boundPort > 0 && lastServeRaw.get() == raw) {
                return tileTemplate(boundPort)
            }
            stopLocked()
            demPath.set(path)
            lastServeRaw.set(raw)
            synchronized(tileCache) { tileCache.clear() }
            verifyFirstTile.set(true)
            logDecodeDims.set(0)
            val ss = ServerSocket(0, 32, InetAddress.getByName("127.0.0.1"))
            serverSocket = ss
            boundPort = ss.localPort
            running.set(true)
            acceptThread =
                Thread({
                    while (running.get()) {
                        try {
                            val client = ss.accept()
                            pool.execute { handleClient(client) }
                        } catch (_: Exception) {
                            if (!running.get()) break
                        }
                    }
                }, "local-dem-accept").apply {
                    isDaemon = true
                    start()
                }
            Log.i(
                TAG,
                "serving ${demFile.name} as " +
                    (if (serveRawTerrarium()) "terrarium PNG (from WebP)" else "mapbox PNG") +
                    " on 127.0.0.1:$boundPort",
            )
            return tileTemplate(boundPort)
        }
    }

    /** TileJSON for the bound server (encoding declared for Native). */
    fun tileJsonUrl(): String? {
        val port = boundPort
        if (!running.get() || port <= 0) return null
        return "http://127.0.0.1:$port/tilejson.json"
    }

    /** Loopback `{z}/{x}/{y}.webp` template when serving; null if server is down. */
    fun activeTileTemplate(): String? {
        val port = boundPort
        if (!running.get() || port <= 0) return null
        return tileTemplate(port)
    }

    fun stop() {
        synchronized(lock) { stopLocked() }
    }

    private fun stopLocked() {
        running.set(false)
        runCatching { serverSocket?.close() }
        serverSocket = null
        acceptThread = null
        boundPort = -1
        demPath.set(null)
        synchronized(tileCache) { tileCache.clear() }
        verifyFirstTile.set(true)
        logDecodeDims.set(0)
    }

    private fun tileTemplate(port: Int): String =
        if (serveRawTerrarium()) {
            // Loopback: terrarium RGB in lossless PNG (SM-P613 Native skips 127.0.0.1 WebP DEM).
            "http://127.0.0.1:$port/{z}/{x}/{y}.png"
        } else {
            "http://127.0.0.1:$port/{z}/{x}/{y}.png"
        }

    private fun handleClient(socket: Socket) {
        try {
            socket.use { sock ->
                sock.soTimeout = 20_000
                val reader = BufferedReader(InputStreamReader(sock.getInputStream()))
                val requestLine = reader.readLine() ?: return
                while (true) {
                    val line = reader.readLine() ?: break
                    if (line.isEmpty()) break
                }
                val path =
                    requestLine
                        .substringAfter(' ', "")
                        .substringBefore(' ')
                        .trim()
                        .substringBefore('?')
                val out = BufferedOutputStream(sock.getOutputStream())
                if (path == "/tilejson.json" || path == "/tilejson") {
                    writeTileJson(out)
                    return
                }
                val match =
                    Regex("""^/(\d+)/(\d+)/(\d+)(?:\.(?:webp|png))?$""")
                        .matchEntire(path)
                if (match == null) {
                    writeResponse(out, 404, "text/plain", "not found".toByteArray())
                    return
                }
                val z = match.groupValues[1].toIntOrNull()?.toUByte()
                val x = match.groupValues[2].toLongOrNull()
                val y = match.groupValues[3].toLongOrNull()
                val file = demPath.get()
                if (z == null || x == null || y == null || file == null) {
                    writeResponse(out, 400, "text/plain", "bad request".toByteArray())
                    return
                }
                if (x > UInt.MAX_VALUE.toLong() || y > UInt.MAX_VALUE.toLong()) {
                    writeResponse(out, 400, "text/plain", "bad request".toByteArray())
                    return
                }
                val cacheKey = "$z/$x/$y"
                val cached = synchronized(tileCache) { tileCache[cacheKey] }
                if (cached != null) {
                    hitsOk++
                    syncHitHooks()
                    recordElevFromTile(z.toInt(), cached.elevMin, cached.elevMax)
                    writeResponse(out, 200, cached.contentType, cached.bytes)
                    return
                }
                val terrarium =
                    runCatching { pmtilesGetTile(file, z, x.toUInt(), y.toUInt()) }
                        .getOrNull()
                if (terrarium == null || terrarium.isEmpty()) {
                    hitsMiss++
                    syncHitHooks()
                    writeResponse(out, 404, "text/plain", "missing tile".toByteArray())
                    return
                }
                val converted =
                    if (serveRawTerrarium()) {
                        runCatching { rawTerrariumPngTile(terrarium) }
                    } else {
                        runCatching { terrariumWebpToMapboxPng(terrarium) }
                    }
                        .getOrElse {
                            Log.w(TAG, "convert failed z=$z x=$x y=$y: ${it.message}")
                            hitsMiss++
                            writeResponse(out, 500, "text/plain", "convert failed".toByteArray())
                            return
                        }
                synchronized(tileCache) {
                    if (tileCache.size >= CACHE_MAX) {
                        tileCache.keys.firstOrNull()?.let { tileCache.remove(it) }
                    }
                    tileCache[cacheKey] =
                        CachedTile(
                            converted.bytes,
                            converted.contentType,
                            converted.elevMin,
                            converted.elevMax,
                            z.toInt(),
                        )
                }
                hitsOk++
                syncHitHooks()
                recordElevFromTile(z.toInt(), converted.elevMin, converted.elevMax)
                // Prefer z>=10 so diagnostics reflect the camera DEM, not z0 world.
                val sampleElev = z.toInt() >= 10 && (hitsOk == 1L || hitsOk % 10L == 0L)
                if (sampleElev) {
                    lastElevMin = converted.elevMin
                    lastElevMax = converted.elevMax
                }
                if (hitsOk == 1L || hitsOk % 25L == 0L || sampleElev) {
                    Log.i(
                        TAG,
                        "tile ok z=$z x=$x y=$y bytes=${converted.bytes.size} " +
                            "elevMin=${lastElevMin} elevMax=${lastElevMax} " +
                            "hitsOk=$hitsOk hitsMiss=$hitsMiss",
                    )
                }
                writeResponse(out, 200, converted.contentType, converted.bytes)
            }
        } catch (e: Exception) {
            Log.d(TAG, "client gone: ${e.javaClass.simpleName}: ${e.message}")
        }
    }

    private fun writeTileJson(out: OutputStream) {
        val port = boundPort
        val ext = "png"
        val encoding = if (serveRawTerrarium()) "terrarium" else "mapbox"
        val attribution = MapterhornTerrain.ATTRIBUTION
        val body =
            """
            {
              "tilejson": "3.0.0",
              "scheme": "xyz",
              "tiles": ["http://127.0.0.1:$port/{z}/{x}/{y}.$ext"],
              "attribution": "$attribution",
              "bounds": [-180,-85.0511287,180,85.0511287],
              "center": [0,0,6],
              "encoding": "$encoding",
              "tileSize": 512,
              "minzoom": 0,
              "maxzoom": 12
            }
            """.trimIndent().toByteArray(Charsets.UTF_8)
        writeResponse(out, 200, "application/json", body)
    }

    fun terrariumElevM(
        r: Int,
        g: Int,
        b: Int,
    ): Double = r * 256.0 + g + b / 256.0 - 32768.0

    fun mapboxElevM(
        r: Int,
        g: Int,
        b: Int,
    ): Double = -10000.0 + (r * 65536.0 + g * 256.0 + b) * 0.1

    private fun bitmapFactoryNoScaleOpts(): BitmapFactory.Options =
        BitmapFactory.Options().apply {
            inPreferredConfig = Bitmap.Config.ARGB_8888
            inScaled = false
            @Suppress("DEPRECATION")
            inDensity = 0
            @Suppress("DEPRECATION")
            inTargetDensity = 0
        }

    private fun decodeTerrariumWebp(webpBytes: ByteArray): Bitmap {
        var decoded =
            BitmapFactory.decodeByteArray(webpBytes, 0, webpBytes.size, bitmapFactoryNoScaleOpts())
        if (decoded == null) {
            error("webp decode failed (${webpBytes.size} bytes)")
        }
        if (decoded.width != TILE_SIZE || decoded.height != TILE_SIZE) {
            if (Build.VERSION.SDK_INT >= 28) {
                decoded.recycle()
                val source = ImageDecoder.createSource(ByteBuffer.wrap(webpBytes))
                decoded =
                    ImageDecoder.decodeBitmap(source) { decoder, _, _ ->
                        decoder.setAllocator(ImageDecoder.ALLOCATOR_SOFTWARE)
                        decoder.isMutableRequired = true
                    }
            }
        }
        val logN = logDecodeDims.getAndIncrement()
        if (logN < 3) {
            lastDecodedWidth = decoded.width
            lastDecodedHeight = decoded.height
            Log.i(
                TAG,
                "decoded terrarium WebP ${decoded.width}x${decoded.height} " +
                    "(expect ${TILE_SIZE}x${TILE_SIZE})",
            )
        }
        if (decoded.width != TILE_SIZE || decoded.height != TILE_SIZE) {
            val w = decoded.width
            val h = decoded.height
            decoded.recycle()
            error(
                "DEM tile must be ${TILE_SIZE}x$TILE_SIZE, got ${w}x$h " +
                    "(density-scaled decode scrambles slopes)",
            )
        }
        return if (decoded.config == Bitmap.Config.ARGB_8888) {
            decoded
        } else {
            decoded.copy(Bitmap.Config.ARGB_8888, true).also { decoded.recycle() }
        }
    }

    /**
     * Terrarium (Mapzen): `h = R*256 + G + B/256 - 32768`
     * Kept for diagnostics / tests; production serving is converted Mapbox PNG.
     */
    fun terrariumElevRange(webpBytes: ByteArray): Pair<Double, Double> {
        val src = decodeTerrariumWebp(webpBytes)
        val w = src.width
        val h = src.height
        val pixels = IntArray(w * h)
        src.getPixels(pixels, 0, w, 0, 0, w, h)
        var elevMin = Double.POSITIVE_INFINITY
        var elevMax = Double.NEGATIVE_INFINITY
        for (c in pixels) {
            val r = (c ushr 16) and 0xff
            val g = (c ushr 8) and 0xff
            val b = c and 0xff
            val elev = terrariumElevM(r, g, b)
            if (elev < elevMin) elevMin = elev
            if (elev > elevMax) elevMax = elev
        }
        src.recycle()
        return elevMin to elevMax
    }

    private data class CachedTile(
        val bytes: ByteArray,
        val contentType: String,
        val elevMin: Double = Double.NaN,
        val elevMax: Double = Double.NaN,
        val zoom: Int = -1,
    )

    private fun recordElevFromTile(
        zoom: Int,
        elevMin: Double,
        elevMax: Double,
    ) {
        if (zoom < 10) return
        if (!elevMin.isNaN() && !elevMax.isNaN()) {
            lastElevMin = elevMin
            lastElevMax = elevMax
        }
    }

    /** Pass-through terrarium RGB from PMTiles WebP as lossless PNG (encoding still terrarium). */
    private fun rawTerrariumPngTile(webpBytes: ByteArray): ConvertedTile {
        val src = decodeTerrariumWebp(webpBytes)
        val w = src.width
        val h = src.height
        val pixels = IntArray(w * h)
        src.getPixels(pixels, 0, w, 0, 0, w, h)
        var elevMin = Double.POSITIVE_INFINITY
        var elevMax = Double.NEGATIVE_INFINITY
        for (c in pixels) {
            val r = (c ushr 16) and 0xff
            val g = (c ushr 8) and 0xff
            val b = c and 0xff
            val elev = terrariumElevM(r, g, b)
            if (elev < elevMin) elevMin = elev
            if (elev > elevMax) elevMax = elev
        }
        val baos = ByteArrayOutputStream(w * h * 2)
        check(src.compress(Bitmap.CompressFormat.PNG, 100, baos)) { "terrarium PNG compress failed" }
        src.recycle()
        return ConvertedTile(baos.toByteArray(), "image/png", elevMin, elevMax)
    }

    /** Pass-through terrarium WebP bytes (CDN parity; not used on loopback fetch path). */
    private fun rawTerrariumWebpTile(webpBytes: ByteArray): ConvertedTile {
        val range = terrariumElevRange(webpBytes)
        return ConvertedTile(webpBytes, "image/webp", range.first, range.second)
    }

    /** Terrarium WebP → Mapbox Terrain-RGB PNG (lossless RGB). */
    fun terrariumWebpToMapboxPng(webpBytes: ByteArray): ConvertedTile {
        val src = decodeTerrariumWebp(webpBytes)
        val w = src.width
        val h = src.height
        val pixels = IntArray(w * h)
        src.getPixels(pixels, 0, w, 0, 0, w, h)
        val verifyThisTile = verifyFirstTile.compareAndSet(true, false)
        val terrariumElevs = if (verifyThisTile) DoubleArray(pixels.size) else null
        var elevMin = Double.POSITIVE_INFINITY
        var elevMax = Double.NEGATIVE_INFINITY
        for (i in pixels.indices) {
            val c = pixels[i]
            val r = (c ushr 16) and 0xff
            val g = (c ushr 8) and 0xff
            val b = c and 0xff
            val elev = terrariumElevM(r, g, b)
            terrariumElevs?.set(i, elev)
            if (elev < elevMin) elevMin = elev
            if (elev > elevMax) elevMax = elev
            val v = ((elev + 10000.0) * 10.0).toLong().coerceIn(0L, 256L * 256L * 256L - 1L)
            val nr = ((v / 65536L) % 256L).toInt()
            val ng = ((v / 256L) % 256L).toInt()
            val nb = (v % 256L).toInt()
            pixels[i] = (0xff shl 24) or (nr shl 16) or (ng shl 8) or nb
        }
        if (verifyThisTile && terrariumElevs != null) {
            var maxErr = 0.0
            for (i in pixels.indices) {
                val c = pixels[i]
                val nr = (c ushr 16) and 0xff
                val ng = (c ushr 8) and 0xff
                val nb = c and 0xff
                val decoded = mapboxElevM(nr, ng, nb)
                maxErr = maxOf(maxErr, abs(decoded - terrariumElevs[i]))
            }
            lastRoundtripMaxError = maxErr
            Log.i(TAG, "mapbox pack round-trip maxAbsError=${maxErr}m")
            if (maxErr > ROUNDTRIP_MAX_ABS_ERROR_M) {
                src.recycle()
                error("mapbox pack round-trip maxAbsError ${maxErr}m > $ROUNDTRIP_MAX_ABS_ERROR_M m")
            }
        }
        val outBmp = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
        outBmp.setPixels(pixels, 0, w, 0, 0, w, h)
        val baos = ByteArrayOutputStream(w * h * 2)
        check(outBmp.compress(Bitmap.CompressFormat.PNG, 100, baos)) { "tile PNG compress failed" }
        src.recycle()
        outBmp.recycle()
        val pngBytes = baos.toByteArray()
        if (verifyThisTile && terrariumElevs != null) {
            val pngBmp =
                BitmapFactory.decodeByteArray(
                    pngBytes,
                    0,
                    pngBytes.size,
                    bitmapFactoryNoScaleOpts(),
                ) ?: error("PNG round-trip decode failed")
            val pw = pngBmp.width
            val ph = pngBmp.height
            if (pw != TILE_SIZE || ph != TILE_SIZE) {
                pngBmp.recycle()
                error("PNG decode size ${pw}x$ph != ${TILE_SIZE}x$TILE_SIZE")
            }
            val pngPixels = IntArray(pw * ph)
            pngBmp.getPixels(pngPixels, 0, pw, 0, 0, pw, ph)
            var maxErr = 0.0
            for (i in pngPixels.indices) {
                val c = pngPixels[i]
                val nr = (c ushr 16) and 0xff
                val ng = (c ushr 8) and 0xff
                val nb = c and 0xff
                val decoded = mapboxElevM(nr, ng, nb)
                maxErr = maxOf(maxErr, abs(decoded - terrariumElevs[i]))
            }
            pngBmp.recycle()
            lastPngRoundtripMaxError = maxErr
            Log.i(TAG, "PNG compress round-trip maxAbsError=${maxErr}m")
            if (maxErr > ROUNDTRIP_MAX_ABS_ERROR_M) {
                error("PNG round-trip maxAbsError ${maxErr}m > $ROUNDTRIP_MAX_ABS_ERROR_M m")
            }
        }
        return ConvertedTile(pngBytes, "image/png", elevMin, elevMax)
    }

    /** @deprecated use [terrariumWebpToMapboxPng]; kept for callers/tests. */
    fun terrariumWebpToMapboxWebp(webpBytes: ByteArray): ConvertedTile = terrariumWebpToMapboxPng(webpBytes)

    data class ConvertedTile(
        val bytes: ByteArray,
        val contentType: String,
        val elevMin: Double,
        val elevMax: Double,
    )

    private fun syncHitHooks() {
        NaviMapTestHooks.localDemHitsOk = hitsOk
        NaviMapTestHooks.localDemHitsMiss = hitsMiss
    }

    private fun writeResponse(
        out: OutputStream,
        code: Int,
        contentType: String,
        body: ByteArray,
    ) {
        val status =
            when (code) {
                200 -> "200 OK"
                400 -> "400 Bad Request"
                500 -> "500 Internal Server Error"
                else -> "404 Not Found"
            }
        val header =
            "HTTP/1.1 $status\r\n" +
                "Content-Type: $contentType\r\n" +
                "Content-Length: ${body.size}\r\n" +
                "Connection: close\r\n" +
                "Cache-Control: no-store\r\n" +
                "Access-Control-Allow-Origin: *\r\n" +
                "\r\n"
        out.write(header.toByteArray(Charsets.US_ASCII))
        out.write(body)
        out.flush()
    }
}
