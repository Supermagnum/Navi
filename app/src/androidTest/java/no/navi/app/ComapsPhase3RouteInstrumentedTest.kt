package no.navi.app

import android.os.Debug
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiComapsProfile
import uniffi.navi.FfiComapsRouteCode
import uniffi.navi.comapsNativeAvailable
import uniffi.navi.comapsSessionClose
import uniffi.navi.comapsSessionOpen
import uniffi.navi.comapsSessionRoute
import java.io.File
import java.io.FileOutputStream

/**
 * Phase 3 device gate: CoMaps routes on physical Samsung only.
 *
 * Expects maps under externalFilesDir/navi-mwm-test/ (adb-prepared):
 *   data/          — CoMaps resource files (or copied from APK assets)
 *   maps/261105/   — leaf .mwm files
 *   sidecars/      — *.navi-sidecar.v1.zst
 *
 * Writes GeoJSON + summary JSON under navi-mwm-test/out/.
 */
@RunWith(AndroidJUnit4::class)
class ComapsPhase3RouteInstrumentedTest {
    private lateinit var root: File
    private lateinit var outDir: File

    data class RouteCase(
        val id: String,
        val profile: FfiComapsProfile,
        val fromLat: Double,
        val fromLon: Double,
        val toLat: Double,
        val toLon: Double,
        val expectLengthM: Double? = null,
        val lengthTolM: Double = 50.0,
    )

    @Before
    fun setUp() {
        MwmClientBootstrap.ensureInitialized()
        // Touch UniFFI so JNA loads libnavi.so (and DT_NEEDED liborganicmaps.so)
        // without System.loadLibrary("organicmaps") / JNI_OnLoad.
        check(comapsNativeAvailable()) {
            "comaps-native not linked into this APK"
        }
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        root = File(ctx.getExternalFilesDir(null), "navi-mwm-test")
        outDir = File(root, "out")
        outDir.mkdirs()
        ensureResourceData(ctx)
    }

    /**
     * CoMaps Android [Platform.GetReader] resolves non-.mwm resources via
     * writable-dir (`w`) or APK zip `assets/<file>` (`r`). A plain filesystem
     * "resource dir" is not searched — so classificator / packed_polygons /
     * World*.mwm must live in the **writable** root next to `261105/`.
     */
    private fun ensureResourceData(ctx: android.content.Context) {
        root.mkdirs()
        val mapsVer = File(root, "261105")
        mapsVer.mkdirs()
        // Prefer APK assets/comaps-data into writable root.
        val am = ctx.assets
        val names =
            try {
                am.list("comaps-data")?.toList().orEmpty()
            } catch (_: Exception) {
                emptyList()
            }
        for (name in names) {
            if (name == "MANIFEST.txt") continue
            val dest = File(root, name)
            if (dest.isFile && dest.length() > 0L) continue
            am.open("comaps-data/$name").use { input ->
                FileOutputStream(dest).use { output -> input.copyTo(output) }
            }
        }
        // Sidecar / World overlays may already sit under data/ from adb push.
        val dataDir = File(root, "data")
        if (dataDir.isDirectory) {
            for (f in dataDir.listFiles().orEmpty()) {
                if (!f.isFile) continue
                val dest = File(root, f.name)
                if (!dest.exists() || dest.length() == 0L) {
                    f.copyTo(dest, overwrite = true)
                }
            }
        }
        // Flatten maps/261105 → 261105 if prepared that way.
        val nested = File(root, "maps/261105")
        if (nested.isDirectory) {
            for (f in nested.listFiles().orEmpty()) {
                if (f.name.endsWith(".mwm")) {
                    val dest = File(mapsVer, f.name)
                    if (!dest.exists()) f.copyTo(dest, overwrite = false)
                }
            }
        }
        // World*.mwm stay at writable root (Phase 1 download / adb push). Do
        // NOT copy them into 261105/: registering them as versioned country
        // files without cross-mwm sections breaks Joints cross-MWM routing.
        // CoMaps GetAdditionalWorldScope is "r" (APK assets) for resource
        // Worlds; leaf-to-leaf routing uses cross-mwm inside the region MWMs.
    }

    private fun pssKb(): Int {
        val mi = Debug.MemoryInfo()
        Debug.getMemoryInfo(mi)
        return mi.totalPss
    }

    private fun writeGeoJson(
        id: String,
        wetland: Boolean,
        points: List<uniffi.navi.FfiComapsLatLon>,
    ): File {
        val coords = JSONArray()
        for (p in points) {
            coords.put(JSONArray().put(p.lon).put(p.lat))
        }
        val feat =
            JSONObject()
                .put("type", "Feature")
                .put(
                    "geometry",
                    JSONObject().put("type", "LineString").put("coordinates", coords),
                ).put(
                    "properties",
                    JSONObject().put("id", id).put("wetland_hook", wetland),
                )
        val fc =
            JSONObject()
                .put("type", "FeatureCollection")
                .put("features", JSONArray().put(feat))
        val suffix = if (wetland) "wetland" else "plain"
        val f = File(outDir, "$id.$suffix.geojson")
        f.writeText(fc.toString())
        return f
    }

    @Test
    fun phase3OstlandetRoutesOnDevice() {
        assertTrue(
            "liborganicmaps / comaps-native must be linked for Phase 3",
            comapsNativeAvailable(),
        )

        val sidecarDir = File(root, "sidecars")
        assertTrue(
            "missing classificator in writable root $root",
            File(root, "classificator.txt").isFile,
        )
        assertTrue("missing packed_polygons.bin", File(root, "packed_polygons.bin").isFile)
        assertTrue("missing maps dir $root/261105", File(root, "261105").isDirectory)
        assertTrue("missing sidecars $sidecarDir", sidecarDir.isDirectory)

        val coldPssBefore = pssKb()
        // Both paths = writable root: Android GetReader `r` (zip) fails on a
        // plain directory, then `w` finds classificator / packed_polygons /
        // World*.mwm; leaf MWMs live under 261105/.
        val open =
            comapsSessionOpen(
                storagePath = root.absolutePath,
                writablePath = root.absolutePath,
                sidecarDir = sidecarDir.absolutePath,
                loadWetlandSidecars = true,
            )
        assertTrue(
            "comaps_session_open failed: ${open.errorMessage}",
            open.ok,
        )
        val idlePss = pssKb()

        val cases =
            listOf(
                RouteCase(
                    "hamar_elverum",
                    FfiComapsProfile.CAR,
                    60.7945,
                    11.0680,
                    60.8825,
                    11.5623,
                ),
                RouteCase(
                    "elverum_lillehammer",
                    FfiComapsProfile.CAR,
                    60.8825,
                    11.5623,
                    61.1153,
                    10.4662,
                ),
                RouteCase(
                    "lillehammer_drammen",
                    FfiComapsProfile.CAR,
                    61.1153,
                    10.4662,
                    59.7440,
                    10.2045,
                ),
                RouteCase(
                    "foot_wetland_detour",
                    FfiComapsProfile.PEDESTRIAN,
                    61.0500,
                    11.4700,
                    61.0800,
                    11.5200,
                    expectLengthM = 13867.0,
                ),
            )

        val summary = JSONObject()
        summary.put("cold_start_init_ms", open.initMs)
        summary.put("mwm_register_ms", open.registerMs)
        summary.put("sidecars_loaded", open.sidecarsLoaded)
        summary.put("pss_kb_before_init", coldPssBefore)
        summary.put("pss_kb_idle_ostlandet", idlePss)
        val routes = JSONArray()
        var allOk = true

        for (c in cases) {
            for (wetland in listOf(false, true)) {
                if (c.profile != FfiComapsProfile.PEDESTRIAN && wetland) {
                    // Car routes: still measure with hook on (should be NoError).
                }
                val pssBefore = pssKb()
                val t0 = System.nanoTime()
                val r =
                    comapsSessionRoute(
                        profile = c.profile,
                        fromLat = c.fromLat,
                        fromLon = c.fromLon,
                        toLat = c.toLat,
                        toLon = c.toLon,
                        wetlandHook = wetland,
                    )
                val wallMs = (System.nanoTime() - t0) / 1_000_000.0
                val peakPss = maxOf(pssBefore, pssKb())
                val geo = writeGeoJson(c.id, wetland, r.points)
                val expectLen =
                    when {
                        c.id == "foot_wetland_detour" && !wetland -> 13867.0
                        c.id == "foot_wetland_detour" && wetland -> 16245.0
                        else -> c.expectLengthM
                    }
                val lenOk =
                    expectLen == null ||
                        kotlin.math.abs(r.lengthM - expectLen) <= c.lengthTolM
                val codeOk = r.code == FfiComapsRouteCode.OK
                if (!codeOk || !lenOk) allOk = false
                routes.put(
                    JSONObject()
                        .put("id", c.id)
                        .put("wetland_hook", wetland)
                        .put("code", r.code.name)
                        .put("length_m", r.lengthM)
                        .put("time_s", r.timeS)
                        .put("build_time_s", r.buildTimeS)
                        .put("wall_ms", wallMs)
                        .put("peak_pss_kb", peakPss)
                        .put("point_count", r.points.size)
                        .put("geojson", geo.absolutePath)
                        .put("error", r.errorMessage)
                        .put("length_ok", lenOk)
                        .put("expect_length_m", expectLen),
                )
            }
        }

        summary.put("routes", routes)
        summary.put("all_ok", allOk)
        File(outDir, "phase3-summary.json").writeText(summary.toString(2))
        comapsSessionClose()

        assertTrue("Phase 3 routes failed — see ${File(outDir, "phase3-summary.json")}", allOk)
    }
}
