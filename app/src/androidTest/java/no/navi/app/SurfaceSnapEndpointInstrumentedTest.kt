package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.planCarRoute
import java.io.File

/**
 * On-device (SM-P613) confirmation that route destinations snap to the literal
 * nearest routable node, not a paved road hundreds of metres away.
 *
 * Uses the installed ostlandet car packs under the app files dir.
 */
@RunWith(AndroidJUnit4::class)
class SurfaceSnapEndpointInstrumentedTest {
    private lateinit var dataDir: File

    companion object {
        private const val TAG = "SurfaceSnapEndpoint"

        // Espedalsvegen 656 — old bug snapped ~125 m onto paved.
        private val ESPEDALSVEGEN = 61.3636391 to 9.6735332

        // Søndre Grøtting, Rendalen — old bug snapped ~260 m onto paved.
        private val SONDRE_GROTTING = 61.865580 to 10.898674

        /** Nearby start points on the public network (same region). */
        private val START_NEAR_ESPEDAL = 61.3700 to 9.6800
        private val START_NEAR_GROTTING = 61.8700 to 10.9050

        private const val ENDPOINT_MAX_SNAP_M = 50.0
    }

    @Before
    fun setUp() {
        dataDir = NaviAppData.resolve(InstrumentationRegistry.getInstrumentation().targetContext)
        val carTile =
            File(dataDir, "ostlandet-latest.navi-graph-car.t0_0.rkyv")
        assertTrue(
            "missing ostlandet car packs under ${dataDir.absolutePath}",
            carTile.isFile,
        )
        val pbf = File(dataDir, "ostlandet-latest.osm.pbf")
        assertTrue("missing ostlandet PBF under ${dataDir.absolutePath}", pbf.isFile)
    }

    private fun parseSnapEndM(report: String): Double {
        // snap_end=lat,lon dist_m=NN;  or older snap_end_m=NN
        val modern =
            Regex("""snap_end=[-\d.]+,[-\d.]+\s+dist_m=([\d.]+)""")
                .find(report)
                ?.groupValues
                ?.get(1)
                ?.toDoubleOrNull()
        if (modern != null) return modern
        return Regex("""snap_end_m=([\d.]+)""")
            .find(report)
            ?.groupValues
            ?.get(1)
            ?.toDoubleOrNull()
            ?: error("no snap_end distance in report:\n${report.take(1200)}")
    }

    private fun planTo(
        label: String,
        start: Pair<Double, Double>,
        end: Pair<Double, Double>,
    ): Double {
        val pbf = File(dataDir, "ostlandet-latest.osm.pbf")
        val result =
            planCarRoute(
                pbfPath = pbf.absolutePath,
                elevDir = File(dataDir, "elevation").absolutePath,
                cacheDir = File(dataDir, "graph-cache").absolutePath,
                startLat = start.first,
                startLon = start.second,
                endLat = end.first,
                endLon = end.second,
                useEco = false,
                profile = TravelProfile.CAR,
                avoidMotorways = false,
                tollPolicy = uniffi.navi.FfiTollPolicy.ALLOW,
                avoidFerries = false,
                avoidTunnels = false,
                vehicle = FfiVehicleLimits(null, null, null, null, null, null),
                preferOfficialNetworks = false,
                dataDir = dataDir.absolutePath,
                packDir = "",
                longTripEnabled = false,
                viaPoints = emptyList(),
            )
        Log.i(TAG, "$label report_head=${result.report.take(900)}")
        assertTrue(
            "$label plan must PASS: ${result.report.take(600)}",
            result.report.contains("PASS"),
        )
        val snapEnd = parseSnapEndM(result.report)
        Log.i(TAG, "$label snap_end_m=$snapEnd")
        return snapEnd
    }

    @Test
    fun espedalsvegen_656_destination_snap_within_50m() {
        val snap =
            planTo(
                "Espedalsvegen656",
                START_NEAR_ESPEDAL,
                ESPEDALSVEGEN,
            )
        assertTrue(
            "Espedalsvegen 656 destination snap ${snap}m exceeds ${ENDPOINT_MAX_SNAP_M}m (old bug ~125m)",
            snap <= ENDPOINT_MAX_SNAP_M,
        )
    }

    @Test
    fun sondre_grotting_destination_snap_within_50m() {
        val snap =
            planTo(
                "SondreGrotting",
                START_NEAR_GROTTING,
                SONDRE_GROTTING,
            )
        assertTrue(
            "Søndre Grøtting destination snap ${snap}m exceeds ${ENDPOINT_MAX_SNAP_M}m (old bug ~260m)",
            snap <= ENDPOINT_MAX_SNAP_M,
        )
    }
}
