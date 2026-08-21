package no.navi.app

import android.os.Debug
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.liveHazardConeChildrenWarningJson
import uniffi.navi.liveHazardConeM
import uniffi.navi.liveHazardConeRoadSignWarningJson
import uniffi.navi.liveHazardConeSpeedCameraWarningJson
import uniffi.navi.liveHazardsIngestFromJson
import java.io.File

/**
 * On-device overhead for the compact live hazard cone on SM-P613 / Pixel.
 *
 * Prefer host-extracted compact JSON under live_hazards_cache/ (avoids multi-pass
 * Ostlandet PBF scans that SIGKILL low-RAM tablets). Measures UniFFI tick cost
 * after a single ingest into the native point store.
 */
@RunWith(AndroidJUnit4::class)
class LiveHazardConeOverheadInstrumentedTest {
    private companion object {
        const val TAG = "LiveHazardConeOverhead"
        const val QUERY_LAT = 60.68080462520444
        const val QUERY_LON = 11.34538019366088
        const val HEADING = 160.0
        const val ITERS = 80
        const val PBF_STEM = "ostlandet-latest.osm"
    }

    private fun dataDir(): File = NaviAppData.resolve(InstrumentationRegistry.getInstrumentation().targetContext)

    private fun cacheDir(): File? {
        val candidates =
            listOf(
                File(dataDir(), "live_hazards_cache/$PBF_STEM"),
                File("/data/local/tmp/navi_fixtures/live_hazards_cache/$PBF_STEM"),
            )
        return candidates.firstOrNull { dir ->
            listOf("signs.json", "cameras.json", "children.json", "bumps.json")
                .all { File(dir, it).isFile && File(dir, it).length() > 2 }
        }
    }

    private fun memLine(label: String): String {
        val mi = Debug.MemoryInfo()
        Debug.getMemoryInfo(mi)
        val rt = Runtime.getRuntime()
        return "$label pss_kib=${mi.totalPss} native_pss_kib=${mi.nativePss} dalvik_pss_kib=${mi.dalvikPss} " +
            "java_heap_used_kib=${(rt.totalMemory() - rt.freeMemory()) / 1024} " +
            "java_heap_total_kib=${rt.totalMemory() / 1024}"
    }

    private fun meanMs(
        iters: Int,
        block: () -> Unit,
    ): Double {
        repeat(5) { block() }
        val t0 = System.nanoTime()
        repeat(iters) { block() }
        return (System.nanoTime() - t0) / 1_000_000.0 / iters
    }

    @Test
    fun measure_compact_cone_vs_naive_json_tick() {
        val cache =
            cacheDir()
                ?: error(
                    "need live_hazards_cache/$PBF_STEM/{signs,cameras,children,bumps}.json " +
                        "(host: cargo run -p navi-ffi --bin live-hazard-extract --release -- <pbf> <out>)",
                )
        val signs = File(cache, "signs.json").readText()
        val cameras = File(cache, "cameras.json").readText()
        val schools = File(cache, "children.json").readText()
        val bumps = File(cache, "bumps.json").readText()
        val layerUtf8 = signs.length.toLong() + cameras.length + schools.length + bumps.length
        Log.i(TAG, "device model=${android.os.Build.MODEL}")
        Log.i(TAG, "cache=${cache.absolutePath} layer_utf8=$layerUtf8")
        Log.i(TAG, "cone_m=${liveHazardConeM()}")
        Log.i(TAG, memLine("BEFORE_INGEST"))

        val t0 = System.nanoTime()
        val stats =
            liveHazardsIngestFromJson(
                "cache:$PBF_STEM",
                signs,
                cameras,
                schools,
                bumps,
            )
        val ingestMs = (System.nanoTime() - t0) / 1_000_000.0
        Log.i(
            TAG,
            "COMPACT_LOAD signs=${stats.signs} children=${stats.children} cameras=${stats.cameras} " +
                "bumps=${stats.bumps} compact_utf8=${stats.compactJsonUtf8} cone_m=${stats.coneM} " +
                "ingest_ms=${"%.1f".format(ingestMs)} layer_utf8=$layerUtf8",
        )
        Log.i(TAG, memLine("AFTER_INGEST"))
        assertTrue("signs indexed", stats.signs > 0u)
        assertTrue("children centroids", stats.children > 0u && stats.children < 25_000u)
        assertTrue("bumps indexed", stats.bumps > 0u)
        assertTrue(kotlin.math.abs(stats.coneM - 300.0) < 0.01)
        assertTrue("compact layers should be ~MB-scale", layerUtf8 in 100_000L..8_000_000L)

        // Point-set cone queries only (speed-limit graph warm is separate and can OOM
        // on low-RAM tablets during this measurement pass).
        val compactTickMs =
            meanMs(ITERS) {
                liveHazardConeRoadSignWarningJson(QUERY_LAT, QUERY_LON, HEADING)
                liveHazardConeChildrenWarningJson(QUERY_LAT, QUERY_LON, HEADING)
                liveHazardConeSpeedCameraWarningJson(QUERY_LAT, QUERY_LON, HEADING, true)
            }
        Log.i(TAG, "COMPACT_TICK_ms_mean=${"%.3f".format(compactTickMs)} iters=$ITERS")
        Log.i(TAG, memLine("AFTER_COMPACT_TICK"))
        assertTrue(
            "compact tick too slow: ${compactTickMs}ms (expected << 50ms naive JSON path)",
            compactTickMs < 25.0,
        )
        Log.i(
            TAG,
            "VERDICT compact_ms=${"%.3f".format(compactTickMs)} " +
                "compact_utf8_est=${stats.compactJsonUtf8} layer_utf8=$layerUtf8 cone_m=${stats.coneM}",
        )
    }

    @Test
    fun cone_categories_emit_icon_keys_and_rasterize() {
        val cache =
            cacheDir()
                ?: error("need live_hazards_cache/$PBF_STEM for cone category probe")
        liveHazardsIngestFromJson(
            "cache:$PBF_STEM:icons",
            File(cache, "signs.json").readText(),
            File(cache, "cameras.json").readText(),
            File(cache, "children.json").readText(),
            File(cache, "bumps.json").readText(),
        )
        val sign = liveHazardConeRoadSignWarningJson(QUERY_LAT, QUERY_LON, HEADING)
        val children = liveHazardConeChildrenWarningJson(QUERY_LAT, QUERY_LON, HEADING)
        val camera = liveHazardConeSpeedCameraWarningJson(QUERY_LAT, QUERY_LON, HEADING, true)
        Log.i(TAG, "cone_sign=$sign")
        Log.i(TAG, "cone_children=$children")
        Log.i(TAG, "cone_camera=$camera")
        assertTrue(
            "expected children or tagged sign near Vallset: children=$children sign=$sign",
            children.contains("icon_key") || sign.contains("icon_key"),
        )
        if (children.contains("icon_key")) {
            assertTrue(
                "children use 142 plate: $children",
                children.contains("no_sign_142") || children.contains("\"code\":\"142\""),
            )
        }
        if (sign.contains("\"code\":\"109\"")) {
            assertTrue("bump uses 109 plate: $sign", sign.contains("no_sign_109"))
        }

        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val iconsDir = File(ctx.filesDir, "icons-cone-category-test").also { it.mkdirs() }

        fun copyAsset(
            assetPath: String,
            dest: File,
        ) {
            dest.parentFile?.mkdirs()
            ctx.assets.open(assetPath).use { input ->
                dest.outputStream().use { input.copyTo(it) }
            }
        }
        copyAsset("icons/speed_camera.svg", File(iconsDir, "speed_camera.svg"))
        for (key in listOf("no_sign_109", "no_sign_142", "no_sign_362_20")) {
            copyAsset("icons/road-signs/$key.svg", File(iconsDir, "road-signs/$key.svg"))
        }
        for (key in listOf("no_sign_109", "no_sign_142", "speed_camera", "no_sign_362_20")) {
            val png =
                uniffi.navi.rasterizeIconPng(
                    key = key,
                    theme = uniffi.navi.FfiIconTheme.DAY,
                    width = 96u,
                    height = 96u,
                    bundledDir = iconsDir.absolutePath,
                )
            assertTrue("$key empty png", png.isNotEmpty())
            Log.i(TAG, "raster_ok key=$key bytes=${png.size}")
        }

        val cams = org.json.JSONArray(File(cache, "cameras.json").readText())
        assertTrue("Ostlandet cache should include cameras", cams.length() > 0)
        var sawCameraWarn = false
        for (i in 0 until minOf(cams.length(), 80)) {
            val o = cams.getJSONObject(i)
            val lat = o.getDouble("lat")
            val lon = o.getDouble("lon")
            val qLat = lat + (150.0 / 111_320.0)
            val warn = liveHazardConeSpeedCameraWarningJson(qLat, lon, 180.0, true)
            if (warn != "{}" && warn.isNotBlank()) {
                Log.i(TAG, "camera_warn=$warn at $lat,$lon")
                sawCameraWarn = true
                break
            }
        }
        assertTrue("expected at least one camera cone warning in region sample", sawCameraWarn)
    }
}
