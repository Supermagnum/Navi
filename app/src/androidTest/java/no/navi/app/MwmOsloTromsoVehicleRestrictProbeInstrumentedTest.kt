package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import org.json.JSONObject
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.FixMethodOrder
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters
import uniffi.navi.FfiComapsProfile
import uniffi.navi.FfiComapsRouteCode
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.comapsSessionSnapSuggestion
import uniffi.navi.saveVehicleLimits
import java.io.File
import kotlin.math.asin
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Device-only long-route probe: Oslo→Tromsø with a test-only height limit on the
 * main Oppland corridor segment. Not for CI. Host injects the sidecar between
 * [a01_baseline_dump_oppland_segment] and [a02_truck_avoids_injected_restriction].
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class MwmOsloTromsoVehicleRestrictProbeInstrumentedTest {
    private val h = ComapsPhase6Harness

    private val oslo = 59.9139 to 10.7522
    private val tromso = 69.6492 to 18.9553

    @Before
    fun openSession() {
        assumeTrue("MWM client flag", MwmClientBootstrap.isEnabled())
        assertTrue(
            "CoMaps session",
            ComapsSessionBootstrap.ensureOpen(h.context, force = false),
        )
        assertTrue("production planner", MwmProductionPlanner.shouldUse(h.context))
    }

    @Test
    fun a01_baseline_dump_oppland_segment() {
        setLimits(heightM = null)
        val wall0 = System.nanoTime()
        val r =
            plan(
                TravelProfile.CAR,
                oslo.first,
                oslo.second,
                tromso.first,
                tromso.second,
            )
        val wallS = (System.nanoTime() - wall0) / 1e9
        assertTrue("baseline PASS\n${r.report.take(500)}", r.report.contains("PASS"))
        assertTrue("baseline distance", r.distanceKm > 1000.0)

        val pts = decodePolyline(r.routePolyline)
        assertTrue("baseline polyline", pts.size > 100)
        // Oppland corridor sample: Gudbrandsdalen / E6 band.
        val candidates =
            pts.filter { (la, lo) ->
                la in 61.0..62.2 && lo in 8.5..11.0
            }
        assertTrue("need Oppland-band points on baseline", candidates.isNotEmpty())
        val sample = candidates[candidates.size / 2]
        val snap =
            comapsSessionSnapSuggestion(
                FfiComapsProfile.CAR,
                FfiComapsRouteCode.START_NOT_ON_ROAD,
                sample.first,
                sample.second,
                80.0,
            )
        assertTrue(
            "snap for inject target found=${snap.found} err=${snap.errorMessage} mwm=${snap.mwmName}",
            snap.found && snap.featureId > 0u,
        )
        val mwm = snap.mwmName ?: ""
        assertTrue("snap mwm Oppland-ish: $mwm", mwm.contains("Oppland") || mwm.contains("Hedmark") || mwm.contains("Central"))

        val mode =
            r.report.lineSequence().firstOrNull { it.startsWith("router_mode=") } ?: ""
        val build =
            r.report.lineSequence().firstOrNull { it.startsWith("build_s=") } ?: ""
        val vlOk =
            r.report.lineSequence().firstOrNull { it.startsWith("vehicle_limits_ok=") } ?: ""

        val dump =
            JSONObject()
                .put("phase", "baseline")
                .put("pass", true)
                .put("distance_km", r.distanceKm)
                .put("wall_s", wallS)
                .put("build_line", build)
                .put("router_mode_line", mode)
                .put("vehicle_limits_ok_line", vlOk)
                .put("sample_lat", sample.first)
                .put("sample_lon", sample.second)
                .put("snap_lat", snap.snapLat)
                .put("snap_lon", snap.snapLon)
                .put("feature_id", snap.featureId.toLong())
                .put("segment_idx", snap.segmentIdx.toLong())
                .put("mwm_name", mwm)
                .put(
                    "report_head",
                    r.report
                        .lineSequence()
                        .take(20)
                        .joinToString("\n"),
                )
        writeProbe(dump)
        android.util.Log.i("OsloTromsoProbe", dump.toString())
    }

    @Test
    fun a02_truck_avoids_injected_restriction() {
        val dumpFile = File("/data/local/tmp/oslo_tromso_vl_probe.json")
        assumeTrue("baseline dump present", dumpFile.isFile)
        val dump = JSONObject(dumpFile.readText())
        assumeTrue("inject applied flag", dump.optBoolean("inject_applied", false))
        val fid = dump.getLong("feature_id")
        val seg = dump.getLong("segment_idx")
        val rLat = dump.getDouble("snap_lat")
        val rLon = dump.getDouble("snap_lon")
        val mwm = dump.getString("mwm_name")

        // Force sidecar reload after host push.
        assertTrue(
            "reopen session after inject",
            ComapsSessionBootstrap.ensureOpen(h.context, force = true),
        )

        setLimits(heightM = 2.8)
        val wall0 = System.nanoTime()
        val r =
            plan(
                TravelProfile.TRUCK,
                oslo.first,
                oslo.second,
                tromso.first,
                tromso.second,
            )
        val wallS = (System.nanoTime() - wall0) / 1e9
        val mode =
            r.report.lineSequence().firstOrNull { it.startsWith("router_mode=") } ?: ""
        val build =
            r.report.lineSequence().firstOrNull { it.startsWith("build_s=") } ?: ""
        val vlOk =
            r.report.lineSequence().firstOrNull { it.startsWith("vehicle_limits_ok=") } ?: ""

        val blocked =
            r.report.contains("FAIL") ||
                r.report.contains("ROUTE_NOT_FOUND", ignoreCase = true) ||
                r.report.contains("No route without passing a restriction")
        val near =
            if (r.routePolyline.length > 50) {
                routeNear(r.routePolyline, rLat, rLon, 40.0)
            } else {
                false
            }
        val detoured =
            r.report.contains("PASS") &&
                r.distanceKm > 1000.0 &&
                !near

        val pass = blocked || detoured
        val out =
            JSONObject()
                .put("phase", "restricted")
                .put("pass", pass)
                .put("blocked", blocked)
                .put("detoured", detoured)
                .put("near_restricted", near)
                .put("distance_km", r.distanceKm)
                .put("wall_s", wallS)
                .put("build_line", build)
                .put("router_mode_line", mode)
                .put("vehicle_limits_ok_line", vlOk)
                .put("inject_mwm", mwm)
                .put("inject_feature_id", fid)
                .put("inject_segment_idx", seg)
                .put("inject_lat", rLat)
                .put("inject_lon", rLon)
                .put(
                    "report_head",
                    r.report
                        .lineSequence()
                        .take(30)
                        .joinToString("\n"),
                )
        writeProbe(out)
        android.util.Log.i("OsloTromsoProbe", out.toString())

        assertTrue(
            "truck must avoid injected maxheight=2.7 on corridor: $out",
            pass,
        )
    }

    private fun plan(
        profile: TravelProfile,
        startLat: Double,
        startLon: Double,
        endLat: Double,
        endLon: Double,
    ) = MwmProductionPlanner.plan(
        context = h.context,
        profile = profile,
        startLat = startLat,
        startLon = startLon,
        endLat = endLat,
        endLon = endLon,
        vias = emptyList(),
        avoidMotorways = false,
        avoidTolls = false,
        avoidFerries = false,
        turnsLocale = "nb",
    )

    private fun setLimits(heightM: Double?) {
        val dataDir = NaviAppData.resolve(h.context)
        assertTrue(
            "persist limits",
            saveVehicleLimits(
                dataDir.absolutePath,
                FfiVehicleLimits(
                    heightM = heightM,
                    widthM = null,
                    lengthM = null,
                    totalWeightKg = null,
                    axleWeightKg = null,
                    bogieWeightKg = null,
                ),
            ),
        )
    }

    private fun writeProbe(obj: JSONObject) {
        val dataDir = NaviAppData.resolve(h.context)
        File(dataDir, "oslo_tromso_vl_probe.json").writeText(obj.toString(2))
        h.shell(
            "run-as no.navi.app cat files/oslo_tromso_vl_probe.json > /data/local/tmp/oslo_tromso_vl_probe.json; " +
                "chmod 666 /data/local/tmp/oslo_tromso_vl_probe.json",
        )
    }

    private fun routeNear(
        poly: String,
        rlat: Double,
        rlon: Double,
        thresholdM: Double,
    ): Boolean {
        val pts = decodePolyline(poly)
        if (pts.isEmpty()) return false
        return pts.minOf { (la, lo) -> haversineM(la, lo, rlat, rlon) } <= thresholdM
    }

    private fun decodePolyline(poly: String): List<Pair<Double, Double>> =
        poly.split(';').mapNotNull { part ->
            val bits = part.split(',')
            if (bits.size < 2) return@mapNotNull null
            val lo = bits[0].toDoubleOrNull() ?: return@mapNotNull null
            val la = bits[1].toDoubleOrNull() ?: return@mapNotNull null
            la to lo
        }

    private fun haversineM(
        aLat: Double,
        aLon: Double,
        bLat: Double,
        bLon: Double,
    ): Double {
        val r = 6_371_000.0
        val p1 = Math.toRadians(aLat)
        val p2 = Math.toRadians(bLat)
        val dPhi = Math.toRadians(bLat - aLat)
        val dLam = Math.toRadians(bLon - aLon)
        val h =
            sin(dPhi / 2).pow(2) +
                cos(p1) * cos(p2) * sin(dLam / 2).pow(2)
        return 2 * r * asin(min(1.0, sqrt(h)))
    }
}
