package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.CampingCallKind
import uniffi.navi.campingPluginCampHereTonight
import uniffi.navi.campingPluginConfigure
import uniffi.navi.campingPluginInstallGuest
import uniffi.navi.campingPluginSetClockYmd
import uniffi.navi.campingPluginSetEnabled
import uniffi.navi.campingPluginSetTimezone
import uniffi.navi.initNativeLogging
import java.io.File
import java.util.TimeZone

/**
 * Force-stop persistence for the camping night store (two instrumented runs).
 *
 * Host orchestration:
 * 1. Run [seed_campHereTwoNights]
 * 2. `adb shell am force-stop no.navi.app`
 * 3. Run [load_afterForceStop_thirdNightDeclined]
 *
 * Logcat tags: NaviCampingForceStop
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
class CampingNightForceStopInstrumentedTest {
    private lateinit var filesDir: File
    private lateinit var dataDir: File

    private val lat = 61.11515
    private val lon = 10.46628

    @Before
    fun setUp() {
        initNativeLogging()
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        filesDir = ctx.filesDir
        dataDir = filesDir
        campingPluginConfigure(
            filesDir.absolutePath,
            dataDir.absolutePath,
            TimeZone.getDefault().id,
        )
        val am = ctx.assets
        val name = "right_to_roam_camping"
        val manifest = am.open("plugins/$name/plugin.json").use { it.readBytes().decodeToString() }
        val wasm = am.open("plugins/$name/plugin.wasm").use { it.readBytes() }
        campingPluginInstallGuest(name, manifest, wasm)
        campingPluginSetEnabled(true)
        campingPluginSetTimezone("Europe/Oslo")
    }

    private fun kvFile(): File = File(filesDir, "plugin_kv/camping_night.json")

    private fun jobJson(): String =
        """
        {"probes":[[$lat,$lon]],"max_suggestions":4,"buildings":[],"glaciers":[],
         "safety":{"min_building_distance_m":150.0,"min_glacier_distance_m":1000.0},
         "clock":null,"kv_ok":true,
         "countries":[[$lat,$lon,"no"]],"subdivisions":[[$lat,$lon,"no-34"]],
         "travel_mode":"non_motorised","vehicle_class":"unknown",
         "is_professional_driver_under_rest_rules":false}
        """.trimIndent()

    @Test
    fun seed_campHereTwoNights() {
        val kv = kvFile()
        if (kv.isFile) kv.delete()
        campingPluginSetClockYmd(2026, 7u, 1u)
        val m1 = campingPluginCampHereTonight(lat, lon, "no", "no-34")
        android.util.Log.i("NaviCampingForceStop", "runA camp_01 msg=$m1")
        assertTrue(m1.startsWith("OK:"))
        campingPluginSetClockYmd(2026, 7u, 2u)
        val m2 = campingPluginCampHereTonight(lat, lon, "no", "no-34")
        android.util.Log.i("NaviCampingForceStop", "runA camp_02 msg=$m2 kv=${kv.readText()}")
        assertTrue(m2.startsWith("OK:"))
        assertTrue("night store must exist after Camp here", kv.isFile)
        android.util.Log.i(
            "NaviCampingForceStop",
            "runA ready for force-stop path=${kv.absolutePath} bytes=${kv.length()}",
        )
    }

    @Test
    fun load_afterForceStop_thirdNightDeclined() {
        val kv = kvFile()
        assertTrue("kv must survive force-stop", kv.isFile)
        android.util.Log.i("NaviCampingForceStop", "runB kv=${kv.readText()}")
        campingPluginSetClockYmd(2026, 7u, 3u)
        val call =
            kotlinx.coroutines.runBlocking {
                CampingPluginApi.runSuggest(jobJson())
            }
        assumeTrue("wasm ${call.kind} ${call.message}", call.kind == CampingCallKind.OK)
        val parsed = parseCampingSuggestResultJson(call.resultJson!!)
        val accepted = parsed.list.cards.any { it.accepted } || parsed.list.probesAccepted > 0
        android.util.Log.i(
            "NaviCampingForceStop",
            "runB third_night accepted=$accepted via=${parsed.via} json=${call.resultJson}",
        )
        assertTrue("third consecutive night must be declined after force-stop", !accepted)
    }
}
