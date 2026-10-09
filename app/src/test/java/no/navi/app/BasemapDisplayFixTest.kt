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
            minLat = 59.0,
            minLon = 10.0,
            maxLat = 61.0,
            maxLon = 12.0,
        )
}
