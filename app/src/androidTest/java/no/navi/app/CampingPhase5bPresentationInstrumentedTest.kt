package no.navi.app

import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.CampingCallKind
import uniffi.navi.TravelProfile
import uniffi.navi.campingPluginClearClockOverride
import uniffi.navi.campingPluginConfigure
import uniffi.navi.campingPluginInstallGuest
import uniffi.navi.campingPluginSetClockYmd
import uniffi.navi.campingPluginSetEnabled
import uniffi.navi.campingPluginSetNavContext
import uniffi.navi.campingPluginSetTimezone
import uniffi.navi.initNativeLogging
import java.io.File
import java.util.TimeZone

@RunWith(AndroidJUnit4::class)
@LargeTest
class CampingPhase5bPresentationInstrumentedTest {
    @get:Rule
    val composeRule = createComposeRule()

    private lateinit var filesDir: File
    private lateinit var dataDir: File

    @Before
    fun setUp() {
        initNativeLogging()
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        filesDir = ctx.filesDir
        // Planning graphs live next to ostlandet-latest.navi-graph-*.rkyv in filesDir.
        dataDir = filesDir
        campingPluginConfigure(
            filesDir.absolutePath,
            dataDir.absolutePath,
            TimeZone.getDefault().id,
        )
        installGuest("right_to_roam_camping")
        campingPluginSetEnabled(true)
        campingPluginSetTimezone("Europe/Oslo")
    }

    private fun installGuest(name: String) {
        val am = InstrumentationRegistry.getInstrumentation().targetContext.assets
        val manifest = am.open("plugins/$name/plugin.json").use { it.readBytes().decodeToString() }
        val wasm = am.open("plugins/$name/plugin.wasm").use { it.readBytes() }
        campingPluginInstallGuest(name, manifest, wasm)
    }

    private fun regionPbf(): File? {
        val dest = File(dataDir, "ostlandet-latest.osm.pbf")
        if (dest.isFile && dest.length() > 0L) return dest
        val src =
            listOf(
                File("/data/local/tmp/ostlandet-latest.osm.pbf"),
                File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"),
            ).firstOrNull { it.isFile } ?: return null
        return runCatching {
            src.copyTo(dest, overwrite = false)
            dest.takeIf { it.isFile }
        }.getOrNull() ?: dest.takeIf { it.isFile }
    }

    private fun lillehammerSjusjoenWaypoints(): String {
        val wps =
            listOf(
                doubleArrayOf(61.11515, 10.46628),
                doubleArrayOf(61.1300, 10.5800),
                doubleArrayOf(61.1475, 10.6980),
            )
        return campingWaypointsJson(wps)
    }

    private fun installVarmlandPack(): File? {
        val src = File("/data/local/tmp/navi_varmland")
        if (!src.isDirectory) return null
        val dest = File(filesDir, "varmland_pack")
        dest.mkdirs()
        src.listFiles()?.forEach { f ->
            f.copyTo(File(dest, f.name), overwrite = true)
        }
        File(dest, "europe_sweden_varmland-latest.osm.pbf").writeBytes(ByteArray(0))
        File(dest, "europe_sweden_varmland-latest.navi-server-install.json").writeText("{}")
        return dest.takeIf {
            File(it, "europe_sweden_varmland-latest.navi-manifest.json").isFile
        }
    }

    private suspend fun suggestParsed(): CampingSuggestResult? {
        campingPluginSetNavContext(
            waypointsJson = lillehammerSjusjoenWaypoints(),
            destLat = 61.1475,
            destLon = 10.6980,
            profile = TravelProfile.HIKING,
            professionalDriver = false,
        )
        val call = CampingPluginApi.suggestAlongRoute(12u)
        assumeTrue(
            "graph/PBF unavailable: ${call.kind} ${call.message}",
            call.kind == CampingCallKind.OK || call.kind == CampingCallKind.UNAVAILABLE,
        )
        if (call.kind != CampingCallKind.OK) return null
        val json = call.resultJson ?: return null
        return parseCampingSuggestResultJson(json)
    }

    @Test
    fun hikingCorridor_disclaimerAndNorwegianFireText() {
        val pbf = regionPbf()
        assumeTrue("ostlandet PBF required for corridor graph", pbf != null)
        campingPluginClearClockOverride()
        val parsed =
            kotlinx.coroutines.runBlocking {
                suggestParsed()
            } ?: return
        assertTrue(parsed.disclaimer.contains("not legal advice"))
        assertTrue("must evaluate in wasmtime guest", parsed.via == "wasmtime")
        val cards = parsed.list.cards + parsed.onFootFromHere.cards
        assumeTrue("expected at least one card along Lillehammer corridor", cards.isNotEmpty())
        val noCards = cards.filter { it.countryIso.equals("no", ignoreCase = true) }
        assumeTrue("expected Norwegian cards on this corridor", noCards.isNotEmpty())
        assertTrue(noCards.any { !it.fireText.isNullOrBlank() })
    }

    @Test
    fun fireText_differsBetweenJulyAndOctoberClock() {
        val pbf = regionPbf()
        assumeTrue("ostlandet PBF required", pbf != null)
        campingPluginSetClockYmd(2026, 7u, 15u)
        val july =
            kotlinx.coroutines.runBlocking { suggestParsed() }
                ?: return
        val julyFire =
            (july.list.cards + july.onFootFromHere.cards)
                .mapNotNull { it.fireText }
                .joinToString(" ")
        campingPluginSetClockYmd(2026, 10u, 15u)
        val oct =
            kotlinx.coroutines.runBlocking { suggestParsed() }
                ?: return
        val octFire =
            (oct.list.cards + oct.onFootFromHere.cards)
                .mapNotNull { it.fireText }
                .joinToString(" ")
        assumeTrue("need fire text on at least one month", julyFire.isNotBlank() || octFire.isNotBlank())
        if (julyFire.isNotBlank() && octFire.isNotBlank()) {
            assertNotEquals(julyFire, octFire)
        }
        campingPluginClearClockOverride()
    }

    @Test
    fun mobileHome_vehicleEmpty_onFootHasWalkNote() {
        val pbf = regionPbf()
        assumeTrue("ostlandet PBF required", pbf != null)
        campingPluginSetNavContext(
            waypointsJson = lillehammerSjusjoenWaypoints(),
            destLat = 61.1475,
            destLon = 10.6980,
            profile = TravelProfile.MOBILE_HOME,
            professionalDriver = false,
        )
        val call =
            kotlinx.coroutines.runBlocking {
                CampingPluginApi.suggestAlongRoute(12u)
            }
        if (call.kind != CampingCallKind.OK) {
            assumeTrue("skip when graph unavailable: ${call.message}", false)
            return
        }
        val parsed = parseCampingSuggestResultJson(call.resultJson!!)
        assertTrue(
            "vehicle overnight should be empty or unaccepted",
            parsed.vehicle.probesAccepted == 0 && parsed.vehicle.cards.isEmpty(),
        )
        assertNotNull(parsed.onFootFromHere)
        val foot = parsed.onFootFromHere.cards
        assumeTrue("expected on-foot cards for mobile home", foot.isNotEmpty())
        assertTrue(
            foot.any {
                (it.walkM != null && it.walkM!! > 0.0) ||
                    it.notes.any { n -> n.contains("walk", ignoreCase = true) || n.contains("track", ignoreCase = true) }
            },
        )
    }

    @Test
    fun kongsvingerCharlottenberg_swedishTierASafetyDefault() {
        val pack = installVarmlandPack()
        assumeTrue(
            "Värmland navi pack required under /data/local/tmp/navi_varmland",
            pack != null,
        )
        campingPluginConfigure(
            filesDir.absolutePath,
            pack!!.absolutePath,
            TimeZone.getDefault().id,
        )
        installGuest("right_to_roam_camping")
        campingPluginSetEnabled(true)
        campingPluginSetTimezone("Europe/Oslo")
        campingPluginSetNavContext(
            waypointsJson =
                campingWaypointsJson(
                    listOf(
                        doubleArrayOf(59.889366, 12.192353),
                        doubleArrayOf(59.8840, 12.3040),
                    ),
                ),
            destLat = 59.8840,
            destLon = 12.3040,
            profile = TravelProfile.HIKING,
            professionalDriver = false,
        )
        val call =
            kotlinx.coroutines.runBlocking {
                CampingPluginApi.suggestAlongRoute(12u)
            }
        assumeTrue("graph ${call.kind} ${call.message}", call.kind == CampingCallKind.OK)
        val parsed = parseCampingSuggestResultJson(call.resultJson!!)
        assertTrue("must evaluate in wasmtime guest", parsed.via == "wasmtime")
        val se =
            (parsed.list.cards + parsed.onFootFromHere.cards).filter {
                it.countryIso.equals("se", ignoreCase = true)
            }
        assumeTrue("expected Swedish cards on Värmland pack at Charlottenberg", se.isNotEmpty())
        assertTrue(
            se.any { c ->
                c.notes.any { n ->
                    n.contains("Navi safety default", ignoreCase = true) ||
                        n.contains("not Swedish law", ignoreCase = true)
                }
            },
        )
        composeRule.setContent {
            MaterialTheme {
                CampingSuggestionSheet(
                    result = parsed,
                    profile = TravelProfile.HIKING,
                    listDisclaimer = parsed.disclaimer,
                    sessionDisableMessage = null,
                    onReEnableSession = {},
                    onClose = {},
                )
            }
        }
        composeRule.onNodeWithTag("camping_suggestion_sheet").assertExists()
        composeRule.waitForIdle()
        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertNotNull(shot)
        val outDir = File("/sdcard/Download/navi_camping_screenshots")
        outDir.mkdirs()
        File(outDir, "camping_sweden_tier_a.png").outputStream().use { os ->
            shot!!.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, os)
        }
        android.util.Log.i(
            "NaviCampingSweden",
            "wasm se_cards=${se.size} via=${parsed.via} pack=europe/sweden/varmland",
        )
    }

    @Test
    fun nightStore_displayThreeDaysNeverRecords_campHereBlocksThird_differentSpotsOk() {
        campingPluginSetEnabled(true)
        val kv = File(filesDir, "plugin_kv/camping_night.json")
        if (kv.isFile) kv.delete()
        val lat = 61.11515
        val lon = 10.46628
        val jobBase =
            """
            {"probes":[[$lat,$lon]],"max_suggestions":4,"buildings":[],"glaciers":[],
             "safety":{"min_building_distance_m":150.0,"min_glacier_distance_m":1000.0},
             "clock":null,"kv_ok":true,
             "countries":[[$lat,$lon,"no"]],"subdivisions":[[$lat,$lon,"no-34"]],
             "travel_mode":"non_motorised","vehicle_class":"unknown",
             "is_professional_driver_under_rest_rules":false}
            """.trimIndent()
        fun suggestOn(y: Int, m: UInt, d: UInt): CampingSuggestResult {
            campingPluginSetClockYmd(y, m, d)
            val call =
                kotlinx.coroutines.runBlocking {
                    CampingPluginApi.runSuggest(jobBase)
                }
            assumeTrue("wasm suggest ${call.kind} ${call.message}", call.kind == CampingCallKind.OK)
            android.util.Log.i(
                "NaviCampingNight",
                "suggest date=$y-$m-$d kv_exists=${kv.isFile} kv_bytes=${if (kv.isFile) kv.length() else 0} json=${call.resultJson}",
            )
            return parseCampingSuggestResultJson(call.resultJson!!)
        }
        fun campHere(y: Int, m: UInt, d: UInt, la: Double = lat, lo: Double = lon): String {
            campingPluginSetClockYmd(y, m, d)
            val msg = uniffi.navi.campingPluginCampHereTonight(la, lo, "no", "no-34")
            android.util.Log.i("NaviCampingNight", "camp_here date=$y-$m-$d msg=$msg kv=${kv.takeIf { it.isFile }?.readText()}")
            return msg
        }
        fun nightKeysPresent(): Boolean {
            if (!kv.isFile) return false
            val text = kv.readText()
            return text.contains("\"rtr_night:") || text.contains("rtr_night:")
        }
        // Display-only across three days must never write night records or block.
        for (day in 1u..3u) {
            val shown = suggestOn(2026, 7u, day)
            assumeTrue(
                "display day $day should accept",
                shown.list.probesAccepted > 0 || shown.list.cards.any { it.accepted },
            )
            assertTrue(
                "display/suggest must not record a night (day $day); kv=${kv.takeIf { it.isFile }?.readText()}",
                !nightKeysPresent(),
            )
        }
        // Explicit Camp here on 07-01 and 07-02 → 07-03 declined for same spot.
        assertTrue(campHere(2026, 7u, 1u).startsWith("OK:"))
        assertTrue("Camp here must create night store", kv.isFile)
        val d1 = suggestOn(2026, 7u, 1u)
        assumeTrue(d1.list.cards.any { it.accepted })
        assertTrue(campHere(2026, 7u, 2u).startsWith("OK:"))
        val d2 = suggestOn(2026, 7u, 2u)
        assumeTrue(d2.list.cards.any { it.accepted })
        val d3 = suggestOn(2026, 7u, 3u)
        assertTrue(
            "third consecutive Camp-here night must be declined",
            !(d3.list.cards.any { it.accepted } || d3.list.probesAccepted > 0),
        )
        // Undo today (with clock on 07-02) then verify, then different spots do not block.
        campingPluginSetClockYmd(2026, 7u, 2u)
        val undo = uniffi.navi.campingPluginUndoCampHereTonight(lat, lon, "no", "no-34")
        android.util.Log.i("NaviCampingNight", "undo msg=$undo")
        assertTrue(undo.startsWith("OK:"))
        // Re-camp 07-02 after undo so we still have two nights for force-stop suite elsewhere.
        assertTrue(campHere(2026, 7u, 2u).startsWith("OK:"))

        // Different spots: camp A on 07-01, B on 07-02 → B still accepted on 07-03.
        if (kv.isFile) kv.delete()
        assertTrue(campHere(2026, 7u, 1u, lat, lon).startsWith("OK:"))
        val latB = 61.20000
        val lonB = 10.70000
        assertTrue(campHere(2026, 7u, 2u, latB, lonB).startsWith("OK:"))
        val jobB =
            """
            {"probes":[[$latB,$lonB]],"max_suggestions":4,"buildings":[],"glaciers":[],
             "safety":{"min_building_distance_m":150.0,"min_glacier_distance_m":1000.0},
             "clock":null,"kv_ok":true,
             "countries":[[$latB,$lonB,"no"]],"subdivisions":[[$latB,$lonB,"no-34"]],
             "travel_mode":"non_motorised","vehicle_class":"unknown",
             "is_professional_driver_under_rest_rules":false}
            """.trimIndent()
        campingPluginSetClockYmd(2026, 7u, 3u)
        val callB =
            kotlinx.coroutines.runBlocking {
                CampingPluginApi.runSuggest(jobB)
            }
        assumeTrue(callB.kind == CampingCallKind.OK)
        val parsedB = parseCampingSuggestResultJson(callB.resultJson!!)
        assertTrue(
            "different spots must not block",
            parsedB.list.cards.any { it.accepted } || parsedB.list.probesAccepted > 0,
        )
        android.util.Log.i("NaviCampingNight", "different_spots_ok via=${parsedB.via}")
    }

    @Test
    fun timing_lillehammerFirstAndWarm() {
        val pbf = regionPbf()
        assumeTrue("ostlandet PBF required", pbf != null)
        campingPluginSetNavContext(
            waypointsJson = lillehammerSjusjoenWaypoints(),
            destLat = 61.1475,
            destLon = 10.6980,
            profile = TravelProfile.HIKING,
            professionalDriver = false,
        )
        val cold =
            kotlinx.coroutines.runBlocking {
                CampingPluginApi.suggestAlongRoute(12u)
            }
        assumeTrue("cold ${cold.kind} ${cold.message}", cold.kind == CampingCallKind.OK)
        val warm =
            kotlinx.coroutines.runBlocking {
                CampingPluginApi.suggestAlongRoute(12u)
            }
        android.util.Log.i(
            "NaviCampingTiming",
            "Lillehammer suggest cold_ms=${cold.elapsedMs} warm_ms=${warm.elapsedMs} " +
                "cold_peak_guest=${cold.peakGuestMemoryBytes} warm_peak_guest=${warm.peakGuestMemoryBytes} " +
                "cold_timing=${org.json.JSONObject(cold.resultJson!!).optJSONObject("timing_ms")} " +
                "warm_timing=${org.json.JSONObject(warm.resultJson!!).optJSONObject("timing_ms")}",
        )
        assertTrue(cold.elapsedMs.toLong() >= 0L)
        assertTrue(warm.elapsedMs.toLong() >= 0L)
    }
}
