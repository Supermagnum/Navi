package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiIconTheme
import uniffi.navi.loadRoadSignsJson
import uniffi.navi.nearestRoadSignWarningJson
import uniffi.navi.rasterizeIconPng
import uniffi.navi.roadSignJurisdictionAllows
import java.io.File

@RunWith(AndroidJUnit4::class)
class RoadSignIntegrationInstrumentedTest {
    private companion object {
        const val TAG = "RoadSignIntegration"
    }

    private fun dataDir(): File {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        return NaviAppData.resolve(context)
    }

    private fun norwegianPbf(): File {
        val dataDir = dataDir()
        val candidates =
            dataDir
                .listFiles()
                ?.filter {
                    it.name.endsWith(".osm.pbf") && it.length() > 1_000_000
                }.orEmpty()
        return candidates.firstOrNull { f ->
            val n = f.name.lowercase()
            n.contains("ostlandet") || n.contains("norway") || n.contains("norge")
        } ?: error(
            "need Norwegian region pbf under ${dataDir.absolutePath}; have ${candidates.map { it.name }}",
        )
    }

    private fun approachQueryPoints(
        lat: Double,
        lon: Double,
        distanceM: Double = 200.0,
    ): List<Pair<Double, Double>> {
        val deltaLat = distanceM / 111_320.0
        val cosLat = kotlin.math.cos(Math.toRadians(lat)).coerceAtLeast(0.2)
        val deltaLon = distanceM / (111_320.0 * cosLat)
        return listOf(
            lat - deltaLat to lon,
            lat + deltaLat to lon,
            lat to lon - deltaLon,
            lat to lon + deltaLon,
        )
    }

    private fun findApproachWarning(
        raw: String,
        arr: JSONArray,
    ): String? {
        for (i in 0 until arr.length()) {
            val sign = arr.getJSONObject(i)
            val lat = sign.getDouble("lat")
            val lon = sign.getDouble("lon")
            for ((qLat, qLon) in approachQueryPoints(lat, lon)) {
                if (!roadSignJurisdictionAllows(qLat, qLon)) continue
                val warn = nearestRoadSignWarningJson(raw, qLat, qLon)
                if (warn.contains("icon_key")) return warn
            }
        }
        return null
    }

    private fun iconsDir(): File {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dir = File(ctx.filesDir, "icons-road-sign-test")
        dir.mkdirs()
        copyAssetTree(ctx.assets, "icons", dir)
        return dir
    }

    private fun copyAssetTree(
        am: android.content.res.AssetManager,
        assetPath: String,
        destDir: File,
    ) {
        val names = am.list(assetPath) ?: return
        destDir.mkdirs()
        for (name in names) {
            val childAsset = "$assetPath/$name"
            val children = am.list(childAsset)
            if (children != null && children.isNotEmpty()) {
                copyAssetTree(am, childAsset, File(destDir, name))
            } else {
                runCatching {
                    am.open(childAsset).use { input ->
                        File(destDir, name).outputStream().use { output -> input.copyTo(output) }
                    }
                }
            }
        }
    }

    @Test
    fun catalogue_icons_rasterize_on_device() {
        val icons = iconsDir()
        val samples =
            listOf(
                "no_sign_100_1" to "fareskilt",
                "no_sign_366" to "speed_limit",
                "no_sign_640_10" to "serviceskilt",
                "no_sign_755" to "vegvisning",
            )
        for ((key, category) in samples) {
            val png =
                rasterizeIconPng(
                    key = key,
                    theme = FfiIconTheme.DAY,
                    width = 96u,
                    height = 96u,
                    bundledDir = icons.absolutePath,
                )
            assertTrue("$category $key png empty", png.isNotEmpty())
            Log.i(TAG, "raster ok key=$key bytes=${png.size}")
        }
    }

    @Test
    fun pbf_index_and_synthetic_warning() {
        val pbf = norwegianPbf()
        val raw = loadRoadSignsJson(pbf.absolutePath)
        assertFalse("load error", raw.contains("\"error\""))
        val arr = JSONArray(raw)
        Log.i(TAG, "indexed signs=${arr.length()} from ${pbf.name}")
        if (arr.length() == 0) {
            Log.i(TAG, "no tagged signs in region; skipping nearest warning assert")
            return
        }
        val warn =
            findApproachWarning(raw, arr)
                ?: error("no sign yielded approach warning within Norway jurisdiction")
        assertFalse("812 must not appear in warning", warn.contains("\"code\":\"812\""))
        Log.i(TAG, "nearest=$warn")
    }

    @Test
    fun compound_tag_matches_base_sign_only() {
        val pbf = norwegianPbf()
        val raw = loadRoadSignsJson(pbf.absolutePath)
        val arr = JSONArray(raw)
        for (i in 0 until arr.length()) {
            val o = arr.getJSONObject(i)
            assertFalse(
                "812 must not match as fixed symbol",
                o.optString("code") == "812",
            )
        }
        assertEquals("{}", nearestRoadSignWarningJson("[]", 59.91, 10.75))
        if (arr.length() > 0) {
            val corridor =
                findApproachWarning(raw, arr)
                    ?: error("corridor: no sign yielded approach warning")
            assertTrue("corridor warning: $corridor", corridor.contains("icon_key"))
        }
    }
}
