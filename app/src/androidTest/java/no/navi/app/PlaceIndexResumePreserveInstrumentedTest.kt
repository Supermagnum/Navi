package no.navi.app

import android.database.sqlite.SQLiteDatabase
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.ensurePlaceIndex
import uniffi.navi.initNativeLogging
import uniffi.navi.searchPlaces
import java.io.File

/**
 * Task 3 regression: PLACE_INDEX resume with name_index_build.complete=0 must
 * not wipe name_entries (clearRegionRows), and a killed mid-index region must
 * be rediscovered for automatic retry. Also re-indexes Hamburg on the AVD when
 * the Geofabrik extract is present and asserts a real place search hit.
 */
@RunWith(AndroidJUnit4::class)
class PlaceIndexResumePreserveInstrumentedTest {
    private companion object {
        const val TAG = "PlaceIndexResume"
        const val REGION = "europe/germany/hamburg"
        const val FILENAME = "hamburg-latest.osm.pbf"
    }

    @Test
    fun place_index_resume_preserves_rows_when_complete_zero() {
        initNativeLogging()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir =
            File(context.filesDir, "place_index_resume_preserve").also {
                it.deleteRecursively()
                it.mkdirs()
            }
        val dbFile = File(dataDir, "place_index.db")
        seedIncompleteBuild(dbFile, REGION, osmId = 42L, name = "ResumeKeepMe")
        PlaceIndexReady.readyFile(dataDir).writeText("""["$REGION"]""")

        assertTrue(
            "fixture must look incomplete",
            RegionDownloadBackground.placeIndexBuildIncomplete(dataDir, REGION),
        )
        assertFalse(
            RegionDownloadBackground.shouldClearPlaceRowsOnPipelineStart(
                RegionDownloadBackground.Phase.PLACE_INDEX,
                dataDir,
                REGION,
            ),
        )

        PlaceIndexReady.preparePipelineStart(
            dataDir,
            REGION,
            preserveIncompleteRows = true,
        )
        assertFalse(PlaceIndexReady.isReady(dataDir, REGION))
        assertEquals(
            "clearRegionRows must not run on incomplete PLACE_INDEX resume",
            1,
            countEntries(dbFile, REGION),
        )
        assertEquals("ResumeKeepMe", entryName(dbFile, 42L))

        // Process-death model: sidecar + incomplete build → rediscovered.
        File(dataDir, FILENAME).writeBytes(ByteArray(1_500_000))
        RegionDownloadBackground.writeJob(
            dataDir,
            RegionDownloadBackground.Job(
                url = "https://download.geofabrik.de/europe/germany/hamburg-latest.osm.pbf",
                filename = FILENAME,
                geofabrikPath = REGION,
                phase = RegionDownloadBackground.Phase.PLACE_INDEX,
            ),
        )
        val pending = RegionDownloadBackground.discoverPending(dataDir)
        assertEquals(RegionDownloadBackground.Phase.PLACE_INDEX, pending!!.phase)
        assertEquals(REGION, pending.geofabrikPath)
    }

    @Test
    fun avd_reindex_hamburg_and_search_hits_real_place() {
        initNativeLogging()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = NaviAppData.resolve(context)
        val packDir = LongTripPackStorage.packDownloadDir(context)
        val pbf =
            File(packDir, FILENAME).takeIf { it.isFile && it.length() > 1_000_000L }
                ?: File(dataDir, FILENAME).takeIf { it.isFile && it.length() > 1_000_000L }
        assumeTrue(
            "hamburg extract missing under packDir=$packDir dataDir=$dataDir",
            pbf != null && pbf.isFile,
        )
        val indexDb = File(dataDir, "place_index.db")
        Log.i(TAG, "re-index $REGION from ${pbf!!.absolutePath} -> ${indexDb.absolutePath}")
        val report =
            ensurePlaceIndex(
                pbf.absolutePath,
                indexDb.absolutePath,
                REGION,
            )
        Log.i(TAG, "ensurePlaceIndex=$report bytes=${indexDb.length()}")
        assertTrue("index must PASS: $report", report.contains("PASS"))
        PlaceIndexReady.markReady(dataDir, REGION)

        val hits = searchPlaces(indexDb.absolutePath, "Hamburg", 10u)
        Log.i(TAG, "search hits=${hits.map { "${it.name}@${it.regionId}" }}")
        assertTrue(
            "place search must return a Hamburg hit after re-index",
            hits.any {
                it.name.contains("Hamburg", ignoreCase = true) ||
                    it.regionId.contains("hamburg")
            },
        )
    }

    private fun seedIncompleteBuild(
        dbFile: File,
        regionId: String,
        osmId: Long,
        name: String,
    ) {
        SQLiteDatabase.openOrCreateDatabase(dbFile, null).use { db ->
            db.execSQL(
                """
                CREATE TABLE IF NOT EXISTS name_entries(
                  osm_id INTEGER PRIMARY KEY,
                  name TEXT NOT NULL,
                  kind TEXT NOT NULL,
                  lat REAL NOT NULL,
                  lon REAL NOT NULL,
                  sub_area TEXT NOT NULL DEFAULT '',
                  municipality TEXT NOT NULL DEFAULT '',
                  region_id TEXT NOT NULL DEFAULT ''
                )
                """.trimIndent(),
            )
            db.execSQL(
                """
                CREATE TABLE IF NOT EXISTS name_index_build(
                  region_id TEXT PRIMARY KEY,
                  expected INTEGER NOT NULL,
                  written INTEGER NOT NULL,
                  complete INTEGER NOT NULL DEFAULT 0
                )
                """.trimIndent(),
            )
            db.execSQL("PRAGMA user_version = ${RegionDownloadBackground.PLACE_INDEX_SCHEMA_VERSION}")
            db.execSQL(
                "INSERT OR REPLACE INTO name_entries(osm_id,name,kind,lat,lon,region_id) VALUES(?,?,?,?,?,?)",
                arrayOf(osmId.toString(), name, "place:city", "53.55", "9.99", regionId),
            )
            db.execSQL(
                "INSERT OR REPLACE INTO name_index_build(region_id,expected,written,complete) VALUES(?,?,?,0)",
                arrayOf(regionId, "1000", "50"),
            )
        }
    }

    private fun countEntries(
        dbFile: File,
        regionId: String,
    ): Int =
        SQLiteDatabase.openDatabase(dbFile.absolutePath, null, SQLiteDatabase.OPEN_READONLY).use { db ->
            db
                .rawQuery(
                    "SELECT COUNT(*) FROM name_entries WHERE region_id = ?",
                    arrayOf(regionId),
                ).use { c ->
                    c.moveToFirst()
                    c.getInt(0)
                }
        }

    private fun entryName(
        dbFile: File,
        osmId: Long,
    ): String =
        SQLiteDatabase.openDatabase(dbFile.absolutePath, null, SQLiteDatabase.OPEN_READONLY).use { db ->
            db.rawQuery("SELECT name FROM name_entries WHERE osm_id = ?", arrayOf(osmId.toString())).use { c ->
                assertTrue(c.moveToFirst())
                c.getString(0)
            }
        }
}
