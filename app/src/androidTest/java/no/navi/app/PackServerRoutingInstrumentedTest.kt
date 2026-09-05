package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiRegionSourceKind
import uniffi.navi.decideRegionAcquisition
import uniffi.navi.defaultPackServerBaseUrl
import uniffi.navi.geofabrikLatestPbfUrl
import uniffi.navi.initNativeLogging
import uniffi.navi.provisionRegionData
import java.io.File

/**
 * Device LAN checks for pack-server routing vs Geofabrik fallback.
 *
 * Expects the pack host at [defaultPackServerBaseUrl] (default
 * http://192.168.1.195) reachable from the tablet Wi-Fi.
 */
@RunWith(AndroidJUnit4::class)
class PackServerRoutingInstrumentedTest {
    private companion object {
        const val TAG = "PackServerRouting"
        const val CATALOG_REGION = "australia-oceania/australia/christmas-island"
        const val MISSING_REGION = "europe/norway/ostlandet"
        const val UNREACHABLE_BASE = "http://192.0.2.1:9"
    }

    @Test
    fun catalog_region_resolves_server_then_local_stub() {
        initNativeLogging()
        val base = defaultPackServerBaseUrl()
        Log.i(TAG, "pack_server_base=$base")
        val d =
            decideRegionAcquisition(
                regionId = CATALOG_REGION,
                packServerBaseUrl = base,
            )
        Log.i(
            TAG,
            "catalog_region source=${d.source} execute_local=${d.executeLocalConvert} " +
                "gen=${d.regionGeneration} reason=${d.reason}",
        )
        assertEquals(FfiRegionSourceKind.SERVER, d.source)
        assertTrue(
            "stub must still execute local convert until pack-fetch exists",
            d.executeLocalConvert,
        )
        assertTrue(d.reason.contains("pack fetch not implemented") || d.reason.contains("local convert"))
    }

    @Test
    fun missing_region_resolves_local() {
        initNativeLogging()
        val d =
            decideRegionAcquisition(
                regionId = MISSING_REGION,
                packServerBaseUrl = defaultPackServerBaseUrl(),
            )
        Log.i(TAG, "missing_region source=${d.source} reason=${d.reason}")
        assertEquals(FfiRegionSourceKind.LOCAL, d.source)
        assertTrue(d.executeLocalConvert)
        assertTrue(d.reason.contains("not published") || d.reason.contains("unreachable"))
    }

    @Test
    fun unreachable_host_resolves_local() {
        initNativeLogging()
        val d =
            decideRegionAcquisition(
                regionId = CATALOG_REGION,
                packServerBaseUrl = UNREACHABLE_BASE,
            )
        Log.i(TAG, "unreachable source=${d.source} reason=${d.reason}")
        assertEquals(FfiRegionSourceKind.LOCAL, d.source)
        assertTrue(d.executeLocalConvert)
        assertTrue(d.reason.contains("unreachable") || d.reason.contains("local convert"))
    }

    /**
     * Existing local path still callable: Geofabrik URL builder + provisionRegionData
     * for a tiny catalog-missing leaf (Faroe Islands) — download only, no convert assert.
     */
    @Test
    fun local_geofabrik_provision_still_callable() {
        initNativeLogging()
        val path = "europe/faroe-islands"
        val decision =
            decideRegionAcquisition(
                regionId = path,
                packServerBaseUrl = defaultPackServerBaseUrl(),
            )
        Log.i(TAG, "faroe routing source=${decision.source} reason=${decision.reason}")
        assertEquals(
            "faroe-islands must not be in the migrate catalog sample",
            FfiRegionSourceKind.LOCAL,
            decision.source,
        )

        val url = geofabrikLatestPbfUrl(path)
        assertTrue(url.contains("download.geofabrik.de"))
        assertTrue(url.endsWith("faroe-islands-latest.osm.pbf"))
        Log.i(TAG, "geofabrik_url=$url")

        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = File(NaviAppData.resolve(context), "pack_server_routing_local_e2e")
        dataDir.mkdirs()
        val filename = "faroe-islands-latest.osm.pbf"
        // Remove prior incomplete artifacts so this run is deterministic.
        File(dataDir, filename).delete()
        File(dataDir, "$filename.partial").delete()

        val report =
            provisionRegionData(
                dataDir = dataDir.absolutePath,
                pbfUrl = url,
                pbfFilename = filename,
                elevationTarUrl = null,
            )
        Log.i(TAG, "provision_report=${report.take(400)}")
        assertTrue("local provision must PASS:\n$report", report.contains("PASS"))
        val pbf = File(dataDir, filename)
        assertTrue("pbf downloaded", pbf.isFile && pbf.length() > 1_000_000L)
        Log.i(TAG, "pbf_bytes=${pbf.length()}")
    }
}
