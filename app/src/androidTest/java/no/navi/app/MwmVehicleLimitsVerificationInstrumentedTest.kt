package no.navi.app

import android.system.Os
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.FixMethodOrder
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.mwmInstallLeaves
import uniffi.navi.saveVehicleLimits
import java.io.File
import java.util.concurrent.CopyOnWriteArrayList
import kotlin.math.asin
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Device-only Samsung verification table for MWM vehicle_limits (scratch sidecars).
 *
 * Not part of PR CI: needs physical device `R52TB0JQEDE`, Nordic scratch sidecars,
 * and optional adb reverse to the scratch catalog. Run via
 * `connectedDebugAndroidTest` / `am instrument` locally. Ordered methods so the
 * summary file accumulates even when a later case fails.
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class MwmVehicleLimitsVerificationInstrumentedTest {
    private val h = ComapsPhase6Harness

    private val restrictedSamples =
        listOf(
            60.730013 to 11.186320,
            60.730004 to 11.186197,
        )

    private val auditStart = 60.7318000 to 11.1905000
    private val auditVia = 60.7307100 to 11.1866980
    private val auditEnd = 60.7228810 to 11.1530380

    // Opposite sides of Fokholgutua MWM fid 4577 (sidecar maxheight=2.7 m, seg 0).
    // Not a sole corridor: truck HardAvoid can detour ~1 km. a03 accepts block or that detour.
    private val onlyConnBefore = 60.7306 to 11.1868
    private val onlyConnAfter = 60.7299 to 11.1844

    private val stange = 60.717675 to 11.192379
    private val gjovik = 60.7955 to 10.6915

    private val lillehammer = 61.1151 to 10.4663
    private val bergen = 60.388114 to 5.333857

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
    fun a01_audit_small_car() {
        RESULTS.clear()
        setLimits(heightM = null, widthM = null)
        val r =
            plan(
                TravelProfile.CAR,
                auditStart.first,
                auditStart.second,
                auditEnd.first,
                auditEnd.second,
                listOf(auditVia),
            )
        val near = routeNearRestricted(r.routePolyline)
        val pass =
            r.report.contains("PASS") &&
                r.report.contains("vehicle_limits_ok=true") &&
                near
        record(
            "1a_audit_small_car",
            if (pass) "PASS" else "FAIL",
            "expect hug Fokholgutua without height; near_restricted=$near " + keyLines(r.report),
        )
        assertTrue("small car should use Fokholgutua when unrestricted", pass)
    }

    @Test
    fun a02_audit_large_truck() {
        setLimits(heightM = 2.8, widthM = null)
        val r =
            plan(
                TravelProfile.TRUCK,
                auditStart.first,
                auditStart.second,
                auditEnd.first,
                auditEnd.second,
                listOf(auditVia),
            )
        val near = routeNearRestricted(r.routePolyline)
        val cov = VehicleRestrictionUi.parseCoverage(r.report)
        val pass =
            r.report.contains("PASS") &&
                cov.dataAvailable &&
                !near &&
                r.distanceKm > 0.5
        record(
            "1b_audit_large_truck_2_8",
            if (pass) "PASS" else "FAIL",
            "expect avoid Fokholgutua; near_restricted=$near " + keyLines(r.report),
        )
        assertTrue("truck 2.8 must avoid maxheight=2.7", pass)
    }

    @Test
    fun a03_only_connection_blocked() {
        // Fid 4577 (Fokholgutua) carries sidecar maxheight=2.7; neighbors 4558/4578 do not.
        // This pin is not a sole corridor — truck HardAvoid finds ~1 km alternatives — so the
        // post-route guard only rejects when the finished route still contains 4577. Verify:
        // under-limit hugs the short crossing; over-limit is blocked by the guard OR detours
        // clear of the restriction (near_restricted=false, much longer).
        setLimits(heightM = null, widthM = null)
        val under =
            plan(
                TravelProfile.CAR,
                onlyConnBefore.first,
                onlyConnBefore.second,
                onlyConnAfter.first,
                onlyConnAfter.second,
                emptyList(),
            )
        setLimits(heightM = 2.8, widthM = null)
        val over =
            plan(
                TravelProfile.TRUCK,
                onlyConnBefore.first,
                onlyConnBefore.second,
                onlyConnAfter.first,
                onlyConnAfter.second,
                emptyList(),
            )
        val underNear = routeNearRestricted(under.routePolyline)
        val overNear = routeNearRestricted(over.routePolyline)
        val underOk =
            under.report.contains("PASS") && under.distanceKm > 0.02 && underNear
        val blocked =
            over.report.contains("FAIL") ||
                over.report.contains("ROUTE_NOT_FOUND", ignoreCase = true) ||
                over.report.contains("No route without passing a restriction") ||
                over.report.contains("comaps_route_code=ROUTE_NOT_FOUND") ||
                over.report.contains("comaps_route_code=RouteNotFound")
        val detoured =
            over.report.contains("PASS") &&
                !overNear &&
                over.distanceKm > under.distanceKm * 3.0 &&
                over.distanceKm > 0.5
        val pass = underOk && (blocked || detoured)
        record(
            "2_only_connection_blocked",
            if (pass) "PASS" else "FAIL",
            "fid4577 maxheight=2.7; under_km=${under.distanceKm} over_km=${over.distanceKm} " +
                "under_near=$underNear over_near=$overNear blocked=$blocked detoured=$detoured " +
                "under=${keyLines(under.report)} over=${keyLines(over.report)}",
        )
        assertTrue("height over 2.7 must block or detour clear of Fokholgutua limit", pass)
    }

    @Test
    fun a04_stange_gjovik() {
        setLimits(heightM = 2.8, widthM = null)
        val r =
            plan(
                TravelProfile.TRUCK,
                stange.first,
                stange.second,
                gjovik.first,
                gjovik.second,
                emptyList(),
            )
        val cov = VehicleRestrictionUi.parseCoverage(r.report)
        val pass =
            r.report.contains("PASS") &&
                cov.dataAvailable &&
                r.distanceKm > 20.0 &&
                r.distanceKm < 120.0
        record(
            "3_stange_gjovik",
            if (pass) "PASS" else "FAIL",
            "expect Hedmark/Oppland vehicle_limits_ok; " + keyLines(r.report),
        )
        assertTrue("Stange-Gjovik with vehicle_limits", pass)
    }

    @Test
    fun a05_leaps_over_300km() {
        setLimits(heightM = 2.8, widthM = null)
        val r =
            plan(
                TravelProfile.TRUCK,
                lillehammer.first,
                lillehammer.second,
                bergen.first,
                bergen.second,
                emptyList(),
            )
        val modeLine =
            r.report.lineSequence().firstOrNull { it.startsWith("router_mode=") } ?: ""
        val leaps =
            modeLine.contains("LeapsOnly", ignoreCase = true) ||
                Regex("""router_mode=Leaps\b""", RegexOption.IGNORE_CASE).containsMatchIn(modeLine)
        val longOk = r.report.contains("PASS") && r.distanceKm > 300.0
        val pass = longOk && leaps
        record(
            "4_leaps_over_300km",
            if (pass) "PASS" else "FAIL",
            "expect >300km + LeapsOnly; leaps=$leaps mode=$modeLine " + keyLines(r.report),
        )
        // Keep going for the table even when mode string is Joints on this pin.
        if (!pass && longOk) {
            android.util.Log.w(
                "VlVerify",
                "long route OK but router_mode not LeapsOnly: $modeLine",
            )
        }
        assertTrue("long Nordic route LeapsOnly >300km", pass)
    }

    @Test
    fun a06_coverage_banners() {
        val ctx = h.context
        val applied = ctx.getString(R.string.vehicle_limits_applied_where_available)
        val partialTpl = ctx.getString(R.string.vehicle_limits_partly_available_for_region)
        val missingTpl = ctx.getString(R.string.vehicle_limits_unavailable_for_region)
        val generic = ctx.getString(R.string.vehicle_limits_not_yet_applied_banner)

        val hedmarkPartial =
            VehicleLimitsCoverageStore.partialLeaves(ctx, listOf("Norway_Hedmark", "Norway_Oppland"))
        val finlandPartial =
            VehicleLimitsCoverageStore.partialLeaves(ctx, listOf("Finland_Northern Finland"))
        val hedmarkCov = VehicleLimitsCoverageStore.leafCoverage(ctx, "Norway_Hedmark")
        val finlandCov = VehicleLimitsCoverageStore.leafCoverage(ctx, "Finland_Northern Finland")

        val highBanner =
            VehicleRestrictionUi.routeWarningText(
                profile = TravelProfile.TRUCK,
                heightM = 2.8,
                dataAvailable = hedmarkPartial.isEmpty(),
                missingLeaves = emptyList(),
                genericBanner = generic,
                regionTemplate = missingTpl,
                partialLeaves = hedmarkPartial,
                partialTemplate = partialTpl,
                appliedTemplate = applied,
            )
        val lowBanner =
            VehicleRestrictionUi.routeWarningText(
                profile = TravelProfile.TRUCK,
                heightM = 2.8,
                dataAvailable = false,
                missingLeaves = emptyList(),
                genericBanner = generic,
                regionTemplate = missingTpl,
                partialLeaves = listOf("Finland_Northern Finland"),
                partialTemplate = partialTpl,
                appliedTemplate = applied,
            )

        setLimits(heightM = 2.8, widthM = null)
        val planR =
            plan(
                TravelProfile.TRUCK,
                auditStart.first,
                auditStart.second,
                auditEnd.first,
                auditEnd.second,
                listOf(auditVia),
            )
        val planCov = VehicleRestrictionUi.parseCoverage(planR.report)

        val highOk =
            hedmarkPartial.isEmpty() &&
                (hedmarkCov?.matchRate ?: 0.0) >= 0.90 &&
                highBanner == applied &&
                planCov.dataAvailable
        val lowOk =
            finlandPartial.contains("Finland_Northern Finland") &&
                (finlandCov?.matchRate ?: 1.0) < 0.90 &&
                lowBanner != null &&
                lowBanner.contains("Finland Northern Finland") &&
                lowBanner != applied

        record(
            "5a_coverage_banner_ge_0_90",
            if (highOk) "PASS" else "FAIL",
            "hedmark_rate=${hedmarkCov?.matchRate} partial=$hedmarkPartial banner=$highBanner " +
                "plan_ok=${planCov.dataAvailable}",
        )
        record(
            "5b_coverage_banner_lt_0_90",
            if (lowOk) "PASS" else "FAIL",
            "finland_rate=${finlandCov?.matchRate} partial=$finlandPartial banner=$lowBanner",
        )
        assertTrue(">=0.90 applied-where-available banner", highOk)
        assertTrue("<0.90 partial banner for Northern Finland", lowOk)
    }

    @Test
    fun a07_sidecar_refresh_same_release() {
        val dataDir = NaviAppData.resolve(h.context)
        val leafMwm = File(dataDir, "mwm/releases/261105/leaves/Norway_Hedmark.mwm")
        val leafSc = File(dataDir, "mwm/releases/261105/sidecars/Norway_Hedmark.navi-sidecar.v1.zst")
        assertTrue("Hedmark mwm present", leafMwm.isFile)
        assertTrue("Hedmark sidecar present", leafSc.isFile)

        val mwmSizeBefore = leafMwm.length()
        val mwmMtimeBefore = leafMwm.lastModified()
        val mwmShaBefore = sha256File(leafMwm)
        val scShaBefore = sha256File(leafSc)
        val wantSha = "7e8267fe767a6e529fa570d2ff7177d3e6d4e392c3f971861ea0eff16386d4c2"
        val oldSc = File("/data/local/tmp/Norway_Hedmark.OLD.navi-sidecar.v1.zst")
        val goodSc = File("/data/local/tmp/Norway_Hedmark.GOOD.navi-sidecar.v1.zst")
        assumeTrue("old sidecar staged on device", oldSc.isFile && oldSc.length() > 1000)
        assumeTrue("good sidecar staged on device", goodSc.isFile && goodSc.length() > 1000)

        try {
            oldSc.copyTo(leafSc, overwrite = true)
            val plantedSha = sha256File(leafSc)
            assertTrue("planted old sidecar differs", plantedSha != scShaBefore)

            val origin = "http://127.0.0.1:18080"
            Os.setenv("NAVI_PACK_SERVER_BASE_URL", origin, true)
            val res = mwmInstallLeaves(dataDir.absolutePath, origin, listOf("Norway_Hedmark"))
            ComapsSessionBootstrap.ensureOpen(h.context, force = true)

            val mwmSizeAfter = leafMwm.length()
            val mwmMtimeAfter = leafMwm.lastModified()
            val mwmShaAfter = sha256File(leafMwm)
            val scShaAfter = sha256File(leafSc)

            val sizeOk = mwmSizeAfter == mwmSizeBefore
            val contentOk = mwmShaAfter.equals(mwmShaBefore, ignoreCase = true)
            val sidecarOk = scShaAfter.equals(wantSha, ignoreCase = true)
            val mtimeOk = mwmMtimeAfter == mwmMtimeBefore
            val pass = res.ok && sizeOk && contentOk && sidecarOk

            record(
                "6_sidecar_refresh_same_release",
                if (pass) "PASS" else "FAIL",
                "install_ok=${res.ok} err=${res.error} downloaded=${res.downloadedLeaves} " +
                    "already=${res.alreadyInstalledLeaves} " +
                    "mwm_size_unchanged=$sizeOk mwm_sha_unchanged=$contentOk mwm_mtime_unchanged=$mtimeOk " +
                    "sidecar_sha_before=$scShaBefore planted=$plantedSha after=$scShaAfter want=$wantSha " +
                    "mtime_before=$mwmMtimeBefore mtime_after=$mwmMtimeAfter",
            )
            assertTrue("same-release sidecar SHA refresh without map content change", pass)
        } finally {
            goodSc.copyTo(leafSc, overwrite = true)
            ComapsSessionBootstrap.ensureOpen(h.context, force = true)
        }
    }

    @Test
    fun a99_flush_report() {
        flushReport()
        assertTrue("at least one case recorded", RESULTS.isNotEmpty())
    }

    private fun plan(
        profile: TravelProfile,
        startLat: Double,
        startLon: Double,
        endLat: Double,
        endLon: Double,
        vias: List<Pair<Double, Double>>,
    ) = MwmProductionPlanner.plan(
        context = h.context,
        profile = profile,
        startLat = startLat,
        startLon = startLon,
        endLat = endLat,
        endLon = endLon,
        vias = vias,
        avoidMotorways = false,
        avoidTolls = false,
        avoidFerries = false,
        turnsLocale = "nb",
    )

    private fun setLimits(
        heightM: Double?,
        widthM: Double?,
    ) {
        val dataDir = NaviAppData.resolve(h.context)
        assertTrue(
            "persist limits",
            saveVehicleLimits(
                dataDir.absolutePath,
                FfiVehicleLimits(
                    heightM = heightM,
                    widthM = widthM,
                    lengthM = null,
                    totalWeightKg = null,
                    axleWeightKg = null,
                    bogieWeightKg = null,
                ),
            ),
        )
    }

    private fun keyLines(report: String): String =
        report
            .lineSequence()
            .filter {
                it.startsWith("PASS") ||
                    it.startsWith("FAIL") ||
                    it.startsWith("distance_km=") ||
                    it.startsWith("router_mode=") ||
                    it.startsWith("comaps_route_code=") ||
                    it.startsWith("vehicle_limits_") ||
                    it.contains("No route without")
            }.joinToString(" | ")

    private fun record(
        caseId: String,
        result: String,
        evidence: String,
    ) {
        val line = "CASE=$caseId RESULT=$result EVIDENCE=$evidence"
        RESULTS.add(line)
        android.util.Log.i("VlVerify", line)
        // Keep a live file even if a later method crashes.
        flushReport()
    }

    private fun flushReport() {
        val body =
            buildString {
                appendLine("git_head=ecdd66dfa6836f0397e1daacd64f5e1aa6c4356b")
                appendLine("apk_sha256=8e365c527d2cb24304350950f490806f4dc311249983d509e7c40b66d678cb93")
                appendLine("sidecars=scratch Nordic 261105 Hedmark/Oppland/Hordaland/Western (+catalog coverage)")
                for (line in RESULTS) appendLine(line)
            }
        File(NaviAppData.resolve(h.context), "mwm_vehicle_limits_verification.txt").writeText(body)
        h.shell(
            "run-as no.navi.app cat files/mwm_vehicle_limits_verification.txt > /data/local/tmp/mwm_vehicle_limits_verification.txt; " +
                "chmod 666 /data/local/tmp/mwm_vehicle_limits_verification.txt",
        )
    }

    private fun routeNearRestricted(
        poly: String,
        thresholdM: Double = 25.0,
    ): Boolean {
        val pts = decodePolyline(poly)
        if (pts.isEmpty()) return false
        for ((rlat, rlon) in restrictedSamples) {
            val d = pts.minOf { (la, lo) -> haversineM(la, lo, rlat, rlon) }
            if (d <= thresholdM) return true
        }
        return false
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

    private fun sha256File(f: File): String {
        val md = java.security.MessageDigest.getInstance("SHA-256")
        f.inputStream().use { input ->
            val buf = ByteArray(64 * 1024)
            while (true) {
                val n = input.read(buf)
                if (n <= 0) break
                md.update(buf, 0, n)
            }
        }
        return md.digest().joinToString("") { b -> "%02x".format(b) }
    }

    companion object {
        private val RESULTS = CopyOnWriteArrayList<String>()
    }
}
