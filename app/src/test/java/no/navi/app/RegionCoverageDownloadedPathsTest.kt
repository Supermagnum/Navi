package no.navi.app

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class RegionCoverageDownloadedPathsTest {
    @get:Rule
    val tmp = TemporaryFolder()

    @After
    fun tearDown() {
        RegionCoverage.exactRegionBboxForTest = null
    }

    @Test
    fun ready_id_niedersachsen_appears_in_downloaded_list() {
        val dir = tmp.newFolder("data")
        File(dir, PlaceIndexReady.READY_FILE).writeText(
            """["europe/germany/niedersachsen","europe/norway/ostlandet"]""",
        )
        val paths = RegionCoverage.downloadedGeofabrikPaths(dir)
        assertTrue(paths.contains("europe/germany/niedersachsen"))
        assertTrue(paths.contains("europe/norway/ostlandet"))
    }

    @Test
    fun bad_norway_niedersachsen_ready_id_is_ignored() {
        val dir = tmp.newFolder("data")
        File(dir, PlaceIndexReady.READY_FILE).writeText(
            """["europe/norway/niedersachsen","europe/germany/niedersachsen"]""",
        )
        val paths = RegionCoverage.downloadedGeofabrikPaths(dir)
        assertFalse(paths.contains("europe/norway/niedersachsen"))
        assertTrue(paths.contains("europe/germany/niedersachsen"))
    }

    @Test
    fun pack_install_stamp_region_id_is_included() {
        val dir = tmp.newFolder("data")
        File(dir, "hamburg-latest.navi-server-install.json").writeText(
            """
            {"schema":1,"region_id":"europe/germany/hamburg","generation":"g",
             "bake_stem":"europe_germany_hamburg-latest","leaf_stem":"hamburg-latest",
             "base_url":"https://navigate-me.duckdns.org"}
            """.trimIndent(),
        )
        val paths = RegionCoverage.downloadedGeofabrikPaths(dir)
        assertTrue(paths.contains("europe/germany/hamburg"))
    }

    @Test
    fun niedersachsen_point_covered_only_with_exact_bbox_bavaria_never() {
        val dir = tmp.newFolder("data")
        File(dir, PlaceIndexReady.READY_FILE).writeText(
            """["europe/germany/niedersachsen"]""",
        )
        val niedLat = 52.3759
        val niedLon = 9.7320
        val bayernLat = 48.1374
        val bayernLon = 11.5755

        RegionCoverage.exactRegionBboxForTest = { null }
        assertFalse(
            RegionCoverage.pointCovered(
                niedLat,
                niedLon,
                RegionCoverage.downloadedGeofabrikPaths(dir),
            ),
        )
        assertFalse(
            RegionCoverage.pointCovered(
                bayernLat,
                bayernLon,
                RegionCoverage.downloadedGeofabrikPaths(dir),
            ),
        )

        // Synthetic exact leaf box around Hannover (not Bavaria).
        RegionCoverage.exactRegionBboxForTest = { path ->
            if (path == "europe/germany/niedersachsen") {
                listOf(51.5, 8.0, 54.0, 11.5)
            } else {
                null
            }
        }
        assertTrue(
            RegionCoverage.pointCovered(
                niedLat,
                niedLon,
                RegionCoverage.downloadedGeofabrikPaths(dir),
            ),
        )
        assertFalse(
            RegionCoverage.pointCovered(
                bayernLat,
                bayernLon,
                RegionCoverage.downloadedGeofabrikPaths(dir),
            ),
        )
    }

    @Test
    fun missing_coverage_null_when_exact_bbox_covers_non_norwegian_install() {
        val dir = tmp.newFolder("data")
        File(dir, PlaceIndexReady.READY_FILE).writeText("""["europe/sweden"]""")
        // Country sweden is an exact GEOFABRIK_PATH_BBOX entry; inject for JVM.
        RegionCoverage.exactRegionBboxForTest = { path ->
            if (path == "europe/sweden") listOf(55.0, 10.0, 70.0, 25.0) else null
        }
        val wp =
            listOf(
                RegionCoverage.Waypoint("To", "Stockholm", 59.33, 18.07),
            )
        assertNull(RegionCoverage.missingCoverage(wp, dir))
    }

    @Test
    fun missing_coverage_prompts_when_installed_leaf_has_no_exact_bbox() {
        val dir = tmp.newFolder("data")
        File(dir, PlaceIndexReady.READY_FILE).writeText(
            """["europe/germany/niedersachsen"]""",
        )
        RegionCoverage.exactRegionBboxForTest = { null }
        val wp =
            listOf(
                RegionCoverage.Waypoint("To", "Hannover", 52.3759, 9.7320),
            )
        val miss = RegionCoverage.missingCoverage(wp, dir)
        assertNotNull(miss)
        assertTrue(miss!!.message.contains("not in any downloaded area"))
    }

    @Test
    fun readyIds_empty_on_missing_or_corrupt() {
        val dir = tmp.newFolder("data")
        assertEquals(emptyList<String>(), PlaceIndexReady.readyIds(dir))
        File(dir, PlaceIndexReady.READY_FILE).writeText("not-json")
        // Defensive parser yields no quoted strings → empty.
        assertEquals(emptyList<String>(), PlaceIndexReady.readyIds(dir))
    }

    @Test
    fun leaf_download_covers_parent_identity() {
        assertTrue(
            RegionCoverage.downloadedCoversIdentity(
                "europe/germany/niedersachsen",
                "europe/germany",
            ),
        )
    }
}
