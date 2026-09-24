package no.navi.app

import android.os.Debug
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiTollPolicy
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.planCarRoute
import java.io.File

/**
 * Drammen→Berlevåg long-haul regression on Automotive AVD (4 GB).
 *
 * Locks E6-spine densify success numbers from the Stay-in-Country investigation:
 * Stay ON ≈2567.3 km / ~38 h; Stay OFF ≈2593 km. Requires Ready long-trip packs
 * for ostlandet/trondelag/nord-norge on device (SD FEF6-BB2E or app files).
 * Does not wipe packs.
 */
@RunWith(AndroidJUnit4::class)
class LongTripDrammenBerlevagStayOnTest {
    @Test
    fun drammen_berlevag_stay_on_and_off_success_numbers() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = NaviAppData.resolve(ctx)
        // Densify scans packDir + dataDir. SD holds nord-norge/trondelag Ready;
        // ostlandet Ready may live under dataDir (or SD). Do not wipe SD.
        val sdPacks =
            File("/mnt/media_rw/FEF6-BB2E/Android/data/no.navi.app/files/long-trip-packs")
        val internalPacks = File(dataDir, "long-trip-packs")
        fun hasManifest(
            dir: File,
            stem: String,
        ) = File(dir, "$stem.navi-manifest.json").isFile ||
            File(dataDir, "$stem.navi-manifest.json").isFile

        val packDir =
            when {
                sdPacks.isDirectory &&
                    hasManifest(sdPacks, "ostlandet-latest") &&
                    hasManifest(sdPacks, "trondelag-latest") &&
                    hasManifest(sdPacks, "nord-norge-latest") -> sdPacks
                internalPacks.isDirectory &&
                    hasManifest(internalPacks, "ostlandet-latest") &&
                    hasManifest(internalPacks, "trondelag-latest") &&
                    hasManifest(internalPacks, "nord-norge-latest") -> internalPacks
                sdPacks.isDirectory &&
                    hasManifest(sdPacks, "trondelag-latest") &&
                    hasManifest(sdPacks, "nord-norge-latest") &&
                    hasManifest(dataDir, "ostlandet-latest") -> sdPacks
                else -> null
            }
        assertTrue(
            "Need Ready ostlandet+trondelag+nord-norge (SD and/or dataDir). " +
                "sd=${sdPacks.absolutePath} data=${dataDir.absolutePath}",
            packDir != null,
        )
        val pbf =
            listOf(
                File(packDir, "ostlandet-latest.osm.pbf"),
                File(dataDir, "ostlandet-latest.osm.pbf"),
                File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"),
            ).firstOrNull { it.isFile }
        assertTrue("Ostlandet PBF required", pbf != null)

        val elev =
            listOf(
                File(packDir, "elevation/elevation"),
                File(packDir, "elevation"),
                File(dataDir, "elevation"),
            ).firstOrNull { it.isDirectory }?.absolutePath ?: File(dataDir, "elevation").absolutePath

        val vehicle = FfiVehicleLimits(null, null, null, null, null, null)

        fun plan(
            tag: String,
            allowed: List<String>?,
        ) = planCarRoute(
            pbfPath = pbf!!.absolutePath,
            elevDir = elev,
            cacheDir = File(dataDir, "graph-cache-drammen-berlevag-$tag").absolutePath,
            startLat = START_LAT,
            startLon = START_LON,
            endLat = END_LAT,
            endLon = END_LON,
            useEco = false,
            profile = TravelProfile.CAR,
            avoidMotorways = false,
            tollPolicy = FfiTollPolicy.ALLOW,
            avoidFerries = false,
            avoidTunnels = false,
            vehicle = vehicle,
            preferOfficialNetworks = false,
            dataDir = dataDir.absolutePath,
            packDir = packDir!!.absolutePath,
            longTripEnabled = true,
            allowedCountries = allowed,
            viaPoints = emptyList(),
        )

        val rss0 = Debug.getPss()
        val stayOn = plan("stay-on", listOf("no"))
        val rssOn = Debug.getPss()
        Log.i(
            TAG,
            "Drammen→Berlevåg Stay ON km=${stayOn.distanceKm} eta_min=${stayOn.etaMinutes} " +
                "terminate=${stayOn.searchTerminateReason} pss0_kb=$rss0 pss1_kb=$rssOn",
        )
        Log.i(TAG, "Stay ON report_tail:\n${stayOn.report.takeLast(4000)}")
        assertTrue(
            "Stay ON must terminate found:\n${stayOn.report.takeLast(2000)}",
            stayOn.searchTerminateReason == "found" && stayOn.report.contains("PASS"),
        )
        // Host E6-spine measure: 2567.3 km, ETA 2278 min (~38.0 h).
        assertTrue(
            "Stay ON distance ~2567 km (got ${stayOn.distanceKm})",
            stayOn.distanceKm in 2500.0..2650.0,
        )
        assertTrue(
            "Stay ON ETA ~38 h / 2278 min (got ${stayOn.etaMinutes})",
            stayOn.etaMinutes in 2100.0..2500.0,
        )
        assertTrue(
            "Stay ON PSS growth must stay under ~2.5 GiB class (kb)",
            (rssOn - rss0) < 2_500_000,
        )

        val stayOff = plan("stay-off", null)
        val rssOff = Debug.getPss()
        Log.i(
            TAG,
            "Drammen→Berlevåg Stay OFF km=${stayOff.distanceKm} eta_min=${stayOff.etaMinutes} " +
                "terminate=${stayOff.searchTerminateReason} pss_kb=$rssOff",
        )
        Log.i(TAG, "Stay OFF report_tail:\n${stayOff.report.takeLast(4000)}")
        assertTrue(
            "Stay OFF must terminate found:\n${stayOff.report.takeLast(2000)}",
            stayOff.searchTerminateReason == "found" && stayOff.report.contains("PASS"),
        )
        // Host measure: ~2593 km when Stay-in-Country is off.
        assertTrue(
            "Stay OFF distance ~2593 km (got ${stayOff.distanceKm})",
            stayOff.distanceKm in 2520.0..2700.0,
        )
        assertTrue(
            "Stay OFF PSS growth must stay under ~2.5 GiB class (kb)",
            (rssOff - rss0) < 2_500_000,
        )
    }

    companion object {
        private const val TAG = "DrammenBerlevagStay"
        private const val START_LAT = 59.7401977
        private const val START_LON = 10.2015629
        private const val END_LAT = 70.8578156
        private const val END_LON = 29.0860363
    }
}
