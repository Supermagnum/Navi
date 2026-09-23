package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiTollPolicy
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.planCarRoute
import java.io.File

/**
 * On-device gate check: ordinary mid-span car plans with long-trip OFF must not
 * densify/chunk (Hamar→Dombås / Dombås→Bolleland E6-class distances).
 */
@RunWith(AndroidJUnit4::class)
class LongTripChunkGateInstrumentedTest {
    @Test
    fun hamar_dombas_and_dombas_bolleland_long_trip_off_e6_class() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = NaviAppData.resolve(ctx)
        val pbf =
            listOf(
                File(dataDir, "ostlandet-latest.osm.pbf"),
                File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"),
            ).firstOrNull { it.isFile }
        assumeTrue("Ostlandet PBF required on device", pbf != null)

        val elev = File(dataDir, "elevation").absolutePath
        val vehicle =
            FfiVehicleLimits(null, null, null, null, null, null)

        fun plan(
            startLat: Double,
            startLon: Double,
            endLat: Double,
            endLon: Double,
            tag: String,
        ) = planCarRoute(
            pbfPath = pbf!!.absolutePath,
            elevDir = elev,
            cacheDir = File(dataDir, "graph-cache-chunk-gate-$tag").absolutePath,
            startLat = startLat,
            startLon = startLon,
            endLat = endLat,
            endLon = endLon,
            useEco = false,
            profile = TravelProfile.CAR,
            avoidMotorways = false,
            tollPolicy = FfiTollPolicy.ALLOW,
            avoidFerries = false,
            avoidTunnels = false,
            vehicle = vehicle,
            preferOfficialNetworks = false,
            dataDir = dataDir.absolutePath,
            packDir = "",
            longTripEnabled = false,
            viaPoints = emptyList(),
        )

        val hamarDombas =
            plan(60.792206, 11.085951, 62.0755539, 9.1278983, "hamar-dombas")
        Log.i(
            TAG,
            "Hamar→Dombås OFF km=${hamarDombas.distanceKm} eta=${hamarDombas.etaMinutes} " +
                "chunked=${hamarDombas.report.contains("long_trip_chunked=true")}",
        )
        Log.i(TAG, "Hamar→Dombås full report:\n${hamarDombas.report}")
        assertTrue(
            "Hamar→Dombås must PASS:\n${hamarDombas.report}",
            hamarDombas.report.contains("PASS"),
        )
        assertTrue(
            "must not densify when long-trip OFF",
            !hamarDombas.report.contains("long_trip_chunked=true"),
        )
        assertTrue(
            "E6-class ~213 km, not ~327 densify detour; got ${hamarDombas.distanceKm}",
            hamarDombas.distanceKm in 190.0..250.0,
        )

        val dombasBolle =
            plan(62.0755539, 9.1278983, 60.562578, 11.256970, "dombas-bolle")
        Log.i(
            TAG,
            "Dombås→Bolleland OFF km=${dombasBolle.distanceKm} eta=${dombasBolle.etaMinutes} " +
                "chunked=${dombasBolle.report.contains("long_trip_chunked=true")}",
        )
        assertTrue("Dombås→Bolleland must PASS", dombasBolle.report.contains("PASS"))
        assertTrue(
            "must not densify when long-trip OFF",
            !dombasBolle.report.contains("long_trip_chunked=true"),
        )
        assertTrue(
            "E6-class ~242 km, not ~347 densify detour; got ${dombasBolle.distanceKm}",
            dombasBolle.distanceKm in 210.0..280.0,
        )
    }

    companion object {
        private const val TAG = "LongTripChunkGate"
    }
}
