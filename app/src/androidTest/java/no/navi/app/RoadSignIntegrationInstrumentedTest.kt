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
import uniffi.navi.loadSchoolPoisJson
import uniffi.navi.nearestRoadSignWarningJson
import uniffi.navi.nearestSchoolProximityWarningJson
import uniffi.navi.rasterizeIconPng
import uniffi.navi.roadSignJurisdictionAllows
import uniffi.navi.schoolsNearRouteCorridorJson
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
    fun probe_vallset_109_warning_json() {
        val pbf = norwegianPbf()
        val raw = loadRoadSignsJson(pbf.absolutePath)
        val lat = 60.68080462520444
        val lon = 11.34538019366088
        val allows = roadSignJurisdictionAllows(lat, lon)
        val warn = nearestRoadSignWarningJson(raw, lat, lon)
        Log.i(TAG, "probe allows=$allows warn=$warn")
        assertTrue("jurisdiction at Vallset", allows)
        assertTrue("expected 109 at 70m: $warn", warn.contains("\"code\":\"109\""))
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

    @Test
    fun school_corridor_fallback_boundary_is_real_200m() {
        val pbf = norwegianPbf()
        val pois = loadSchoolPoisJson(pbf.absolutePath)
        assertFalse("children-zone load error", pois.contains("\"error\""))
        val nearRoute =
            """
            [
              {"lat":60.68105,"lon":11.34210,"cum_m":0.0},
              {"lat":60.68120,"lon":11.34240,"cum_m":150.0}
            ]
            """.trimIndent()
        val nearFiltered = JSONArray(schoolsNearRouteCorridorJson(pois, nearRoute, 200.0))
        assertTrue("expected Vallset school in 200m corridor", nearFiltered.length() >= 1)
        val warnNear = nearestSchoolProximityWarningJson(nearFiltered.toString(), 60.68020, 11.34223)
        assertTrue("expected 142 fallback near school: $warnNear", warnNear.contains("\"code\":\"142\""))
        assertTrue("expected children_proximity source: $warnNear", warnNear.contains("\"source\":\"children_proximity\""))

        val farRoute =
            """
            [
              {"lat":60.68110,"lon":11.34720,"cum_m":0.0},
              {"lat":60.68120,"lon":11.34780,"cum_m":200.0}
            ]
            """.trimIndent()
        val farFiltered = JSONArray(schoolsNearRouteCorridorJson(pois, farRoute, 200.0))
        val warnFar = nearestSchoolProximityWarningJson(farFiltered.toString(), 60.68110, 11.34740)
        assertEquals("{}", warnFar)
    }

    @Test
    fun children_zone_index_includes_kindergarten_and_playground() {
        val pbf = norwegianPbf()
        val raw = loadSchoolPoisJson(pbf.absolutePath)
        assertFalse("load error", raw.contains("\"error\""))
        val arr = JSONArray(raw)
        assertTrue("expected child-zone POIs in region", arr.length() >= 1)
        var hasSchool = false
        var hasKindergarten = false
        var hasPlayground = false
        for (i in 0 until arr.length()) {
            when (arr.getJSONObject(i).optString("category")) {
                "school" -> hasSchool = true
                "kindergarten" -> hasKindergarten = true
                "playground" -> hasPlayground = true
            }
        }
        assertTrue("expected school category in extract", hasSchool)
        assertTrue("expected kindergarten category in extract", hasKindergarten)
        assertTrue("expected playground category in extract", hasPlayground)
        Log.i(TAG, "child-zone categories school=$hasSchool kg=$hasKindergarten pg=$hasPlayground n=${arr.length()}")
    }

    @Test
    fun vallset_kindergarten_proximity_fallback_in_corridor() {
        val pbf = norwegianPbf()
        val pois = loadSchoolPoisJson(pbf.absolutePath)
        val arr = JSONArray(pois)
        val kg =
            (0 until arr.length())
                .map { arr.getJSONObject(it) }
                .firstOrNull {
                    it.optString("category") == "kindergarten" &&
                        it.optString("name").contains("Vallset", ignoreCase = true)
                }
                ?: error("Vallset barnehage not indexed in ${pbf.name}")
        val lat = kg.getDouble("lat")
        val lon = kg.getDouble("lon")
        val route =
            """
            [
              {"lat":${lat + 0.0008},"lon":${lon - 0.0004},"cum_m":0.0},
              {"lat":${lat - 0.0008},"lon":${lon + 0.0004},"cum_m":250.0}
            ]
            """.trimIndent()
        val filtered = JSONArray(schoolsNearRouteCorridorJson(pois, route, 200.0))
        assertTrue("kindergarten must be in 200m corridor", filtered.length() >= 1)
        val warn = nearestSchoolProximityWarningJson(filtered.toString(), lat + 0.0003, lon)
        assertTrue("expected 142 at kindergarten: $warn", warn.contains("\"code\":\"142\""))
        assertTrue("expected kindergarten category: $warn", warn.contains("\"category\":\"kindergarten\""))
    }

    @Test
    fun clustered_child_zones_emit_single_nearest_warning() {
        val clustered =
            """
            [
              {"osm_id":1,"lat":60.68110,"lon":11.34220,"name":"Vallset skole","category":"school","kind":"way_centroid"},
              {"osm_id":2,"lat":60.68216,"lon":11.34090,"name":"Vallset barnehage","category":"kindergarten","kind":"node"},
              {"osm_id":3,"lat":60.68105,"lon":11.34225,"name":"Playground","category":"playground","kind":"node"}
            ]
            """.trimIndent()
        val warn = nearestSchoolProximityWarningJson(clustered, 60.68050, 11.34200)
        assertTrue("expected single 142 warning: $warn", warn.contains("\"code\":\"142\""))
        assertTrue("expected children_proximity: $warn", warn.contains("\"source\":\"children_proximity\""))
        assertFalse("must not stack multiple warnings", warn.contains("\"warnings\""))
        // Nearest to query point is playground at 60.68105 (shortest haversine from 60.68050, 11.34200)
        assertTrue("nearest POI wins: $warn", warn.contains("\"category\":\"playground\""))
    }
}
