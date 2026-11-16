package no.navi.app

import android.content.Context
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import java.io.File

/**
 * Shared helpers for the Phase 6 / 6b on-device acceptance (CoMaps drape).
 *
 * Everything runs inside the app under test (debug or releasetest) on the physical
 * device: map data comes from the app's own downloader into `{filesDir}/mwm` (nothing is
 * pushed with adb), screenshots go through the instrumentation UiAutomation + shell
 * `screencap` (no other apps), and the "location" comes from Navi's own route
 * simulation or camera hooks.
 */
object ComapsPhase6Harness {
    const val TAG = "Phase6"

    val context: Context get() = InstrumentationRegistry.getInstrumentation().targetContext

    /** adb-pullable output directory (shell can write it; app can read it). */
    val SHELL_DIR: String
        get() =
            InstrumentationRegistry.getArguments().getString("out_dir") ?: "/data/local/tmp/phase6b"

    /**
     * Stage the headless CoMaps resources (classificator, drules, countries, ...) into [root] the way the app does:
     * from the SDK's root assets on the drape build, from `assets/comaps-data` on an SDK-less build. The Phase 4 / 5
     * suites used to copy only `comaps-data`, which the drape build no longer packages.
     */
    fun stageHeadlessAssets(
        ctx: Context,
        root: File,
    ) {
        fun copy(
            assetPath: String,
            dest: File,
        ) {
            val children = ctx.assets.list(assetPath).orEmpty()
            if (children.isNotEmpty()) {
                dest.mkdirs()
                for (c in children) copy("$assetPath/$c", File(dest, c))
                return
            }
            val input = runCatching { ctx.assets.open(assetPath) }.getOrNull() ?: return
            dest.parentFile?.mkdirs()
            input.use { i -> dest.outputStream().use { o -> i.copyTo(o) } }
        }
        val legacy =
            ctx.assets
                .list("comaps-data")
                .orEmpty()
                .filter { it != "MANIFEST.txt" }
        if (legacy.isNotEmpty()) {
            for (n in legacy) copy("comaps-data/$n", File(root, n))
        } else {
            // The app stages the minimum for routing (ROOT_ASSETS); search also needs the ICU data
            // (icudt75l.dat: transliteration). AssetManager.list("") is not a reliable inventory of
            // every root file on all devices, so ICU is always named explicitly.
            val plain =
                ctx.assets
                    .list("")
                    .orEmpty()
                    .filter {
                        ctx.assets
                            .list(it)
                            .orEmpty()
                            .isEmpty() &&
                            !it.endsWith(".kotlin_module")
                    }
            for (n in (ComapsSessionBootstrap.ROOT_ASSETS + plain + listOf("countries-strings", "icudt75l.dat")).distinct()) {
                copy(n, File(root, n))
            }
        }
    }

    fun productionRoot(): File = File(NaviAppData.resolve(context), "mwm")

    fun shell(cmd: String): String {
        val pfd = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
        return java.io
            .FileInputStream(pfd.fileDescriptor)
            .use { it.readBytes().decodeToString() }
            .also { pfd.close() }
    }

    fun ensureShellDirs() {
        shell("mkdir -p $SHELL_DIR/screenshots $SHELL_DIR/logs")
        shell("chmod -R 777 $SHELL_DIR")
    }

    /** Settled full-display PNG under `/data/local/tmp/phase6/screenshots/<name>.png`. */
    fun screenshot(
        name: String,
        settleMs: Long = 25_000,
    ) {
        InstrumentedMapCapture.awaitRenderSettled(settleMs)
        Thread.sleep(600)
        val path = "$SHELL_DIR/screenshots/$name.png"
        shell("screencap -p $path")
        shell("chmod 644 $path")
        android.util.Log.i(
            TAG,
            "screenshot $name drape=${NaviMapTestHooks.drapeState} basemapDrape=${NaviMapTestHooks.drapeBasemapVisible} " +
                "cam=${NaviMapTestHooks.lastCameraLat},${NaviMapTestHooks.lastCameraLon} " +
                "z=${NaviMapTestHooks.lastCameraZoom}",
        )
    }

    fun waitFor(
        timeoutMs: Long,
        pollMs: Long = 250,
        cond: () -> Boolean,
    ): Boolean {
        val end = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < end) {
            if (cond()) return true
            Thread.sleep(pollMs)
        }
        return cond()
    }

    fun gotoCamera(
        lat: Double,
        lon: Double,
        zoom: Double,
        timeoutMs: Long = 20_000,
    ): Boolean {
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        return waitFor(timeoutMs) {
            if (kotlin.math.abs(NaviMapTestHooks.lastCameraLat - lat) > 0.01 ||
                kotlin.math.abs(NaviMapTestHooks.lastCameraLon - lon) > 0.01
            ) {
                NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
                false
            } else {
                kotlin.math.abs(NaviMapTestHooks.lastCameraZoom - zoom) < 0.2
            }
        }
    }

    /**
     * Periodic device sampler (memory, UI frame rate, drape surface frame rate, thermal,
     * battery) running inside the instrumentation process for the duration of a run.
     */
    class PerfSampler(
        private val intervalMs: Long = 15_000,
    ) {
        private val samples = org.json.JSONArray()

        @Volatile
        private var running = false
        private var thread: Thread? = null
        private var frames = 0L
        private var jank = 0L
        private var lastFrameNs = 0L
        private val choreo =
            object : android.view.Choreographer.FrameCallback {
                override fun doFrame(frameTimeNanos: Long) {
                    if (lastFrameNs != 0L) {
                        frames++
                        if (frameTimeNanos - lastFrameNs > JANK_NS) jank++
                    }
                    lastFrameNs = frameTimeNanos
                    if (running) {
                        android.view.Choreographer
                            .getInstance()
                            .postFrameCallback(this)
                    }
                }
            }

        fun start() {
            running = true
            android.os.Handler(android.os.Looper.getMainLooper()).post {
                android.view.Choreographer
                    .getInstance()
                    .postFrameCallback(choreo)
            }
            thread =
                Thread {
                    var winStart = System.nanoTime()
                    var f0 = frames
                    var j0 = jank
                    while (running) {
                        try {
                            Thread.sleep(intervalMs)
                        } catch (_: InterruptedException) {
                            break
                        }
                        val now = System.nanoTime()
                        val secs = (now - winStart) / 1e9
                        val uiFps = (frames - f0) / secs
                        val janky = jank - j0
                        winStart = now
                        f0 = frames
                        j0 = jank
                        runCatching {
                            val row = sample(uiFps, janky, 0)
                            samples.put(row)
                            // Also to logcat so a crash mid-run does not lose the series.
                            android.util.Log.i(TAG, "perf $row")
                        }.onFailure { android.util.Log.w(TAG, "perf sample failed: $it") }
                    }
                }.also {
                    it.isDaemon = true
                    it.start()
                }
        }

        fun stop(): org.json.JSONArray {
            running = false
            thread?.interrupt()
            return samples
        }

        private fun sample(
            uiFps: Double,
            janky: Long,
            @Suppress("UNUSED_PARAMETER") unused: Long,
        ): JSONObject {
            val pkg = context.packageName
            val mem = shell("dumpsys meminfo $pkg")

            fun memLine(label: String): Long? =
                Regex("$label:?\\s+(\\d+)")
                    .find(mem)
                    ?.groupValues
                    ?.get(1)
                    ?.toLongOrNull()
            val battery = shell("dumpsys battery")

            fun batt(key: String): Int? =
                Regex("$key: (-?\\d+)")
                    .find(battery)
                    ?.groupValues
                    ?.get(1)
                    ?.toIntOrNull()
            val thermal = shell("dumpsys thermalservice")
            val status = Regex("Thermal Status: (\\d+)").find(thermal)?.groupValues?.get(1)
            val temps =
                Regex("mType=(\\d+), mName=([A-Za-z0-9_-]+), mValue=([0-9.]+)")
                    .findAll(thermal)
                    .filter { it.groupValues[1] == "0" || it.groupValues[1] == "3" || it.groupValues[1] == "2" }
                    .joinToString(",") { "${it.groupValues[2]}=${it.groupValues[3]}" }
            return JSONObject()
                .put("t_s", System.currentTimeMillis() / 1000)
                .put("ui_fps", "%.1f".format(java.util.Locale.US, uiFps).toDouble())
                .put("ui_janky_frames", janky)
                .put("drape_surface_fps", drapeSurfaceFps())
                .put("pss_total_kb", memLine("TOTAL PSS") ?: memLine("TOTAL"))
                .put("native_heap_kb", memLine("Native Heap"))
                .put("graphics_kb", memLine("Graphics"))
                .put("cpu_pct_one_core", cpuPct())
                .put("battery_pct", batt("level"))
                .put("battery_temp_tenthC", batt("temperature"))
                .put("thermal_status", status)
                .put("thermal_temps", temps)
                .put("drape", NaviMapTestHooks.drapeBasemapVisible)
                .put("gps", "${NaviMapTestHooks.lastGpsLat},${NaviMapTestHooks.lastGpsLon}")
        }

        private var lastCpuMs = android.os.Process.getElapsedCpuTime()
        private var lastWallMs = android.os.SystemClock.elapsedRealtime()

        /** CPU time of this process since the previous sample, in percent of one core. */
        private fun cpuPct(): Double {
            val cpu = android.os.Process.getElapsedCpuTime()
            val wall = android.os.SystemClock.elapsedRealtime()
            val pct = if (wall > lastWallMs) (cpu - lastCpuMs) * 100.0 / (wall - lastWallMs) else 0.0
            lastCpuMs = cpu
            lastWallMs = wall
            return "%.1f".format(java.util.Locale.US, pct).toDouble()
        }

        /** FPS of the drape SurfaceView layer from SurfaceFlinger's per-layer latency ring. */
        private fun drapeSurfaceFps(): Double? {
            val list = shell("dumpsys SurfaceFlinger --list")
            val layer =
                list
                    .lineSequence()
                    .firstOrNull { it.startsWith("SurfaceView[") && it.contains(context.packageName) && it.contains("(BLAST)") }
                    ?.trim()
                    ?: return null
            val lat = shell("dumpsys SurfaceFlinger --latency $layer").lineSequence().drop(1).toList()
            val present =
                lat
                    .mapNotNull {
                        it
                            .trim()
                            .split(Regex("\\s+"))
                            .getOrNull(1)
                            ?.toLongOrNull()
                    }.filter { it > 0 && it != Long.MAX_VALUE }
            if (present.size < 3) return null
            val span = (present.last() - present.first()) / 1e9
            return if (span > 0) "%.1f".format(java.util.Locale.US, (present.size - 1) / span).toDouble() else null
        }

        private companion object {
            const val JANK_NS = 25_000_000L
        }
    }
}
