package no.navi.app

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import uniffi.navi.FfiPmtilesJob
import java.io.File

class BasemapDisplayFixTest {
    @get:Rule
    val tmp = TemporaryFolder()

    @Before
    fun setUp() {
        PmtilesArchiveGate.resetForTests()
        BasemapStyleApplyQueue.resetForTests()
        InstalledMaps.clearForTests()
    }

    @After
    fun tearDown() {
        PmtilesArchiveGate.resetForTests()
        BasemapStyleApplyQueue.resetForTests()
        InstalledMaps.clearForTests()
    }

    @Test
    fun empty_completed_file_is_not_selected() {
        val empty = tmp.newFile("europe_norway_ostlandet.pmtiles")
        assertEquals(0L, empty.length())
        val jobs =
            listOf(
                job("empty-completed", "europe_norway_ostlandet", empty.absolutePath),
            )
        assertNull(BasemapStyleResolver.selectVectorCoveringJob(jobs))
        assertEquals("empty archive", PmtilesArchiveGate.rejectionReason(empty))
    }

    @Test
    fun missing_file_falls_through_to_next_covering() {
        PmtilesArchiveGate.validateContent = { file, _ ->
            if (!file.isFile || file.length() == 0L) "missing or empty" else null
        }
        val missing = File(tmp.root, "europe_sweden_varmland.pmtiles")
        val next = tmp.newFile("europe_norway_vestlandet.pmtiles").also { it.writeText("valid-stub") }
        val jobs =
            listOf(
                job("missing", "europe_sweden_varmland", missing.absolutePath),
                job("next", "europe_norway_vestlandet", next.absolutePath),
            )
        val picked = BasemapStyleResolver.selectVectorCoveringJob(jobs)
        assertEquals("next", picked?.id)
        assertEquals(next.absolutePath, picked?.localPath)
    }

    @Test
    fun late_style_result_does_not_replace_newer_one() {
        val first = BasemapStyleApplyQueue.enqueue("offline:ostlandet")
        val second = BasemapStyleApplyQueue.enqueue("offline:vestlandet")
        assertTrue(BasemapStyleApplyQueue.shouldReloadBase(second))
        assertFalse(BasemapStyleApplyQueue.isCurrent(first.generation))
        assertFalse(
            BasemapStyleApplyQueue.accept(
                BasemapStyleApplyQueue.Result(
                    generation = first.generation,
                    sourceKey = "offline:ostlandet",
                    ok = true,
                    reloadedBase = true,
                ),
            ),
        )
        assertEquals(first.generation, BasemapStyleApplyQueue.lastDiscardedGeneration())
        assertNull(BasemapStyleApplyQueue.appliedSourceKey())
        assertTrue(
            BasemapStyleApplyQueue.accept(
                BasemapStyleApplyQueue.Result(
                    generation = second.generation,
                    sourceKey = "offline:vestlandet",
                    ok = true,
                    reloadedBase = true,
                ),
            ),
        )
        assertEquals("offline:vestlandet", BasemapStyleApplyQueue.appliedSourceKey())
    }

    @Test
    fun route_update_does_not_reload_base_map() {
        val load = BasemapStyleApplyQueue.enqueue("offline:ostlandet")
        assertTrue(BasemapStyleApplyQueue.shouldReloadBase(load))
        BasemapStyleApplyQueue.accept(
            BasemapStyleApplyQueue.Result(
                generation = load.generation,
                sourceKey = "offline:ostlandet",
                ok = true,
                reloadedBase = true,
            ),
        )
        val route = BasemapStyleApplyQueue.enqueue("offline:ostlandet", forceBaseReload = true)
        assertFalse(
            "layerEpoch / styleEpoch / route must not reload the same source",
            BasemapStyleApplyQueue.shouldReloadBase(route),
        )
        BasemapStyleApplyQueue.accept(
            BasemapStyleApplyQueue.Result(
                generation = route.generation,
                sourceKey = "offline:ostlandet",
                ok = true,
                reloadedBase = false,
            ),
        )
        assertEquals("offline:ostlandet", BasemapStyleApplyQueue.appliedSourceKey())
        assertFalse(BasemapStyleApplyQueue.lastAccepted()!!.reloadedBase)
    }

    @Test
    fun viewport_beyond_one_archive_mounts_online_when_network_on() {
        assertTrue(
            BasemapStyleResolver.shouldMountOnlineUnderlay(
                hasNetwork = true,
                viewportExtendsBeyond = true,
            ),
        )
        assertFalse(
            BasemapStyleResolver.shouldMountOnlineUnderlay(
                hasNetwork = false,
                viewportExtendsBeyond = true,
            ),
        )
        assertFalse(
            BasemapStyleResolver.shouldMountOnlineUnderlay(
                hasNetwork = true,
                viewportExtendsBeyond = false,
            ),
        )
    }

    @Test
    fun intersecting_jobs_include_every_archive_the_view_touches() {
        PmtilesArchiveGate.validateContent = { file, _ ->
            if (!file.isFile || file.length() == 0L) "empty" else null
        }
        val ost = tmp.newFile("ostlandet.pmtiles").also { it.writeText("ost") }
        val vest = tmp.newFile("vestlandet.pmtiles").also { it.writeText("vest") }
        val jobs =
            listOf(
                job("ost", "europe_norway_ostlandet", ost.absolutePath, 58.0, 7.5, 62.7, 13.0),
                job("vest", "europe_norway_vestlandet", vest.absolutePath, 58.0, 4.5, 63.2, 8.5),
            )
        val view = BasemapStyleResolver.Viewport(south = 59.0, west = 6.0, north = 62.0, east = 12.0)
        val hit = BasemapStyleResolver.selectIntersectingVectorJobs(jobs, view, emptyList())
        assertEquals(2, hit.size)
        assertTrue(hit.any { it.id == "ost" })
        assertTrue(hit.any { it.id == "vest" })
        val scandinavia =
            BasemapStyleResolver.Viewport(south = 55.0, west = 4.0, north = 71.0, east = 32.0)
        assertTrue(BasemapStyleResolver.viewportExtendsBeyondArchives(scandinavia, jobs))
        val oslo = BasemapStyleResolver.Viewport(south = 59.8, west = 10.5, north = 60.0, east = 10.9)
        assertFalse(BasemapStyleResolver.viewportExtendsBeyondArchives(oslo, jobs))
    }

    @Test
    fun rewrite_source_uses_world_bounds_and_tile_url() {
        val style = tmp.newFile("style.json")
        style.writeText(
            """{"sources":{"protomaps":{"type":"vector","url":"pmtiles://file:///tmp/x.pmtiles","maxzoom":15}}}""",
        )
        BasemapStyleResolver.rewriteSourceToCompositeTiles(
            style,
            "http://127.0.0.1:9/{z}/{x}/{y}.pbf",
        )
        val json = org.json.JSONObject(style.readText())
        val pm = json.getJSONObject("sources").getJSONObject("protomaps")
        assertFalse(pm.has("url"))
        assertEquals("http://127.0.0.1:9/{z}/{x}/{y}.pbf", pm.getJSONArray("tiles").getString(0))
        assertEquals(-180.0, pm.getJSONArray("bounds").getDouble(0), 1e-6)
        assertEquals(0, pm.getInt("minzoom"))
    }

    @Test
    fun rewrite_world_bounds_keeps_file_url() {
        val style = tmp.newFile("style-file.json")
        style.writeText(
            """{"sources":{"protomaps":{"type":"vector","url":"pmtiles://file:///tmp/x.pmtiles","maxzoom":15}}}""",
        )
        BasemapStyleResolver.rewriteSourceWorldBounds(style)
        val json = org.json.JSONObject(style.readText())
        val pm = json.getJSONObject("sources").getJSONObject("protomaps")
        assertEquals("pmtiles://file:///tmp/x.pmtiles", pm.getString("url"))
        assertEquals(-180.0, pm.getJSONArray("bounds").getDouble(0), 1e-6)
        assertEquals(0, pm.getInt("minzoom"))
    }

    @Test
    fun installed_maps_reports_empty_completed_as_no_offline_map() {
        PmtilesArchiveGate.validateContent = { file, _ ->
            if (file.length() == 0L) "empty archive" else null
        }
        val internal = tmp.newFolder("empty-tiles")
        val packs = tmp.newFolder("empty-packs")
        File(packs, "ostlandet-latest.navi-manifest.json").writeText(
            """{"schema":1,"stem":"ostlandet-latest","graph_format_version":9}""",
        )
        File(packs, "ostlandet-latest.navi-server-install.json").writeText(
            """{"schema":1,"region_id":"europe/norway/ostlandet","generation":"g1"}""",
        )
        val tiles = File(internal, "pmtiles/europe_norway_ostlandet.pmtiles")
        tiles.parentFile!!.mkdirs()
        tiles.createNewFile()
        InstalledMaps.refreshFromDirs(internal, listOf("sd" to packs))
        val r = InstalledMaps.region("europe/norway/ostlandet", internal)!!
        assertFalse(r.tilesPresent)
        assertTrue(r.tilesRejected)
        assertEquals("empty archive", r.tilesRejectReason)
        val msg = r.noOfflineMapMessage()
        assertTrue(msg != null && msg.contains("no offline map") && msg.contains("empty"))
    }

    private fun job(
        id: String,
        regionKey: String,
        localPath: String,
        minLat: Double = 59.0,
        minLon: Double = 10.0,
        maxLat: Double = 61.0,
        maxLon: Double = 12.0,
    ): FfiPmtilesJob =
        FfiPmtilesJob(
            id = id,
            regionKey = regionKey,
            url = "https://example.invalid",
            localPath = localPath,
            bytesReceived = 1uL,
            totalBytes = 1uL,
            status = "completed",
            paused = false,
            minLat = minLat,
            minLon = minLon,
            maxLat = maxLat,
            maxLon = maxLon,
        )
}
