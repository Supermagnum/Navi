package no.navi.app

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class InstalledMapsSnapshotTest {
    @get:Rule
    val tmp = TemporaryFolder()

    @After
    fun tearDown() {
        InstalledMaps.clearForTests()
    }

    @Test
    fun snapshot_ready_requires_graph_tiles_not_manifest_only() {
        val internal = tmp.newFolder("files")
        val packs = tmp.newFolder("long-trip-packs")
        File(packs, "ostlandet-latest.navi-manifest.json").writeText(
            """{"schema":1,"stem":"ostlandet-latest","graph_format_version":9}""",
        )
        File(packs, "ostlandet-latest.navi-server-install.json").writeText(
            """{"schema":1,"region_id":"europe/norway/ostlandet","generation":"g1"}""",
        )
        InstalledMaps.refreshFromDirs(internal, listOf("sd" to packs))
        val r = InstalledMaps.region("europe/norway/ostlandet", internal)!!
        assertEquals(9, r.graphFormat)
        assertTrue(r.profilesLoadable.isEmpty())
        assertFalse(r.tilesLoadFor("car"))
        assertFalse(InstalledMaps.packReadyForProfile("europe/norway/ostlandet", "car"))

        File(packs, "ostlandet-latest.navi-graph-car.t0_0.rkyv").writeBytes(ByteArray(64))
        InstalledMaps.refreshFromDirs(internal, listOf("sd" to packs))
        val r2 = InstalledMaps.region("europe/norway/ostlandet", internal)!!
        assertTrue(r2.tilesLoadFor("car"))
        assertTrue(r2.tilesLoadFor("truck"))
        assertTrue(InstalledMaps.packReadyForProfile("europe/norway/ostlandet", "truck"))
    }

    @Test
    fun auto_build_refuses_empty_db_and_defers_during_plan() {
        val internal = tmp.newFolder("idx")
        // Missing / stub DB: never start a missing-region build.
        assertFalse(PlaceIndexAutoBuild.dbReadyForMissingRegionBuilds(internal))
        assertFalse(PlaceIndexAutoBuild.mayStart(internal, "europe/denmark"))
        assertFalse(PlaceIndexAutoBuild.mayStart(internal, "europe/norway/vestlandet"))
        RoutePlanGate.tryBegin()
        try {
            assertFalse(
                PlaceIndexAutoBuild.mayStart(internal, "europe/norway/vestlandet"),
            )
        } finally {
            RoutePlanGate.end()
        }
    }

    @Test
    fun hasInstallForUi_uses_snapshot_partial_fetch_without_listFiles() {
        val internal = tmp.newFolder("files")
        val packs = tmp.newFolder("long-trip-packs")
        File(packs, "schleswig-holstein-latest.navi-manifest.json").writeText(
            """{"schema":1,"stem":"schleswig-holstein-latest","graph_format_version":9}""",
        )
        File(packs, "schleswig-holstein-latest.navi-server-install.json").writeText(
            """{"schema":1,"region_id":"europe/germany/schleswig-holstein","generation":"g1"}""",
        )
        File(packs, "schleswig-holstein-latest.navi-graph-car.t0_0.rkyv").writeBytes(ByteArray(64))
        InstalledMaps.refreshFromDirs(internal, listOf("sd" to packs))
        assertTrue(
            InstalledMaps.hasInstallForUi("europe/germany/schleswig-holstein", internal),
        )
        assertFalse(InstalledMaps.hasInstallForUi("europe/sweden", internal))
    }

    @Test
    fun resolvePlanPbf_prefers_origin_leaf_not_smallest_dest() {
        val dataDir = tmp.newFolder("files")
        val packDir = tmp.newFolder("packs")
        val origin = File(packDir, "niedersachsen-latest.osm.pbf")
        origin.writeBytes(ByteArray(8_000_000))
        File(packDir, "niedersachsen-latest.navi-manifest.json").writeText("{}")
        val dest = File(packDir, "vestlandet-latest.osm.pbf")
        dest.writeBytes(ByteArray(2_000_000))
        File(packDir, "vestlandet-latest.navi-manifest.json").writeText("{}")
        val bevensen =
            RegionCoverage.Waypoint("From", "Bevensen", 53.0797, 10.5872)
        val dalsoren =
            RegionCoverage.Waypoint("To", "Dalsoren", 61.4434, 7.4614)
        val found = RegionCoverage.resolvePlanPbf(dataDir, listOf(bevensen, dalsoren), packDir)
        assertEquals(
            "origin Niedersachsen must win over smaller Vestlandet dest PBF",
            origin.absolutePath,
            found!!.absolutePath,
        )
    }
}
