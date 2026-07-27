package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.pmtilesCancelJob
import uniffi.navi.pmtilesListCovering
import uniffi.navi.pmtilesPauseJob
import uniffi.navi.pmtilesPlanetUrl
import uniffi.navi.pmtilesQueueRegion
import uniffi.navi.pmtilesResumeJob
import uniffi.navi.pmtilesRunJob
import java.io.File
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

@RunWith(AndroidJUnit4::class)
class BasemapStyleResolverInstrumentedTest {
    @Test
    fun resolve_falls_back_to_liberty_without_pmtiles() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = NaviAppData.resolve(context)
        val resolved =
            BasemapStyleResolver.resolve(
                context = context,
                dataDir = dataDir,
                lat = 69.65,
                lon = 18.96,
                prefer3d = false,
                vulkanAvailable = true,
                forceOnline2d = true,
            )
        assertEquals(BasemapStyleResolver.StyleKind.OnlineLiberty, resolved.kind)
        assertEquals(BasemapStyleResolver.LIBERTY_URL, resolved.styleUri)
    }

    @Test
    fun planet_url_points_at_protomaps_build() {
        val url = pmtilesPlanetUrl()
        assertTrue(
            "expected build.protomaps.com URL, got $url",
            url.contains("build.protomaps.com") && url.endsWith(".pmtiles"),
        )
    }

    @Test
    fun real_oslo_extract_then_offline_style() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = NaviAppData.resolve(context)
        val planet = pmtilesPlanetUrl()
        val job = pmtilesQueueRegion(dataDir.absolutePath, "test/oslo", planet)
        assertTrue("queue failed: ${job.status}", job.id.isNotBlank() && !job.status.startsWith("failed"))

        val pool = Executors.newSingleThreadExecutor()
        pool.execute {
            Thread.sleep(800)
            pmtilesPauseJob(job.id)
            Thread.sleep(500)
            pmtilesResumeJob(job.id)
        }

        val done = pmtilesRunJob(dataDir.absolutePath, job.id)
        pool.shutdown()
        pool.awaitTermination(120, TimeUnit.SECONDS)

        assertEquals("completed", done.status)
        assertTrue(File(done.localPath).isFile)
        assertTrue(File(done.localPath).length() > 1000)

        val covering = pmtilesListCovering(dataDir.absolutePath, 59.91, 10.75)
        assertTrue(covering.any { it.id == done.id })

        val resolved =
            BasemapStyleResolver.resolve(
                context = context,
                dataDir = dataDir,
                lat = 59.91,
                lon = 10.75,
                prefer3d = false,
                vulkanAvailable = true,
            )
        assertEquals(BasemapStyleResolver.StyleKind.OfflineProtomaps, resolved.kind)
        assertNotNull(resolved.coveringJob)
        assertTrue(resolved.styleUri.startsWith("file://"))
        val styleFile = File(resolved.styleUri.removePrefix("file://"))
        assertTrue(styleFile.isFile)
        val text = styleFile.readText()
        assertTrue(
            "style missing pmtiles file source: ${text.take(400)}",
            text.contains("pmtiles:") && text.contains("file:"),
        )
        assertTrue(
            "style missing local path: ${done.localPath}",
            text.contains(done.localPath) || text.contains(done.localPath.replace("/", "\\/")),
        )
    }

    @Test
    fun mapterhorn_augment_style_json_adds_dem_and_hillshade() {
        val base =
            org.json.JSONObject(
                """
                {"version":8,"sources":{"openmaptiles":{"type":"vector","url":"https://example/tiles.json"}},
                 "layers":[{"id":"bg","type":"background"},{"id":"place","type":"symbol","source":"openmaptiles"}]}
                """.trimIndent(),
            )
        val out = MapterhornTerrain.augmentStyleJson(base)
        val sources = out.getJSONObject("sources")
        assertTrue(sources.has(MapterhornTerrain.TERRAIN_SOURCE_ID))
        assertTrue(sources.has(MapterhornTerrain.HILLSHADE_SOURCE_ID))
        assertEquals(
            "raster-dem",
            sources.getJSONObject(MapterhornTerrain.HILLSHADE_SOURCE_ID).getString("type"),
        )
        val demSrc = sources.getJSONObject(MapterhornTerrain.HILLSHADE_SOURCE_ID)
        assertEquals("terrarium", demSrc.getString("encoding"))
        assertEquals(512, demSrc.getInt("tileSize"))
        assertTrue(demSrc.has("tiles"))
        assertTrue(
            demSrc.getJSONArray("tiles").getString(0).contains("mapterhorn.com"),
        )
        assertTrue(
            demSrc.getString("attribution").contains("Mapterhorn"),
        )
        val layers = out.getJSONArray("layers")
        var hillsIdx = -1
        var symbolIdx = -1
        for (i in 0 until layers.length()) {
            val id = layers.getJSONObject(i).getString("id")
            if (id == MapterhornTerrain.HILLS_LAYER_ID) hillsIdx = i
            if (id == "place") symbolIdx = i
        }
        assertTrue(hillsIdx >= 0)
        assertTrue(symbolIdx >= 0)
        assertTrue("hillshade must sit under labels", hillsIdx < symbolIdx)
        assertFalse(out.has("terrain"))
        assertFalse(out.has("sky"))
    }

    @Test
    fun three_d_requires_vulkan_else_liberty() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = NaviAppData.resolve(context)
        val noVulkan =
            BasemapStyleResolver.resolve(
                context = context,
                dataDir = dataDir,
                lat = 59.33,
                lon = 18.07,
                prefer3d = true,
                vulkanAvailable = false,
                forceOnline2d = true,
            )
        assertEquals(BasemapStyleResolver.StyleKind.OnlineLiberty, noVulkan.kind)
        assertEquals(BasemapStyleResolver.LIBERTY_URL, noVulkan.styleUri)
        assertEquals(0.0, noVulkan.cameraPitch, 0.01)
        assertNotNull(noVulkan.note)

        val withVulkan =
            BasemapStyleResolver.resolve(
                context = context,
                dataDir = dataDir,
                lat = 59.33,
                lon = 18.07,
                prefer3d = true,
                vulkanAvailable = true,
                forceOnline2d = true,
            )
        // Online 3D = Liberty vector basemap + Mapterhorn DEM hillshade (Native has no mesh terrain).
        assertEquals(BasemapStyleResolver.StyleKind.Online3d, withVulkan.kind)
        assertEquals(BasemapStyleResolver.LIBERTY_URL, withVulkan.styleUri)
        assertTrue(withVulkan.attachMapterhornTerrain)
        assertEquals(BasemapStyleResolver.TERRAIN_VIEW_TILT, withVulkan.cameraPitch, 0.01)
    }

    @Test
    fun cancel_aborts_job() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = NaviAppData.resolve(context)
        File(dataDir, "pmtiles/test_oslo.pmtiles").delete()
        File(dataDir, "pmtiles/test_oslo.pmtiles.partial").delete()
        val job = pmtilesQueueRegion(dataDir.absolutePath, "test/oslo", pmtilesPlanetUrl())
        assertTrue(job.id.isNotBlank())
        val pool = Executors.newSingleThreadExecutor()
        pool.execute {
            Thread.sleep(200)
            pmtilesCancelJob(job.id)
        }
        val done = pmtilesRunJob(dataDir.absolutePath, job.id)
        pool.shutdown()
        pool.awaitTermination(60, TimeUnit.SECONDS)
        assertTrue(
            "expected cancelled or failed-after-cancel, got ${done.status}",
            done.status == "cancelled" || done.status.startsWith("failed"),
        )
    }
}
