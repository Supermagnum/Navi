package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiTollPolicy
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.ensureFerrySidecar
import uniffi.navi.planCarRoute
import uniffi.navi.setRoutePlanTimingEnabled
import java.io.File

/**
 * Phase 1 A.5 helpers: ferry sidecar build timings and Stavanger contract.
 */
@RunWith(AndroidJUnit4::class)
class FerrySidecarBuildInstrumentedTest {
    private val tag = "FerrySidecarBuild"

    private val stems =
        listOf(
            "ostlandet-latest",
            "vestlandet-latest",
            "trondelag-latest",
            "nord-norge-latest",
            "sorlandet-latest",
        )

    private fun packDir(): File {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        return File(ctx.filesDir, "long-trip-packs")
    }

    @Test
    fun a5_vestlandet_stavanger_contract() {
        val dir = packDir()
        assertTrue(dir.isDirectory)
        File(dir, "vestlandet-latest.navi-ferry-overlay-car.rkyv").delete()
        File(dir, "vestlandet-latest.navi-ferry-overlay-car.meta").delete()
        val t0 = System.nanoTime()
        val report = ensureFerrySidecar(dir.absolutePath, "vestlandet-latest", TravelProfile.CAR)
        val buildMs = (System.nanoTime() - t0) / 1_000_000L
        val side = File(dir, "vestlandet-latest.navi-ferry-overlay-car.rkyv")
        Log.i(tag, "VEST_BUILD ms=$buildMs bytes=${side.length()} report=${report.trim()}")
        assertTrue(report.contains("PASS"))
        assertTrue(side.isFile)

        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val vestPbf = File(dir, "vestlandet-latest.osm.pbf").absolutePath
        File(ctx.filesDir, "graph-cache-ferry-a5b").mkdirs()
        setRoutePlanTimingEnabled(true)
        // Same coords as RegionToRegionPerfMatrixInstrumentedTest (contract 228.21).
        val bergenLat = 60.388144
        val bergenLon = 5.3347434
        val stavLat = 58.969975
        val stavLon = 5.733107
        val off =
            planCarRoute(
                pbfPath = vestPbf,
                elevDir = File(ctx.filesDir, "elevation").absolutePath,
                cacheDir = File(ctx.filesDir, "graph-cache-ferry-a5b").absolutePath,
                startLat = bergenLat,
                startLon = bergenLon,
                endLat = stavLat,
                endLon = stavLon,
                useEco = false,
                profile = TravelProfile.CAR,
                avoidMotorways = false,
                tollPolicy = FfiTollPolicy.ALLOW,
                avoidFerries = false,
                avoidTunnels = false,
                vehicle = FfiVehicleLimits(null, null, null, null, null, null),
                preferOfficialNetworks = false,
                dataDir = ctx.filesDir.absolutePath,
                packDir = dir.absolutePath,
                longTripEnabled = false,
                allowedCountries = null,
                viaPoints = emptyList(),
            )
        val on =
            planCarRoute(
                pbfPath = vestPbf,
                elevDir = File(ctx.filesDir, "elevation").absolutePath,
                cacheDir = File(ctx.filesDir, "graph-cache-ferry-a5b").absolutePath,
                startLat = bergenLat,
                startLon = bergenLon,
                endLat = stavLat,
                endLon = stavLon,
                useEco = false,
                profile = TravelProfile.CAR,
                avoidMotorways = false,
                tollPolicy = FfiTollPolicy.ALLOW,
                avoidFerries = false,
                avoidTunnels = false,
                vehicle = FfiVehicleLimits(null, null, null, null, null, null),
                preferOfficialNetworks = false,
                dataDir = ctx.filesDir.absolutePath,
                packDir = dir.absolutePath,
                longTripEnabled = true,
                allowedCountries = null,
                viaPoints = emptyList(),
            )

        fun fpOf(r: uniffi.navi.CorridorRouteResult): String {
            val matches = Regex("""route_ferry_fp=([^\n;]+)""").findAll(r.report).toList()
            return matches
                .lastOrNull()
                ?.groupValues
                ?.getOrNull(1)
                .orEmpty()
        }
        val fpOff = fpOf(off)
        val fpOn = fpOf(on)
        Log.i(tag, "STAV_OFF dist=${off.distanceKm} fp=$fpOff terminate=${off.searchTerminateReason}")
        Log.i(tag, "STAV_ON dist=${on.distanceKm} fp=$fpOn terminate=${on.searchTerminateReason}")
        Log.i(tag, "STAV_OFF_REPORT\n${off.report}")
        Log.i(tag, "STAV_ON_REPORT\n${on.report}")
        setRoutePlanTimingEnabled(false)
        val expectFp = "Halhjem - Sandvikvåg@21.32|Arsvågen - Mortavika@9.15"
        assertTrue("longTrip on must PASS:\n${on.report}", on.report.contains("PASS"))
        assertTrue(
            "longTrip on distance ~228.21 got=${on.distanceKm} fp=$fpOn",
            kotlin.math.abs(on.distanceKm - 228.21) < 0.05,
        )
        assertTrue("longTrip on ferry fp: $fpOn", fpOn.contains("Halhjem") && fpOn.contains("Arsvågen"))
        assertTrue("longTrip off must PASS:\n${off.report}", off.report.contains("PASS"))
        assertTrue(
            "longTrip off distance ~228.21 got=${off.distanceKm} fp=$fpOff",
            kotlin.math.abs(off.distanceKm - 228.21) < 0.05,
        )
        assertTrue(
            "longTrip off/on ferry fp must match ($expectFp): off=$fpOff on=$fpOn",
            fpOff == fpOn && fpOn.contains("Halhjem") && fpOn.contains("Arsvågen"),
        )
        Log.i(
            tag,
            "A4_COMPARE off_ok=true off_dist=${off.distanceKm} on_dist=${on.distanceKm} same_fp=true",
        )
    }

    @Test
    fun a5_first_plan_while_sidecar_building() {
        val dir = packDir()
        assertTrue(dir.isDirectory)
        File(dir, "vestlandet-latest.navi-ferry-overlay-car.rkyv").delete()
        File(dir, "vestlandet-latest.navi-ferry-overlay-car.meta").delete()
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val vestPbf = File(dir, "vestlandet-latest.osm.pbf").absolutePath
        File(ctx.filesDir, "graph-cache-ferry-a5c").mkdirs()
        // Kick background build without waiting.
        Thread {
            ensureFerrySidecar(dir.absolutePath, "vestlandet-latest", TravelProfile.CAR)
        }.start()
        Thread.sleep(500)
        setRoutePlanTimingEnabled(true)
        val t0 = System.nanoTime()
        var lastTerminate = ""
        var lastStatus = ""
        var route: uniffi.navi.CorridorRouteResult? = null
        // Poll like MainActivity planKick (~1.5 s) until route or timeout.
        while ((System.nanoTime() - t0) / 1_000_000L < 180_000L) {
            val r =
                planCarRoute(
                    pbfPath = vestPbf,
                    elevDir = File(ctx.filesDir, "elevation").absolutePath,
                    cacheDir = File(ctx.filesDir, "graph-cache-ferry-a5c").absolutePath,
                    startLat = 60.388144,
                    startLon = 5.3347434,
                    endLat = 58.969975,
                    endLon = 5.733107,
                    useEco = false,
                    profile = TravelProfile.CAR,
                    avoidMotorways = false,
                    tollPolicy = FfiTollPolicy.ALLOW,
                    avoidFerries = false,
                    avoidTunnels = false,
                    vehicle = FfiVehicleLimits(null, null, null, null, null, null),
                    preferOfficialNetworks = false,
                    dataDir = ctx.filesDir.absolutePath,
                    packDir = dir.absolutePath,
                    longTripEnabled = false,
                    allowedCountries = null,
                    viaPoints = emptyList(),
                )
            lastTerminate = r.searchTerminateReason
            lastStatus =
                Regex("""ferry_preparing_status=([^\n]+)""")
                    .find(r.report)
                    ?.groupValues
                    ?.getOrNull(1)
                    .orEmpty()
            Log.i(
                tag,
                "FIRST_PLAN_POLL terminate=$lastTerminate status=$lastStatus dist=${r.distanceKm} " +
                    "elapsed_ms=${(System.nanoTime() - t0) / 1_000_000L}",
            )
            if (r.distanceKm > 1.0 && r.report.contains("PASS")) {
                route = r
                break
            }
            assertTrue(
                "expected ferry_preparing while sidecar builds, got terminate=$lastTerminate",
                lastTerminate == "ferry_preparing" || lastTerminate.isBlank(),
            )
            Thread.sleep(1500)
        }
        setRoutePlanTimingEnabled(false)
        val waitMs = (System.nanoTime() - t0) / 1_000_000L
        Log.i(tag, "FIRST_PLAN_DONE wait_ms=$waitMs terminate=$lastTerminate status=$lastStatus")
        val ok = route
        assertTrue("route must appear after sidecar ready (wait_ms=$waitMs)", ok != null)
        assertTrue(
            "distance ~228.21 got=${ok!!.distanceKm}",
            kotlin.math.abs(ok.distanceKm - 228.21) < 0.05,
        )
    }

    @Test
    fun a5_background_build_per_stem() {
        val dir = packDir()
        assertTrue(dir.isDirectory)
        dir.listFiles()?.forEach { f ->
            val n = f.name
            if (n.contains("navi-ferry-overlay") && (n.endsWith(".rkyv") || n.endsWith(".meta"))) {
                f.delete()
            }
        }
        val rows = mutableListOf("stem\tbuild_ms\tout_bytes\treport")
        for (stem in stems) {
            val pbf = File(dir, "$stem.osm.pbf")
            if (!pbf.isFile || pbf.length() < 1_000_000L) {
                rows += "$stem\t-\t-\tskip_no_pbf"
                continue
            }
            val t0 = System.nanoTime()
            val report = ensureFerrySidecar(dir.absolutePath, stem, TravelProfile.CAR)
            val ms = (System.nanoTime() - t0) / 1_000_000L
            val side = File(dir, "$stem.navi-ferry-overlay-car.rkyv")
            val bytes = if (side.isFile) side.length() else 0L
            val line = "$stem\t$ms\t$bytes\t${report.trim().replace('\n', ' ')}"
            rows += line
            Log.i(tag, "BUILD_ROW $line")
        }
        val table = rows.joinToString("\n")
        File(dir, "ferry_sidecar_build_timings.tsv").writeText(table)
        Log.i(tag, "BUILD_TABLE\n$table")
    }
}
