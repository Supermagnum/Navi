package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class IndexedPackMissHintTest {
    @Test
    fun graphPackMissShowsHint() {
        val report = "build_s=1.0; pack_hit=false; nodes=0\n"
        assertTrue(reportIndicatesIndexedPackMiss(report))
        assertTrue(indexedPackMissHint(report)!!.startsWith("Slow plan:"))
    }

    @Test
    fun poiPackMissAloneDoesNotShowHint() {
        // Chunk legs skip POI packs; this must not look like PBF graph fallback.
        val report =
            "pack_hit=true; nodes=126300\n" +
                "poi_skipped=chunk_leg\n" +
                "poi_pack_hit=false\n"
        assertFalse(reportIndicatesIndexedPackMiss(report))
        assertNull(indexedPackMissHint(report))
        assertEquals("Route planned: 631,1 km", withIndexedPackMissHint("Route planned: 631,1 km", report))
    }

    @Test
    fun wetlandAndOvernightSidecarMissDoNotShowHint() {
        val report =
            "pack_hit=true\nwetland_pack_hit=false\novernight_buildings_pack_hit=false\n"
        assertFalse(reportIndicatesIndexedPackMiss(report))
        assertNull(indexedPackMissHint(report))
    }
}
