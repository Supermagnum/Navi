package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class OsmUpdateUserCopyTest {
    @Test
    fun check_upToDate_isPlain() {
        val raw =
            "USER_VISIBLE=true\nOSM extract is up to date.\nlocal_sequence=Some(12)\n" +
                "remote_sequence=12\nremote_timestamp=2026-08-01T00:00:00Z\n"
        assertEquals(OsmUpdateUserCopy.UP_TO_DATE, OsmUpdateUserCopy.forCheckReport(raw))
        assertFalse(OsmUpdateUserCopy.forCheckReport(raw).contains("sequence", ignoreCase = true))
    }

    @Test
    fun check_fullRedownload_hidesReason() {
        val raw =
            "USER_VISIBLE=true\nFull re-download recommended (opt-in).\n" +
                "reason=Local Geofabrik sequence unknown; cannot safely chain .osc.gz diffs\n" +
                "remote_sequence=99\nurl=https://download.geofabrik.de/europe/norway/ostlandet-latest.osm.pbf\n" +
                "Confirm Apply to replace the local extract.\n"
        val msg = OsmUpdateUserCopy.forCheckReport(raw)
        assertEquals(OsmUpdateUserCopy.AVAILABLE, msg)
        assertFalse(msg.contains("reason=", ignoreCase = true))
        assertFalse(msg.contains("sequence", ignoreCase = true))
        assertFalse(msg.contains("geofabrik", ignoreCase = true))
    }

    @Test
    fun check_unsupported_hidesRegionMeta() {
        val raw =
            "USER_VISIBLE=true\nOSM update check unsupported.\n" +
                "reason=No region_meta.json — bind a Geofabrik region first\n"
        val msg = OsmUpdateUserCopy.forCheckReport(raw)
        assertEquals(OsmUpdateUserCopy.NO_REGION, msg)
        assertFalse(msg.contains("region_meta", ignoreCase = true))
        assertFalse(msg.contains("USER_VISIBLE", ignoreCase = true))
    }

    @Test
    fun apply_pass_hidesMethod() {
        val raw =
            "PASS\nmethod=full_redownload\nreason=local geofabrik sequence unknown\n" +
                "bytes=123\nUSER_VISIBLE=true\n"
        val msg = OsmUpdateUserCopy.forApplyReport(raw)
        assertEquals(OsmUpdateUserCopy.UPDATED, msg)
        assertFalse(msg.contains("method=", ignoreCase = true))
        assertFalse(msg.contains("PASS", ignoreCase = true))
    }

    @Test
    fun sanitize_truncationStyleLeak() {
        val leaked = "pass method=full redownload reason=local geofabrik sequence unknown..."
        assertTrue(OsmUpdateUserCopy.looksTechnical(leaked))
        val msg = OsmUpdateUserCopy.sanitize(leaked)
        assertEquals(OsmUpdateUserCopy.UPDATED, msg)
    }
}
