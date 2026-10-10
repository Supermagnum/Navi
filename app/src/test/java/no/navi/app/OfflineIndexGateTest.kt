package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.io.RandomAccessFile

class OfflineIndexGateTest {
    @get:Rule
    val tmp = TemporaryFolder()

    @Test
    fun empty_data_dir_has_nothing_to_index() {
        val dir = tmp.newFolder("empty")
        assertFalse(OfflineIndexGate.hasMaterialToIndex(dir))
        assertNull(OfflineIndexGate.resolveAutoIndexPbf(dir, "europe/norway/ostlandet"))
    }

    @Test
    fun tiny_stub_pbf_and_manifest_alone_do_not_count() {
        val dir = tmp.newFolder("stub")
        File(dir, "ostlandet-latest.osm.pbf").writeBytes(ByteArray(4096))
        File(dir, "ostlandet-latest.navi-manifest.json").writeText("{}")
        assertFalse(OfflineIndexGate.hasMaterialToIndex(dir))
        assertNull(OfflineIndexGate.resolveAutoIndexPbf(dir, "europe/norway/ostlandet"))
    }

    @Test
    fun large_pbf_counts_as_material() {
        val dir = tmp.newFolder("pbf")
        val pbf = File(dir, "ostlandet-latest.osm.pbf")
        pbf.writeBytes(ByteArray(OfflineIndexGate.MIN_INDEXABLE_PBF_BYTES.toInt()))
        assertTrue(OfflineIndexGate.hasMaterialToIndex(dir))
        assertTrue(OfflineIndexGate.isIndexablePbf(pbf))
        assertTrue(
            OfflineIndexGate
                .resolveAutoIndexPbf(dir, "europe/norway/ostlandet")
                ?.absolutePath == pbf.absolutePath,
        )
    }

    @Test
    fun hamburg_does_not_fall_back_to_sweden_extract() {
        val dir = tmp.newFolder("hh-se")
        val sweden = File(dir, "sweden-latest.osm.pbf")
        RandomAccessFile(sweden, "rw").use { it.setLength(OfflineIndexGate.MIN_INDEXABLE_PBF_BYTES) }
        File(dir, "hamburg-latest.navi-graph-car.rkyv").writeBytes(ByteArray(64))
        assertNull(
            OfflineIndexGate.resolveAutoIndexPbf(dir, "europe/germany/hamburg"),
        )
    }

    @Test
    fun finland_does_not_fall_back_to_sweden_extract() {
        val dir = tmp.newFolder("fi-se")
        val sweden = File(dir, "sweden-latest.osm.pbf")
        RandomAccessFile(sweden, "rw").use { it.setLength(OfflineIndexGate.MIN_INDEXABLE_PBF_BYTES) }
        assertNull(
            OfflineIndexGate.resolveAutoIndexPbf(dir, "europe/finland"),
        )
    }

    @Test
    fun sweden_lan_does_not_use_country_extract_for_place_index() {
        val dir = tmp.newFolder("halland-se")
        val sweden = File(dir, "sweden-latest.osm.pbf")
        RandomAccessFile(sweden, "rw").use { it.setLength(OfflineIndexGate.MIN_INDEXABLE_PBF_BYTES) }
        assertNull(
            OfflineIndexGate.resolveAutoIndexPbf(dir, "europe/sweden/halland"),
        )
        val leaf = File(dir, "halland-latest.osm.pbf")
        RandomAccessFile(leaf, "rw").use { it.setLength(OfflineIndexGate.MIN_INDEXABLE_PBF_BYTES) }
        assertEquals(
            leaf.canonicalFile,
            OfflineIndexGate.resolveAutoIndexPbf(dir, "europe/sweden/halland")?.canonicalFile,
        )
    }

    @Test
    fun blank_region_path_does_not_pick_largest_pbf() {
        val dir = tmp.newFolder("blank")
        val sweden = File(dir, "sweden-latest.osm.pbf")
        RandomAccessFile(sweden, "rw").use { it.setLength(OfflineIndexGate.MIN_INDEXABLE_PBF_BYTES) }
        assertNull(OfflineIndexGate.resolveAutoIndexPbf(dir, ""))
    }

    @Test
    fun graph_pack_without_pbf_counts_as_material() {
        val dir = tmp.newFolder("packs")
        File(dir, "ostlandet-latest.navi-manifest.json").writeText("{}")
        File(dir, "ostlandet-latest.navi-graph-car.t0_0.rkyv").writeBytes(ByteArray(64))
        assertTrue(OfflineIndexGate.hasMaterialToIndex(dir))
    }

    @Test
    fun pending_partial_extract_counts_as_material() {
        val dir = tmp.newFolder("partial")
        File(dir, "ostlandet-latest.osm.pbf.partial").writeBytes(ByteArray(2048))
        assertTrue(OfflineIndexGate.hasMaterialToIndex(dir))
    }

    @Test
    fun fixture_path_is_never_indexable() {
        val fixture =
            File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf")
        assertTrue(OfflineIndexGate.isFixturePath(fixture))
        assertFalse(OfflineIndexGate.isIndexablePbf(fixture))
    }
}
