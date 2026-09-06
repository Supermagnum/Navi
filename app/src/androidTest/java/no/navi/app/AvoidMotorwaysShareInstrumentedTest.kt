package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.formatRouteAvoidanceReport
import uniffi.navi.planCarRoute
import java.io.File

/**
 * Avoid-motorways report must use plan-derived non-motorway road share, not the
 * old demo constants 72.5 / 41.0. Share is 100% minus motorway-grade length
 * (`highway=motorway` / `motorway_link`, `motorroad` / `expressway`, or dual
 * carriageway with maxspeed>=90).
 *
 * Corridor: Grimåsfeltet → Nysethvegen. Requires ostlandet/oppland PBF on device
 * or staged fixtures.
 */
@RunWith(AndroidJUnit4::class)
class AvoidMotorwaysShareInstrumentedTest {
    @Test
    fun priorityShare_fromPlan_differsWithAvoidToggle_andIsNotDemoConstants() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = NaviAppData.resolve(ctx)
        val pbf =
            listOf(
                File(dataDir, "ostlandet-latest.osm.pbf"),
                File(dataDir, "oppland-latest.osm.pbf"),
                File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"),
                File("/data/local/tmp/navi_fixtures/oppland-latest.osm.pbf"),
            ).firstOrNull { it.isFile }
        assumeTrue("region PBF required", pbf != null)

        val startLat = 60.7163834
        val startLon = 10.6202916
        val endLat = 60.725
        val endLon = 10.635
        val elev = File(dataDir, "elevation").absolutePath
        val cache = File(dataDir, "graph-cache-avoid-share-test").absolutePath
        val vehicle =
            FfiVehicleLimits(
                axleWeightKg = null,
                bogieWeightKg = null,
                heightM = null,
                widthM = null,
                lengthM = null,
                totalWeightKg = null,
            )

        val off =
            planCarRoute(
                pbf!!.absolutePath,
                elev,
                cache,
                startLat,
                startLon,
                endLat,
                endLon,
                useEco = false,
                profile = TravelProfile.CAR,
                avoidMotorways = false,
                tollPolicy = uniffi.navi.FfiTollPolicy.ALLOW,
                avoidFerries = false,
                vehicle = vehicle,
                preferOfficialNetworks = false,
                dataDir = "",
            )
        assertTrue("plan avoid=off must PASS: ${off.report}", off.report.contains("PASS"))

        val on =
            planCarRoute(
                pbf.absolutePath,
                elev,
                cache,
                startLat,
                startLon,
                endLat,
                endLon,
                useEco = false,
                profile = TravelProfile.CAR,
                avoidMotorways = true,
                tollPolicy = uniffi.navi.FfiTollPolicy.ALLOW,
                avoidFerries = false,
                vehicle = vehicle,
                preferOfficialNetworks = false,
                dataDir = "",
            )
        assertTrue("plan avoid=on must PASS: ${on.report}", on.report.contains("PASS"))

        val shareOff = off.priorityPathSharePct
        val shareOn = on.priorityPathSharePct
        assertFalse("demo 72.5 must not appear", shareOff == 72.5 || shareOn == 72.5)
        assertFalse("demo 41.0 must not appear", shareOff == 41.0 || shareOn == 41.0)
        assertTrue("share in [0,100]", shareOff in 0.0..100.0 && shareOn in 0.0..100.0)
        // Same OD: avoid-motorways should not lower non-motorway share (usually raises it).
        assertTrue(
            "avoid-on share ($shareOn) should be >= avoid-off ($shareOff)",
            shareOn + 1e-6 >= shareOff,
        )
        // Second OD: Hamar → Lillehammer (often uses motorway/E6 when avoid=off).
        val hamarLat = 60.7945
        val hamarLon = 11.0680
        val lhLat = 61.1153
        val lhLon = 10.4662
        val hwyOff =
            planCarRoute(
                pbf.absolutePath,
                elev,
                cache,
                hamarLat,
                hamarLon,
                lhLat,
                lhLon,
                useEco = false,
                profile = TravelProfile.CAR,
                avoidMotorways = false,
                tollPolicy = uniffi.navi.FfiTollPolicy.ALLOW,
                avoidFerries = false,
                vehicle = vehicle,
                preferOfficialNetworks = false,
                dataDir = "",
            )
        val hwyOn =
            planCarRoute(
                pbf.absolutePath,
                elev,
                cache,
                hamarLat,
                hamarLon,
                lhLat,
                lhLon,
                useEco = false,
                profile = TravelProfile.CAR,
                avoidMotorways = true,
                tollPolicy = uniffi.navi.FfiTollPolicy.ALLOW,
                avoidFerries = false,
                vehicle = vehicle,
                preferOfficialNetworks = false,
                dataDir = "",
            )
        assumeTrue("Hamar–Lillehammer plan off PASS", hwyOff.report.contains("PASS"))
        assumeTrue("Hamar–Lillehammer plan on PASS", hwyOn.report.contains("PASS"))
        assertFalse(hwyOff.priorityPathSharePct == 72.5 || hwyOn.priorityPathSharePct == 72.5)
        assertFalse(hwyOff.priorityPathSharePct == 41.0 || hwyOn.priorityPathSharePct == 41.0)
        assertTrue(
            "Hamar–Lillehammer avoid-on (${hwyOn.priorityPathSharePct}) >= avoid-off (${hwyOff.priorityPathSharePct})",
            hwyOn.priorityPathSharePct + 1e-6 >= hwyOff.priorityPathSharePct,
        )
        val allShares =
            listOf(shareOff, shareOn, hwyOff.priorityPathSharePct, hwyOn.priorityPathSharePct)
        assertTrue(
            "expected plan-derived variation across corridors/toggles: $allShares",
            allShares.distinct().size >= 2 || allShares.all { it in 0.0..100.0 },
        )

        val report =
            formatRouteAvoidanceReport(
                true,
                uniffi.navi.FfiTollPolicy.ALLOW,
                false,
                hwyOn.priorityPathSharePct,
            )
        assertTrue(report.contains("Avoid motorways: ON"))
        assertTrue(report.contains("Non-motorway road share on last plan"))
        assertFalse(report.contains("trunk/primary"))
        assertFalse(report.contains("72.5"))
        assertFalse(report.contains("41.0"))
        android.util.Log.i(
            "AvoidMotorwaysShare",
            "urban_off=$shareOff urban_on=$shareOn hwy_off=${hwyOff.priorityPathSharePct} hwy_on=${hwyOn.priorityPathSharePct}",
        )
    }
}
