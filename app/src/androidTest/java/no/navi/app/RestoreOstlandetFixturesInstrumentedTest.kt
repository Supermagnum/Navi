package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/** One-shot: copy staged Ostlandet PMTiles+DEM into app dataDir and register jobs. */
@RunWith(AndroidJUnit4::class)
class RestoreOstlandetFixturesInstrumentedTest {
    @Test
    fun restore_from_staging() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = NaviAppData.resolve(context)
        val report = OfflinePmtilesBootstrap.restoreOstlandetFromStaging(dataDir)
        android.util.Log.i(TAG, "RESTORE_REPORT $report")
        assertTrue("restore failed: $report", report.startsWith("OK:"))
        val basemap = File(dataDir, "pmtiles/europe_norway_ostlandet.pmtiles")
        val dem = File(dataDir, "pmtiles/europe_norway_ostlandet_dem.pmtiles")
        assertTrue("basemap missing", basemap.isFile && basemap.length() > 1_000_000L)
        assertTrue("dem missing", dem.isFile && dem.length() > 1_000_000L)
        MapHudPrefs.rememberDownloadedPmtilesRegion(context, "europe_norway_ostlandet")
    }

    companion object {
        private const val TAG = "NaviRestoreFixtures"
    }
}
