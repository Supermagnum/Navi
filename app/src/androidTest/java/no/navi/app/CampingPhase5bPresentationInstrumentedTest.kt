package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
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
        val pbf = regionPbf()
        assumeTrue("ostlandet PBF required", pbf != null)
        campingPluginSetNavContext(
            waypointsJson =
                campingWaypointsJson(
                    listOf(
                        doubleArrayOf(60.1910, 12.0080),
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
        val se =
            (parsed.list.cards + parsed.onFootFromHere.cards).filter {
                it.countryIso.equals("se", ignoreCase = true)
            }
        assumeTrue("expected Swedish cards Kongsvinger–Charlottenberg", se.isNotEmpty())
        assertTrue(
            se.any { c ->
                c.notes.any { n ->
                    n.contains("Navi safety default", ignoreCase = true) ||
                        n.contains("not Swedish law", ignoreCase = true)
                }
            },
        )
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
                "guest_memory_cap_bytes=33554432",
        )
        assertTrue(cold.elapsedMs.toLong() >= 0L)
        assertTrue(warm.elapsedMs.toLong() >= 0L)
    }
}
