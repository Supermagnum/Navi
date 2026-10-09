package no.navi.app

import android.util.Log
import uniffi.navi.pmtilesGetTile
import java.io.BufferedOutputStream
import java.io.BufferedReader
import java.io.InputStreamReader
import java.io.OutputStream
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference
import kotlin.math.PI
import kotlin.math.atan
import kotlin.math.exp

/**
 * Loopback HTTP server that presents every usable offline vector archive as one
 * MapLibre source. TileJSON bounds are the world, so a viewport larger than any
 * one extract still requests tiles; the server returns the first archive that
 * has that z/x/y. Missing tiles are 204 so an online style underneath can show
 * through.
 */
object LocalVectorPmtilesServer {
    private const val TAG = "LocalVectorPmtiles"

    data class Archive(
        val path: String,
        val minLat: Double,
        val minLon: Double,
        val maxLat: Double,
        val maxLon: Double,
    )

    private val lock = Any()
    private val running = AtomicBoolean(false)
    private val archives = AtomicReference<List<Archive>>(emptyList())
    private var serverSocket: ServerSocket? = null
    private var acceptThread: Thread? = null
    private val pool =
        Executors.newCachedThreadPool { r ->
            Thread(r, "local-vector-tile").apply { isDaemon = true }
        }

    @Volatile
    private var boundPort: Int = -1

    @Volatile
    var hitsOk: Long = 0
        private set

    @Volatile
    var hitsMiss: Long = 0
        private set

    fun ensureServing(files: List<Archive>): String {
        require(files.isNotEmpty()) { "no vector archives" }
        val key = files.joinToString("|") { it.path }
        synchronized(lock) {
            val current = archives.get().joinToString("|") { it.path }
            if (running.get() && boundPort > 0 && current == key) {
                return tileTemplate(boundPort)
            }
            stopLocked()
            archives.set(files)
            val ss = ServerSocket(0, 64, InetAddress.getByName("127.0.0.1"))
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
                }, "local-vector-accept").apply {
                    isDaemon = true
                    start()
                }
            Log.i(TAG, "serving ${files.size} archives on 127.0.0.1:$boundPort")
            return tileTemplate(boundPort)
        }
    }

    fun tileJsonUrl(): String? {
        val port = boundPort
        if (!running.get() || port <= 0) return null
        return "http://127.0.0.1:$port/tilejson.json"
    }

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
        archives.set(emptyList())
    }

    private fun tileTemplate(port: Int): String = "http://127.0.0.1:$port/{z}/{x}/{y}.pbf"

    private fun handleClient(socket: Socket) {
        try {
            socket.use { sock ->
                sock.soTimeout = 15_000
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
                val match = Regex("""^/(\d+)/(\d+)/(\d+)(?:\.pbf)?$""").matchEntire(path)
                if (match == null) {
                    writeResponse(out, 404, "text/plain", "not found".toByteArray())
                    return
                }
                val z = match.groupValues[1].toIntOrNull()
                val x = match.groupValues[2].toLongOrNull()
                val y = match.groupValues[3].toLongOrNull()
                if (z == null || x == null || y == null || z !in 0..22) {
                    writeResponse(out, 400, "text/plain", "bad request".toByteArray())
                    return
                }
                val bytes = readTile(z, x, y)
                if (bytes == null || bytes.isEmpty()) {
                    hitsMiss++
                    writeResponse(out, 204, "application/octet-stream", ByteArray(0))
                    return
                }
                hitsOk++
                writeResponse(out, 200, "application/vnd.mapbox-vector-tile", bytes)
            }
        } catch (e: Exception) {
            Log.d(TAG, "client gone: ${e.javaClass.simpleName}: ${e.message}")
        }
    }

    /**
     * Prefer the archive whose bbox contains the tile centre; then any other
     * archive that intersects the tile. First non-empty [pmtilesGetTile] wins.
     */
    internal fun orderedArchivesForTile(
        files: List<Archive>,
        z: Int,
        x: Long,
        y: Long,
    ): List<Archive> {
        val (south, west, north, east) = tileLatLonBounds(z, x, y)
        val clat = (south + north) / 2.0
        val clon = (west + east) / 2.0
        val contain = ArrayList<Archive>()
        val overlap = ArrayList<Archive>()
        for (a in files) {
            if (pointInBbox(clat, clon, a.minLat, a.minLon, a.maxLat, a.maxLon)) {
                contain.add(a)
            } else if (bboxIntersects(
                    south,
                    west,
                    north,
                    east,
                    a.minLat,
                    a.minLon,
                    a.maxLat,
                    a.maxLon,
                )
            ) {
                overlap.add(a)
            } else if (z <= 4) {
                // Low-zoom ancestor tiles in a regional extract cover far more
                // than the stored job bbox; still try them.
                overlap.add(a)
            }
        }
        contain.addAll(overlap)
        return contain
    }

    private fun readTile(
        z: Int,
        x: Long,
        y: Long,
    ): ByteArray? {
        val files = archives.get()
        if (files.isEmpty()) return null
        for (a in orderedArchivesForTile(files, z, x, y)) {
            val got =
                runCatching {
                    pmtilesGetTile(a.path, z.toUByte(), x.toUInt(), y.toUInt())
                }.getOrNull()
            if (got != null && got.isNotEmpty()) return got
        }
        return null
    }

    private fun writeTileJson(out: OutputStream) {
        val port = boundPort
        val body =
            """
            {
              "tilejson": "3.0.0",
              "scheme": "xyz",
              "tiles": ["http://127.0.0.1:$port/{z}/{x}/{y}.pbf"],
              "attribution": "© OpenStreetMap © Protomaps",
              "bounds": [-180,-85.0511287,180,85.0511287],
              "center": [10,60,5],
              "minzoom": 0,
              "maxzoom": 15,
              "vector_layers": [{"id":"earth"},{"id":"water"},{"id":"landcover"},{"id":"landuse"},{"id":"roads"},{"id":"buildings"},{"id":"places"},{"id":"pois"}]
            }
            """.trimIndent().toByteArray(Charsets.UTF_8)
        writeResponse(out, 200, "application/json", body)
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
                204 -> "204 No Content"
                400 -> "400 Bad Request"
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
        if (body.isNotEmpty()) out.write(body)
        out.flush()
    }

    internal fun bboxIntersects(
        aSouth: Double,
        aWest: Double,
        aNorth: Double,
        aEast: Double,
        bSouth: Double,
        bWest: Double,
        bNorth: Double,
        bEast: Double,
    ): Boolean = aWest <= bEast && aEast >= bWest && aSouth <= bNorth && aNorth >= bSouth

    internal fun pointInBbox(
        lat: Double,
        lon: Double,
        minLat: Double,
        minLon: Double,
        maxLat: Double,
        maxLon: Double,
    ): Boolean = lat >= minLat && lat <= maxLat && lon >= minLon && lon <= maxLon

    internal fun tileLatLonBounds(
        z: Int,
        x: Long,
        y: Long,
    ): DoubleArray {
        val n = 1 shl z.coerceIn(0, 22)
        val west = x.toDouble() / n * 360.0 - 180.0
        val east = (x + 1).toDouble() / n * 360.0 - 180.0
        val north = mercatorYToLat(y.toDouble() / n)
        val south = mercatorYToLat((y + 1).toDouble() / n)
        return doubleArrayOf(south, west, north, east)
    }

    private fun mercatorYToLat(y: Double): Double {
        val n = PI - 2.0 * PI * y
        return Math.toDegrees(atan(sinh(n)))
    }

    private fun sinh(x: Double): Double = (exp(x) - exp(-x)) / 2.0
}
