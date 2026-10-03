package no.navi.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

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
