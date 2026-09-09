package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.datexPluginDefaultEnabled
import uniffi.navi.datexRefreshJson
import uniffi.navi.datexSettingsDefaultHost
import uniffi.navi.datexSettingsDefaultPort
import uniffi.navi.datexWifiOnlyDefault
import java.net.HttpURLConnection
import java.net.URL

/**
 * On-device DATEX client against the LAN navi-server DocumentRoot.
 * No screenshots — asserts UniFFI parse/filter against a live GetSituation cache.
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
class DatexLiveServerInstrumentedTest {
    private val host = datexSettingsDefaultHost()
    private val portU = datexSettingsDefaultPort()
    private val port = portU.toInt()

    /** Espa → Atnbrufossen corridor vertices (same family as core fixture tests). */
    private val espaAtnbruJson =
        JSONArray()
            .put(JSONArray().put(60.523132).put(11.242463))
            .put(JSONArray().put(60.5621914).put(11.2561239))
            .put(JSONArray().put(60.577133).put(11.273461))
            .put(JSONArray().put(60.63319).put(11.231814))
            .put(JSONArray().put(60.883553).put(10.913103))
            .put(JSONArray().put(61.8512500).put(10.2338420))
            .toString()

    @Test
    fun defaultRemainsOff() {
        assertFalse(datexPluginDefaultEnabled())
        assertFalse(MapHudPrefs.DATEX_PLUGIN_DEFAULT_ENABLED)
        assertTrue(datexWifiOnlyDefault())
    }

    @Test
    fun disabledSkipsNetwork() {
        val raw =
            datexRefreshJson(
                enabled = false,
                host = host,
                port = portU,
                routeLatLonJson = espaAtnbruJson,
                wifiOnly = false,
                onWifi = true,
                useDiscoveryChain = true,
                cacheDir = null,
            )
        val o = JSONObject(raw)
        assertFalse(o.optBoolean("overlay_enabled"))
        assertTrue(o.optString("warning").contains("plugin_disabled"))
        assertEquals("none", o.optString("data_source"))
    }

    @Test
    fun wifiOnlyBlocksWithoutWifi() {
        val raw =
            datexRefreshJson(
                enabled = true,
                host = host,
                port = portU,
                routeLatLonJson = espaAtnbruJson,
                wifiOnly = true,
                onWifi = false,
                useDiscoveryChain = true,
                cacheDir = null,
            )
        val o = JSONObject(raw)
        assertFalse(o.optBoolean("overlay_enabled"))
        assertEquals("wifi_only", o.optString("warning"))
        assertEquals("none", o.optString("data_source"))
    }

    @Test
    fun liveServerCorridorRefresh() {
        assumeTrue("navi-server DATEX not reachable", sourceJsonReachable())
        val raw =
            datexRefreshJson(
                enabled = true,
                host = host,
                port = portU,
                routeLatLonJson = espaAtnbruJson,
                wifiOnly = false,
                onWifi = true,
                useDiscoveryChain = true,
                cacheDir = null,
            )
        android.util.Log.i("NaviDatexTest", "refresh: ${raw.take(2000)}")
        val o = JSONObject(raw)
        assertTrue(
            "overlay should enable on live server; warning=${o.optString("warning")} body=${raw.take(400)}",
            o.optBoolean("overlay_enabled"),
        )
        val src = o.optString("data_source")
        assertTrue(
            "expected server-duckdns, got $src",
            src == "server-duckdns",
        )
        val active = o.getJSONArray("active")
        val inactive = o.getJSONArray("inactive")
        val onRoute = o.getJSONArray("situations_on_route")
        assertTrue("expected corridor situations, got ${onRoute.length()}", onRoute.length() >= 1)
        for (i in 0 until active.length()) {
            val lat = active.getJSONObject(i).getDouble("lat")
            assertTrue("active lat $lat should be on Espa corridor (lat>60.4)", lat > 60.4)
        }
        assertTrue(active.length() + inactive.length() == onRoute.length())
    }

    private fun sourceJsonReachable(): Boolean =
        runCatching {
            val url =
                if (port == 80) {
                    URL("http://$host/datex/source.json")
                } else {
                    URL("http://$host:$port/datex/source.json")
                }
            val conn = url.openConnection() as HttpURLConnection
            conn.connectTimeout = 3000
            conn.readTimeout = 5000
            conn.requestMethod = "GET"
            val code = conn.responseCode
            conn.disconnect()
            code == 200
        }.getOrDefault(false)
}
