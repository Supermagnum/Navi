package no.navi.app

import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.Style
import org.maplibre.android.style.sources.VectorSource
import java.io.File

/**
 * Follow-up 49 measurement only. Default-off debug switches and style/event
 * dumps. Does not change product behaviour unless a debug extra is set.
 */
object Fu49MapDiag {
    const val TAG_STYLE = "NaviFu49Style"
    const val TAG_EVENT = "NaviFu49Event"
    const val TAG_NET = "NaviFu49Net"
    const val TAG_TOTALS = "NaviMapTotals"

    @Volatile
    var forceOffline: Boolean = false

    /** One regional archive, no overview / online / second archive. */
    @Volatile
    var simpleMount: Boolean = false

    @Volatile
    var enableOverview: Boolean = false

    @Volatile
    var enableSecondRegional: Boolean = false

    @Volatile
    var enableOnline: Boolean = false

    @Volatile
    var bypassStyleQueue: Boolean = false

    @Volatile
    var disableKeepPrevious: Boolean = false

    /** Measurement only: ignore sameUri and call setStyle. */
    @Volatile
    var forceSetStyle: Boolean = false

    @Volatile
    var cameraEpochMs: Long = 0L

    @Volatile
    var lastIdleElapsedMs: Long = -1L

    @Volatile
    var lastFullyElapsedMs: Long = -1L

    @Volatile
    var lastStyleLoadElapsedMs: Long = -1L

    @Volatile
    var tileStarts: Int = 0

    @Volatile
    var tileDone: Int = 0

    @Volatile
    var lastHttpAllowed: Boolean = true

    fun resetEvents() {
        cameraEpochMs = System.currentTimeMillis()
        lastIdleElapsedMs = -1L
        lastFullyElapsedMs = -1L
        lastStyleLoadElapsedMs = -1L
        tileStarts = 0
        tileDone = 0
    }

    fun markCamera() {
        cameraEpochMs = System.currentTimeMillis()
        lastIdleElapsedMs = -1L
        lastFullyElapsedMs = -1L
    }

    fun elapsed(): Long {
        val start = cameraEpochMs
        if (start <= 0L) return -1L
        return System.currentTimeMillis() - start
    }

    fun logEvent(kind: String, extra: String = "") {
        val ms = elapsed()
        when (kind) {
            "idle" -> lastIdleElapsedMs = ms
            "fully" -> lastFullyElapsedMs = ms
            "style_loaded" -> lastStyleLoadElapsedMs = ms
            "tile_start" -> tileStarts += 1
            "tile_done" -> tileDone += 1
        }
        Log.i(
            TAG_EVENT,
            "kind=$kind elapsed_ms=$ms tiles_start=$tileStarts tiles_done=$tileDone $extra",
        )
    }

    fun logNetworkDecision(
        airplane: Boolean,
        hasInternet: Boolean,
        usable: Boolean,
        forceOfflineNow: Boolean,
    ) {
        lastHttpAllowed = usable && !forceOfflineNow
        Log.i(
            TAG_NET,
            "airplane=$airplane has_internet=$hasInternet usable=$usable " +
                "force_offline=$forceOfflineNow http_allowed=$lastHttpAllowed",
        )
    }

    fun dumpGeneratedStyle(styleUri: String, mountedKey: String, sameUri: Boolean, reloadBase: Boolean) {
        val path = styleUri.removePrefix("file://")
        val file = File(path)
        val leaf = file.name
        Log.i(
            TAG_STYLE,
            "generated uri=$styleUri leaf=$leaf exists=${file.isFile} " +
                "bytes=${if (file.isFile) file.length() else -1} mountedKey=$mountedKey " +
                "sameUri=$sameUri reloadBase=$reloadBase",
        )
        if (!file.isFile) return
        runCatching { summarizeStyleJson(JSONObject(file.readText()), "generated") }
            .onFailure { Log.w(TAG_STYLE, "generated parse failed: ${it.message}") }
        copyBeside(file, File(file.parentFile, "fu49-generated-last.json"))
    }

    fun dumpLiveStyle(style: Style?, destDir: File, tag: String) {
        if (style == null) {
            Log.w(TAG_STYLE, "live tag=$tag style=null")
            return
        }
        val uri = runCatching { style.uri }.getOrNull().orEmpty()
        val raw = runCatching { style.json }.getOrNull().orEmpty()
        destDir.mkdirs()
        val out = File(destDir, "fu49-live-$tag.json")
        if (raw.isNotBlank()) {
            out.writeText(raw)
            runCatching { summarizeStyleJson(JSONObject(raw), "live-$tag") }
        } else {
            val fallback = JSONObject()
            fallback.put("uri", uri)
            fallback.put("tag", tag)
            val srcJson = JSONObject()
            for (src in style.sources.orEmpty()) {
                val vs = src as? VectorSource
                val one = JSONObject()
                one.put("id", src.id)
                one.put("url", vs?.url ?: vs?.uri ?: "")
                srcJson.put(src.id, one)
            }
            fallback.put("sources", srcJson)
            val layerIds = JSONArray()
            for (layer in style.layers.orEmpty()) {
                layerIds.put(layer.id)
            }
            fallback.put("layers", layerIds)
            out.writeText(fallback.toString())
            Log.i(
                TAG_STYLE,
                "live tag=$tag uri=$uri sources=${style.sources.orEmpty().map { it.id }} " +
                    "layer_count=${style.layers.orEmpty().size} (no style.json)",
            )
        }
        Log.i(TAG_STYLE, "live_file=${out.absolutePath} bytes=${out.length()} uri=$uri")
    }

    fun summarizeStyleJson(json: JSONObject, tag: String) {
        val sources = json.optJSONObject("sources") ?: JSONObject()
        val names = sources.keys().asSequence().toList().sorted()
        val parts = ArrayList<String>()
        for (name in names) {
            val src = sources.optJSONObject(name) ?: continue
            parts.add(
                "$name:min=${src.opt("minzoom")}:max=${src.opt("maxzoom")}:url=${src.optString("url")}",
            )
        }
        val layers = json.optJSONArray("layers") ?: JSONArray()
        val order = ArrayList<String>()
        for (i in 0 until layers.length()) {
            val layer = layers.optJSONObject(i) ?: continue
            order.add("${layer.optString("id")}>${layer.optString("source")}")
        }
        Log.i(
            TAG_STYLE,
            "file tag=$tag sources=${parts.joinToString(",")} " +
                "layer_count=${layers.length()} order=${order.joinToString(",")}",
        )
    }

    fun logTotals(roads: Int, water: Int, labels: Int, blankCells: Int, phase: String) {
        Log.i(
            TAG_TOTALS,
            "phase=$phase roads=$roads water=$water labels=$labels " +
                "blank_cells=$blankCells elapsed_ms=${elapsed()} " +
                "idle_ms=$lastIdleElapsedMs fully_ms=$lastFullyElapsedMs",
        )
    }

    private fun copyBeside(from: File, to: File) {
        runCatching {
            to.writeText(from.readText())
        }
    }
}
