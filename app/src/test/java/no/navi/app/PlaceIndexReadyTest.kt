package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.navi.PlaceHit
import java.io.File

class PlaceIndexReadyTest {
    @Test
    fun prioritize_without_gps_keeps_request_order() {
        val paths =
            listOf(
                "europe/norway/vestlandet",
                "europe/norway/ostlandet",
            )
        assertEquals(paths, PlaceIndexReady.prioritizePaths(paths, null, null))
    }

    @Test
    fun prioritize_dedupes_and_normalizes() {
        val ordered =
            PlaceIndexReady.prioritizePaths(
                listOf(
                    "europe/norway/vestlandet/",
                    "europe/norway/vestlandet",
                    "europe/norway/trondelag",
                ),
                userLat = null,
                userLon = null,
            )
        assertEquals(
            listOf("europe/norway/vestlandet", "europe/norway/trondelag"),
            ordered,
        )
    }

    @Test
    fun ready_stamp_roundtrip_and_clear() {
        val dir =
            File.createTempFile("place-ready", "dir").apply {
                delete()
                mkdirs()
            }
        try {
            // Seed an empty stamp so load() does not try Android SQLite.
            PlaceIndexReady.readyFile(dir).writeText("[]")
            PlaceIndexReady.markReady(dir, "europe/norway/ostlandet")
            assertTrue(PlaceIndexReady.isReady(dir, "europe/norway/ostlandet"))
            assertFalse(PlaceIndexReady.isReady(dir, "europe/norway/vestlandet"))
            PlaceIndexReady.clearReady(dir, "europe/norway/ostlandet")
            assertFalse(PlaceIndexReady.isReady(dir, "europe/norway/ostlandet"))
            assertEquals(emptySet<String>(), PlaceIndexReady.load(dir))
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun mid_download_cleared_ready_yields_no_hits_via_empty_ready_set() {
        val dir =
            File.createTempFile("place-ready-mid", "dir").apply {
                delete()
                mkdirs()
            }
        try {
            PlaceIndexReady.readyFile(dir).writeText("[]")
            PlaceIndexReady.markReady(dir, "europe/norway/ostlandet")
            PlaceIndexReady.clearReady(dir, "europe/norway/ostlandet")
            // clearReady removes the stamp first — same as download-start —
            // so filterHits sees an empty ready set and returns no hits.
            assertEquals(emptySet<String>(), PlaceIndexReady.load(dir))
            assertFalse(PlaceIndexReady.isReady(dir, "europe/norway/ostlandet"))
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun clear_ready_still_invokes_row_clear_after_fts_order_change() {
        // Regression: clearReady must clear the stamp and attempt DB row clear
        // (FTS best-effort then name_entries). Without a DB file, clearRegionRows
        // is a no-op; stamp clearing is what blocks search mid-download.
        val dir =
            File.createTempFile("place-ready-fts", "dir").apply {
                delete()
                mkdirs()
            }
        try {
            PlaceIndexReady.readyFile(dir).writeText("""["europe/norway/ostlandet"]""")
            assertTrue(PlaceIndexReady.isReady(dir, "europe/norway/ostlandet"))
            PlaceIndexReady.clearReady(dir, "europe/norway/ostlandet")
            assertFalse(PlaceIndexReady.isReady(dir, "europe/norway/ostlandet"))
            assertEquals("[]", PlaceIndexReady.readyFile(dir).readText().trim())
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun filter_keeps_hit_by_region_id_not_lat_lon() {
        val dir =
            File.createTempFile("place-ready-filter", "dir").apply {
                delete()
                mkdirs()
            }
        try {
            PlaceIndexReady.readyFile(dir).writeText("[]")
            PlaceIndexReady.markReady(dir, "europe/norway/ostlandet")
            val keep =
                PlaceHit(
                    1L,
                    "Hamar",
                    "place:town",
                    0.0,
                    0.0,
                    "",
                    "",
                    "europe/norway/ostlandet",
                )
            val drop =
                PlaceHit(
                    2L,
                    "Bergen",
                    "place:city",
                    0.0,
                    0.0,
                    "",
                    "",
                    "europe/norway/vestlandet",
                )
            val filtered =
                PlaceIndexReady.filterHitsToReadyRegions(dir, listOf(keep, drop))
            assertEquals(listOf(keep), filtered)
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun clear_ready_stamp_only_drops_stamp_without_requiring_db() {
        val dir =
            File.createTempFile("place-ready-stamp", "dir").apply {
                delete()
                mkdirs()
            }
        try {
            PlaceIndexReady.readyFile(dir).writeText("""["europe/norway/ostlandet"]""")
            assertTrue(PlaceIndexReady.isReady(dir, "europe/norway/ostlandet"))
            PlaceIndexReady.clearReadyStampOnly(dir, "europe/norway/ostlandet")
            assertFalse(PlaceIndexReady.isReady(dir, "europe/norway/ostlandet"))
            assertEquals("[]", PlaceIndexReady.readyFile(dir).readText().trim())
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun prepare_pipeline_start_preserve_incomplete_uses_stamp_only() {
        val dir =
            File.createTempFile("place-ready-prep", "dir").apply {
                delete()
                mkdirs()
            }
        try {
            PlaceIndexReady.readyFile(dir).writeText("""["europe/germany/hamburg"]""")
            PlaceIndexReady.preparePipelineStart(
                dir,
                "europe/germany/hamburg",
                preserveIncompleteRows = true,
            )
            assertFalse(PlaceIndexReady.isReady(dir, "europe/germany/hamburg"))
            assertEquals("[]", PlaceIndexReady.readyFile(dir).readText().trim())
        } finally {
            dir.deleteRecursively()
        }
    }
}
