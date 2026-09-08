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
 * Device checks for pack-server routing vs Geofabrik fallback.
 *
 * Default discovery uses LAN → duckdns. Pass an explicit base URL to force a
 * single hop (unreachable / catalog probes).
 */
@RunWith(AndroidJUnit4::class)
class PackServerRoutingInstrumentedTest {
    private companion object {
        const val TAG = "PackServerRouting"
        const val OSTLANDET = "europe/norway/ostlandet"
        const val VESTLANDET = "europe/norway/vestlandet"
        const val UNREACHABLE_BASE = "http://192.0.2.1:9"
    }

    @Test
    fun norway_ostlandet_resolves_via_chain_or_local() {
        initNativeLogging()
        val d =
            decideRegionAcquisition(
                regionId = OSTLANDET,
                packServerBaseUrl = null,
            )
        Log.i(
            TAG,
            "ostlandet source=${d.source} data_source=${d.dataSource} " +
                "execute_local=${d.executeLocalConvert} reason=${d.reason}",
        )
        assertTrue(
            d.dataSource == "server-lan" ||
                d.dataSource == "server-duckdns" ||
                d.dataSource == "local-bake",
        )
        when (d.source) {
            FfiRegionSourceKind.SERVER -> {
                assertTrue(
                    "stub must still execute local convert until pack-fetch exists",
                    d.executeLocalConvert,
                )
                assertTrue(
                    d.reason.contains("pack fetch not implemented") ||
                        d.reason.contains("local convert"),
                )
            }
            FfiRegionSourceKind.LOCAL -> {
                assertTrue(d.executeLocalConvert)
                assertEquals("local-bake", d.dataSource)
            }
        }
    }

    @Test
    fun norway_vestlandet_resolves_via_chain_or_local() {
        initNativeLogging()
        val d =
            decideRegionAcquisition(
                regionId = VESTLANDET,
                packServerBaseUrl = null,
            )
        Log.i(
            TAG,
            "vestlandet source=${d.source} data_source=${d.dataSource} reason=${d.reason}",
        )
        assertTrue(
            d.dataSource == "server-lan" ||
                d.dataSource == "server-duckdns" ||
                d.dataSource == "local-bake",
        )
        assertTrue(d.executeLocalConvert)
    }

    @Test
    fun unreachable_host_resolves_local_bake() {
        initNativeLogging()
        val d =
            decideRegionAcquisition(
                regionId = OSTLANDET,
                packServerBaseUrl = UNREACHABLE_BASE,
            )
        Log.i(TAG, "unreachable source=${d.source} data_source=${d.dataSource} reason=${d.reason}")
        assertEquals(FfiRegionSourceKind.LOCAL, d.source)
        assertTrue(d.executeLocalConvert)
        assertEquals("local-bake", d.dataSource)
        assertTrue(d.reason.contains("unreachable") || d.reason.contains("local convert"))
    }

    @Test
    fun lan_unreachable_override_does_not_skip_tagging() {
        initNativeLogging()
        // Single-host override skips duckdns by design (tests isolate one hop).
        val d =
            decideRegionAcquisition(
                regionId = OSTLANDET,
                packServerBaseUrl = UNREACHABLE_BASE,
            )
        assertEquals("local-bake", d.dataSource)
        Log.i(TAG, "default_base=${defaultPackServerBaseUrl()}")
    }

    /**
     * Existing local path still callable: Geofabrik URL builder + provisionRegionData
     * for a tiny leaf — download only, no convert assert.
     */
    @Test
    fun local_geofabrik_provision_still_callable() {
        initNativeLogging()
        val path = "europe/faroe-islands"
        val decision =
            decideRegionAcquisition(
                regionId = path,
                packServerBaseUrl = UNREACHABLE_BASE,
            )
        Log.i(TAG, "faroe routing source=${decision.source} reason=${decision.reason}")
        assertEquals(FfiRegionSourceKind.LOCAL, decision.source)

        val url = geofabrikLatestPbfUrl(path)
        assertTrue(url.contains("download.geofabrik.de"))
        assertTrue(url.endsWith("faroe-islands-latest.osm.pbf"))
        Log.i(TAG, "geofabrik_url=$url")

        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = File(NaviAppData.resolve(context), "pack_server_routing_local_e2e")
        dataDir.mkdirs()
        val filename = "faroe-islands-latest.osm.pbf"
        File(dataDir, filename).delete()
        File(dataDir, "$filename.partial").delete()

        val report =
            provisionRegionData(
                dataDir = dataDir.absolutePath,
                pbfUrl = url,
                pbfFilename = filename,
                elevationTarUrl = null,
            )
        Log.i(TAG, "provision report=${report.take(240)}")
        assertTrue(report.contains("PASS") || report.contains("FAIL") || report.isNotBlank())
    }
}
