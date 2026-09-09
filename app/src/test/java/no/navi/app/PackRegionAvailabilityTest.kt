package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import kotlin.io.path.createTempDirectory

class PackRegionAvailabilityTest {
    @Test
    fun pathCovered_exact_and_parent_child() {
        val ready = listOf("europe/norway/ostlandet", "europe/norway/vestlandet")
        assertTrue(PackRegionAvailability.pathCoveredByReadyIds("europe/norway/ostlandet", ready))
        assertTrue(PackRegionAvailability.pathCoveredByReadyIds("europe/norway", ready))
        assertTrue(PackRegionAvailability.pathCoveredByReadyIds("europe", ready))
        assertFalse(PackRegionAvailability.pathCoveredByReadyIds("europe/sweden", ready))
        assertFalse(PackRegionAvailability.pathCoveredByReadyIds("europe/norway/trondelag", ready))
    }

    @Test
    fun pathCovered_vastra_gotaland_hyphen_chip_matches_underscore_catalog() {
        val ready = listOf("europe/sweden/vastra_gotaland")
        assertTrue(
            PackRegionAvailability.pathCoveredByReadyIds(
                "europe/sweden/vastra-gotaland",
                ready,
            ),
        )
        assertTrue(
            PackRegionAvailability.regionIdsMatchForCatalog(
                "europe/sweden/vastra-gotaland",
                "europe/sweden/vastra_gotaland",
            ),
        )
        assertEquals(
            listOf("europe/sweden/vastra_gotaland"),
            PackRegionAvailability.packCatalogRegionIdAliases("europe/sweden/vastra-gotaland"),
        )
    }

    @Test
    fun localBakeReady_manifest_also_checks_catalog_alias_stem() {
        val dir = createTempDirectory("navi-pill-alias").toFile()
        try {
            assertFalse(
                PackRegionAvailability.localBakeReady(dir, "europe/sweden/vastra-gotaland"),
            )
            // Packs install under the published underscore leaf stem.
            File(dir, "vastra_gotaland-latest.navi-manifest.json").writeText("{}")
            assertTrue(
                PackRegionAvailability.localBakeReady(dir, "europe/sweden/vastra-gotaland"),
            )
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun localBakeReady_manifest() {
        val dir = createTempDirectory("navi-pill").toFile()
        try {
            assertFalse(PackRegionAvailability.localBakeReady(dir, "europe/norway/ostlandet"))
            File(dir, "ostlandet-latest.navi-manifest.json").writeText("{}")
            assertTrue(PackRegionAvailability.localBakeReady(dir, "europe/norway/ostlandet"))
            assertTrue(
                PackRegionAvailability.localBakeReadyUnderPrefix(
                    dir,
                    "europe/norway",
                    listOf("europe/norway/ostlandet"),
                ),
            )
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun statusLine_server_ready() {
        val line =
            PackRegionAvailability.statusLine(
                selectedPath = "europe/norway/ostlandet",
                serverReadyIds = listOf("europe/norway/ostlandet"),
                dataSource = "server-lan",
                unreachableReason = null,
                probing = false,
                dataDir = null,
            )
        assertTrue(line.contains("server-lan"))
        assertTrue(line.contains("published packs") || line.contains("pack server"))
        assertEquals(
            "Checking pack server…",
            PackRegionAvailability.statusLine(
                selectedPath = "europe/norway/ostlandet",
                serverReadyIds = emptyList(),
                dataSource = "local-bake",
                unreachableReason = null,
                probing = true,
                dataDir = null,
            ),
        )
    }

    @Test
    fun download_and_osm_button_look() {
        assertEquals(
            "Download region",
            PackRegionAvailability.downloadRegionButtonLabel(serverReady = true),
        )
        assertEquals(
            "Download region + build place index",
            PackRegionAvailability.downloadRegionButtonLabel(serverReady = false),
        )
        assertTrue(PackRegionAvailability.downloadRegionUsesReadyStyle(true))
        assertFalse(PackRegionAvailability.downloadRegionUsesReadyStyle(false))
        assertTrue(PackRegionAvailability.osmCheckUsesReadyStyle(true))
        assertFalse(PackRegionAvailability.osmCheckUsesReadyStyle(false))
    }
}
