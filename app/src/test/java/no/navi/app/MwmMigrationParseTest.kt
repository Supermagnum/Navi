package no.navi.app

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The JSON the Rust migration layer returns (keys checked by `json_shape_matches_what_the_app_reads`). */
class MwmMigrationParseTest {
    private val planJson =
        """
        {"version":"261105","world_needed":true,"world_bytes":100000000,"download_bytes":855000000,
         "total_bytes":855000000,"per_region_sum_bytes":1056000000,"shared_saved_bytes":301000000,
         "free_bytes_needed":1223435456,"free_bytes_available":600000000,"enough_space":false,
         "space_short_by":623435456,"old_bytes_selected":2880425339,"old_bytes_unmapped":777,
         "large":true,"wifi_only_default":true,"unavailable_leaves":["Norway_X"],
         "rows":[
          {"region_id":"europe/norway/ostlandet","bake_id":"europe_norway_ostlandet","display_name":"Ostlandet",
           "old_bytes":2549326770,"mapped":true,"selected":true,"leaf_ids":["Norway_Hedmark","Norway_Southern"],
           "region_bytes":402000000,"uncovered_pct":0.04,"note":null},
          {"region_id":"europe/atlantis/east","bake_id":null,"display_name":"East","old_bytes":777,
           "mapped":false,"selected":false,"leaf_ids":[],"region_bytes":0,"uncovered_pct":null,
           "note":"no map area mapping for this region; the old map is kept"}],
         "leaves":[
          {"id":"Norway_Southern","display_name":"Norway Southern","bytes":301000000,"installed":false,
           "shared_by":["europe/norway/ostlandet","europe/norway/sorlandet"],"mostly_outside":false,"excluded":false},
          {"id":"Denmark_North Denmark Region","display_name":"North Denmark Region","bytes":51000000,
           "installed":true,"shared_by":["europe/norway/sorlandet"],"mostly_outside":true,"excluded":true}]}
        """.trimIndent()

    @Test
    fun parsesPlan() {
        val p = MwmMigration.parsePlan(JSONObject(planJson))
        assertEquals("261105", p.version)
        assertFalse(p.enoughSpace)
        assertEquals(623_435_456L, p.spaceShortBy)
        assertTrue(p.large && p.wifiOnlyDefault)
        assertEquals(listOf("Norway_X"), p.unavailableLeaves)
        assertEquals(2, p.rows.size)
        assertNull(p.rows[0].note)
        assertFalse(p.rows[1].mapped)
        assertEquals("no map area mapping for this region; the old map is kept", p.rows[1].note)
        val southern = p.leaves.first { it.id == "Norway_Southern" }
        assertEquals(2, southern.sharedBy.size)
        assertTrue(p.leaves.first { it.excluded }.mostlyOutside)
    }

    @Test
    fun parsesPersistedStates() {
        val running =
            MwmMigration.parsePersisted(
                """{"version":"261105","status":"waiting_wifi","leaves":["A","B"],
                   "old_regions":[{"region_id":"europe/norway/ostlandet","old_bytes":5}],
                   "wifi_only":true,"download_bytes":10,"error":null,"old_maps_decision":"pending","bytes_freed":0}""",
            )!!
        assertTrue(running.inProgress)
        assertFalse(running.needsDecision)
        assertEquals(1, running.oldRegions.size)
        assertNull(running.error)
        val verified =
            MwmMigration.parsePersisted(
                """{"version":"261105","status":"verified","leaves":[],"old_regions":[],"wifi_only":false,
                   "download_bytes":0,"error":null,"old_maps_decision":"pending","bytes_freed":0}""",
            )!!
        assertTrue(verified.needsDecision)
        assertFalse(verified.inProgress)
        assertNull(MwmMigration.parsePersisted("null"))
        assertNull(MwmMigration.parsePersisted(""))
    }

    @Test
    fun parsesStorageSummary() {
        val j =
            """
            {"ok":true,"online":false,"update":{"installed_version":"261005","current_version":"261105",
              "update_available":true,"update_bytes":900},
             "summary":{"version":"261005","leaf_count":2,"leaves_bytes":1510,"world_bytes":100,
              "staging_bytes":0,"other_release_bytes":0,"total_bytes":1610,
              "groups":[{"id":"Norway","display_name":"Norway","total_bytes":1010,
                "leaves":[{"id":"Norway_Hedmark","display_name":"Innlandet (Hedmark)","mwm_bytes":1000,
                           "sidecar_bytes":10,"has_sidecar":true}]},
               {"id":"_other","display_name":"Other maps","total_bytes":500,
                "leaves":[{"id":"Sweden_Stockholm","display_name":"Sweden Stockholm","mwm_bytes":500,
                           "sidecar_bytes":0,"has_sidecar":false}]}]}}
            """.trimIndent()
        val s = MwmStorage.parse(j)
        assertEquals("261005", s.version)
        assertEquals(2, s.groups.size)
        assertEquals(1010L, s.groups[0].leaves[0].bytes)
        assertFalse(s.groups[1].leaves[0].hasSidecar)
        assertTrue(s.update!!.available)
        assertEquals("261105", s.update!!.current)
    }

    @Test
    fun formatsBytes() {
        assertEquals("2.55 GB", formatBytes(2_549_326_770L))
        assertEquals("100 MB", formatBytes(100_000_000L))
        assertEquals("12 kB", formatBytes(12_000L))
        assertEquals("999 B", formatBytes(999L))
    }
}
