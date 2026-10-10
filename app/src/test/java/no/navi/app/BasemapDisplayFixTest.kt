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
    fun world_overview_is_not_a_regional_slot() {
        PmtilesArchiveGate.validateContent = { file, _ ->
            if (!file.isFile || file.length() == 0L) "empty" else null
        }
        val world = tmp.newFile("world_overview.pmtiles").also { it.writeText("world") }
        val ost = tmp.newFile("ostlandet.pmtiles").also { it.writeText("ost") }
        val vest = tmp.newFile("vestlandet.pmtiles").also { it.writeText("vest") }
        val jobs =
            listOf(
                job("world", "world_overview", world.absolutePath, -85.0, -180.0, 85.0, 180.0),
                job("ost", "europe_norway_ostlandet", ost.absolutePath, 58.0, 7.5, 62.7, 13.0),
                job("vest", "europe_norway_vestlandet", vest.absolutePath, 58.0, 4.5, 63.2, 8.5),
            )
        val view = BasemapStyleResolver.Viewport(south = 59.0, west = 6.0, north = 62.0, east = 12.0)
        val hit = BasemapStyleResolver.selectIntersectingVectorJobs(jobs, view, emptyList())
        assertFalse(hit.any { it.regionKey == "world_overview" })
        val mounted = BasemapStyleResolver.selectMountedRegionals(hit, view)
        assertTrue(mounted.size <= BasemapStyleResolver.MAX_REGIONAL_SOURCES)
        assertTrue(mounted.any { it.id == "ost" })
        assertTrue(mounted.any { it.id == "vest" })
        val overview = BasemapStyleResolver.findWorldOverview(jobs, tmp.root)
        assertEquals("world", overview?.id)
    }

    @Test
    fun mounted_regionals_keep_the_three_that_cover_most_of_the_view() {
        PmtilesArchiveGate.validateContent = { file, _ ->
            if (!file.isFile || file.length() == 0L) "empty" else null
        }
        val a = tmp.newFile("a.pmtiles").also { it.writeText("a") }
        val b = tmp.newFile("b.pmtiles").also { it.writeText("b") }
        val c = tmp.newFile("c.pmtiles").also { it.writeText("c") }
        val d = tmp.newFile("d.pmtiles").also { it.writeText("d") }
        val jobs =
            listOf(
                job("a", "europe_a", a.absolutePath, 59.0, 10.0, 62.0, 13.0),
                job("b", "europe_b", b.absolutePath, 59.0, 8.0, 61.0, 10.5),
                job("c", "europe_c", c.absolutePath, 60.0, 12.0, 61.0, 13.0),
                job("d", "europe_d", d.absolutePath, 59.2, 10.2, 59.4, 10.4),
            )
        val view = BasemapStyleResolver.Viewport(south = 59.0, west = 8.0, north = 62.0, east = 13.0)
        val mounted = BasemapStyleResolver.selectMountedRegionals(jobs, view)
        assertEquals(3, mounted.size)
        assertTrue(mounted.any { it.id == "a" })
        assertTrue(mounted.any { it.id == "b" })
        assertFalse(mounted.any { it.id == "d" })
        val key1 =
            BasemapStyleResolver.MountedSources(overview = null, regionals = mounted, includeOnline = false).key()
        val key2 =
            BasemapStyleResolver.MountedSources(overview = null, regionals = mounted, includeOnline = false).key()
        assertEquals(key1, key2)
    }

    @Test
    fun country_archive_is_dropped_when_the_view_is_inside_a_regional() {
        PmtilesArchiveGate.validateContent = { file, _ ->
            if (!file.isFile || file.length() == 0L) "empty" else null
        }
        val germany = tmp.newFile("europe_germany.pmtiles").also { it.writeText("de") }
        val hamburg = tmp.newFile("europe_germany_hamburg.pmtiles").also { it.writeText("hh") }
        val jobs =
            listOf(
                job("de", "europe/germany", germany.absolutePath, 47.0, 5.8, 55.1, 15.1),
                job("hh", "europe/germany/hamburg", hamburg.absolutePath, 53.38, 9.70, 53.75, 10.33),
            )
        val city = BasemapStyleResolver.Viewport(south = 53.54, west = 9.97, north = 53.56, east = 10.01)
        val mounted = BasemapStyleResolver.selectMountedRegionals(jobs, city)
        assertEquals(listOf("hh"), mounted.map { it.id })
        val wide = BasemapStyleResolver.Viewport(south = 51.0, west = 6.0, north = 54.5, east = 12.0)
        val wideMounted = BasemapStyleResolver.selectMountedRegionals(jobs, wide)
        assertEquals(listOf("de"), wideMounted.map { it.id })
    }

    @Test
    fun mount_keys_that_share_an_eighty_char_prefix_get_distinct_leaves_and_both_set_style() {
        val ov = "/data/user/0/no.navi.app/files/pmtiles/world_overview.pmtiles"
        val finland = "/data/user/0/no.navi.app/files/pmtiles/europe_finland.pmtiles"
        val vasterbotten = "/data/user/0/no.navi.app/files/pmtiles/europe_sweden_vasterbotten.pmtiles"
        val overview = job("ov", "world_overview", ov)
        val a =
            BasemapStyleResolver.MountedSources(
                overview = overview,
                regionals = listOf(job("fi", "europe/finland", finland)),
                includeOnline = false,
            )
        val b =
            BasemapStyleResolver.MountedSources(
                overview = overview,
                regionals = listOf(job("vb", "europe/sweden/vasterbotten", vasterbotten)),
                includeOnline = false,
            )
        val oldStem: (String) -> String = { raw ->
            raw.replace(Regex("[^A-Za-z0-9._+-]"), "_").take(80)
        }
        assertEquals(oldStem(a.key()), oldStem(b.key()))
        val leafA = BasemapStyleResolver.mountedStyleLeafName(a, false)
        val leafB = BasemapStyleResolver.mountedStyleLeafName(b, false)
        assertTrue(leafA != leafB)
        val sharedUri = "file:///styles/${oldStem(a.key())}.json"
        assertFalse(
            BasemapStyleResolver.shouldSkipSetStyle(
                a.key(),
                sharedUri,
                b.key(),
                sharedUri,
            ),
        )
        assertTrue(
            BasemapStyleResolver.shouldSkipSetStyle(
                a.key(),
                "file:///styles/$leafA",
                a.key(),
                "file:///styles/$leafA",
            ),
        )
        val dir = tmp.newFolder("styles")
        File(dir, leafA).writeText("{}")
        File(dir, leafB).writeText("{}")
        File(dir, "style.native.v1.${oldStem(a.key())}.json").writeText("stale")
        BasemapStyleResolver.sweepStalePreparedStyles(dir, leafA)
        assertTrue(File(dir, leafA).isFile)
        assertFalse(File(dir, leafB).isFile)
        assertFalse(File(dir, "style.native.v1.${oldStem(a.key())}.json").isFile)
    }

    @Test
    fun airplane_mode_is_treated_as_offline() {
        assertFalse(BasemapStyleResolver.networkUsable(airplane = true, hasInternet = true))
        assertTrue(BasemapStyleResolver.networkUsable(airplane = false, hasInternet = true))
        assertFalse(BasemapStyleResolver.networkUsable(airplane = false, hasInternet = false))
    }

    @Test
    fun overview_is_not_drawn_when_the_online_map_is_mounted() {
        assertTrue(BasemapStyleResolver.shouldDrawOverview(overviewPresent = true, includeOnline = false))
        assertFalse(BasemapStyleResolver.shouldDrawOverview(overviewPresent = true, includeOnline = true))
        assertFalse(BasemapStyleResolver.shouldDrawOverview(overviewPresent = false, includeOnline = true))
    }

    @Test
    fun layer_kind_puts_earth_before_water_before_roads() {
        val earth = org.json.JSONObject("""{"id":"earth","type":"fill","source-layer":"earth"}""")
        val water = org.json.JSONObject("""{"id":"water","type":"fill","source-layer":"water"}""")
        val road = org.json.JSONObject("""{"id":"roads_major","type":"line","source-layer":"roads"}""")
        val label = org.json.JSONObject("""{"id":"places","type":"symbol","source-layer":"places"}""")
        assertEquals(BasemapStyleResolver.LayerKind.EarthLand, BasemapStyleResolver.layerKind(earth))
        assertEquals(BasemapStyleResolver.LayerKind.Water, BasemapStyleResolver.layerKind(water))
        assertEquals(BasemapStyleResolver.LayerKind.Roads, BasemapStyleResolver.layerKind(road))
        assertEquals(BasemapStyleResolver.LayerKind.Labels, BasemapStyleResolver.layerKind(label))
    }

    @Test
    fun installed_maps_lists_world_overview() {
        val internal = tmp.newFolder("overview-tiles")
        val packs = tmp.newFolder("overview-packs")
        File(internal, "pmtiles").mkdirs()
        File(internal, "pmtiles/world_overview.pmtiles").writeText("overview-stub")
        PmtilesArchiveGate.validateContent = { file, key ->
            if (key == "world_overview" && file.name.contains("world_overview")) null else "no"
        }
        InstalledMaps.refreshFromDirs(internal, listOf("sd" to packs))
        val r = InstalledMaps.region("world_overview", internal)!!
        assertTrue(r.tilesPresent)
        assertTrue(InstalledMaps.summaryText().contains("world_overview"))
    }

    @Test
    fun mvt_probe_reads_layer_names() {
        // Tile { layers { name = "water" } } — field 3 message, field 1 string.
        val water = "water".toByteArray()
        val layer = byteArrayOf(0x0A, water.size.toByte()) + water
        val tile = byteArrayOf(0x1A, layer.size.toByte()) + layer
        assertTrue(PmtilesLayerProbe.mvtLayerNames(tile).contains("water"))
        assertTrue(PmtilesLayerProbe.hasWater(setOf("water")))
        assertFalse(PmtilesLayerProbe.hasWater(setOf("earth", "roads")))
    }

    @Test
    fun archive_gate_allows_world_overview_maxzoom_6() {
        val file = tmp.newFile("world_overview.pmtiles")
        file.outputStream().use { out ->
            out.write("PMTiles".toByteArray())
            out.write(ByteArray(94))
            out.write(byteArrayOf(6))
            out.write(ByteArray(25))
        }
        // Header-only stub is too small for a real sample tile; hook the content
        // check so the maxzoom floor is what we exercise.
        PmtilesArchiveGate.validateContent = { f, key ->
            if (key == "world_overview" && f.name.contains("world_overview")) null else "no"
        }
        assertTrue(PmtilesArchiveGate.isUsable(file, "world_overview"))
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
