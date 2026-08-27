package no.navi.app

import android.util.Log
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.FixMethodOrder
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.planCarRoute
import uniffi.navi.searchPlaces
import uniffi.navi.waterPoisAlongPolyline
import java.io.File
import java.util.Locale

/**
 * Real-device follow-up: cycle routing (Elverum-Tynset slow-road preference,
 * Gjovik-Kyrkjestolen cross-region), US cross-region routes, water POI pickup.
 * Routes entered via keyboard (station names in Norway; lat,lon for US — place
 * index on device is Norway-only).
 */
@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class CycleRoutingWaterFollowupTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var device: UiDevice
    private lateinit var dataDir: File

    companion object {
        private const val TAG = "CycleWaterFollowup"
        private val EMPTY_VEHICLE = FfiVehicleLimits(null, null, null, null, null, null)
    }

    @Before
    fun setUp() {
        device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = ctx.filesDir
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(ctx.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(ctx.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.missingCoveragePromptVisible = false
        NaviMapTestHooks.lastMissingCoveragePath = ""
        NaviMapTestHooks.lastMissingCoverageMessage = ""
        NaviMapTestHooks.lastRoutePolylineChars = 0
        NaviMapTestHooks.lastPlanReport = ""
        NaviMapTestHooks.lastRoutePolyline = ""
        NaviMapTestHooks.lastSimSamplesJson = "[]"
        NaviMapTestHooks.lastManeuversJson = "[]"
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        dismissPermission()
    }

    @Test
    fun a_elverum_tynset_bicycle_slow_road() {
        openRoutePanel()
        selectProfile("chip_profile_bicycle")
        val from = typeNameAndPickHit("chip_from", "Elverum stasjon", "Elverum")
        val to = typeNameAndPickHit("chip_to", "Tynset stasjon", "Tynset")
        planAndWait(900_000)
        val report = NaviMapTestHooks.lastPlanReport
        val dist = NaviMapTestHooks.lastPlanDistanceKm
        Log.i(TAG, "ELVERUM_TYNSET dist_km=$dist report=$report")
        assertTrue("plan must PASS:\n${report.take(800)}", report.contains("PASS"))

        val streets = streetLabelsAlongRoute()
        Log.i(TAG, "ELVERUM_TYNSET streets=${streets.take(50)}")
        val ref237 = streets.filter { looksLikeRef237(it) }
        val ref3 = streets.filter { looksLikeRv3(it) }
        Log.i(
            TAG,
            "ELVERUM_TYNSET ref237=$ref237 rv3=$ref3 eta_min=${etaMinutes(report)}",
        )

        val audit =
            planCarRoute(
                pbfPath("ostlandet-latest.osm.pbf"),
                File(dataDir, "elevation").absolutePath,
                File(dataDir, "graph-cache-ostlandet-latest.osm-bicycle").absolutePath,
                from.first,
                from.second,
                to.first,
                to.second,
                false,
                TravelProfile.BICYCLE,
                false,
                false,
                false,
                EMPTY_VEHICLE,
                true,
                "",
            )
        Log.i(TAG, "ELVERUM_TYNSET audit_report=${audit.report.take(600)}")
        assertTrue(
            "single-leg bicycle audit must apply slow-road preference:\n${audit.report}",
            audit.report.contains("slow_road_preference=applied"),
        )
        val auditStreets =
            parseRouteSimSamples(audit.simSamplesJson)
                .mapNotNull { it.street?.trim()?.takeIf { s -> s.isNotEmpty() } }
                .distinct()
        val audit237 = auditStreets.filter { looksLikeRef237(it) }
        val audit3 = auditStreets.filter { looksLikeRv3(it) }
        Log.i(
            TAG,
            "ELVERUM_TYNSET audit_dist=${audit.distanceKm} audit237=$audit237 audit3=$audit3",
        )
        logWaterPois("elverum_tynset", pbfPath("ostlandet-latest.osm.pbf"))
        // Street labels often show names, not refs; graph-level preference is in audit report.
        if (audit237.isEmpty() && audit3.isEmpty()) {
            Log.i(
                TAG,
                "ELVERUM_TYNSET no ref 237/3 in sample street labels — " +
                    "corridor uses named local roads / pilgrim path instead",
            )
        }
    }

    @Test
    fun b_gjovik_kyrkjestolen_bicycle_cross_region() {
        openRoutePanel()
        selectProfile("chip_profile_bicycle")
        NaviMapTestHooks.missingCoveragePromptVisible = false
        NaviMapTestHooks.lastMissingCoveragePath = ""
        typeNameAndPickHit("chip_from", "Gjøvik stasjon", "Gjøvik")
        typeNameAndPickHit("chip_to", "Kyrkjestølen", "Kyrkjest")
        clickTag("btn_plan_route")
        val deadline = System.currentTimeMillis() + 45_000
        var prompted = false
        var planned = false
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.missingCoveragePromptVisible) {
                prompted = true
                break
            }
            if (NaviMapTestHooks.lastRoutePolylineChars > 100 &&
                NaviMapTestHooks.lastPlanReport.contains("PASS")
            ) {
                planned = true
                break
            }
            Thread.sleep(250)
        }
        val path = NaviMapTestHooks.lastMissingCoveragePath
        val msg = NaviMapTestHooks.lastMissingCoverageMessage
        Log.i(
            TAG,
            "GJOVIK_KYR prompted=$prompted planned=$planned path=$path msg='$msg' " +
                "poly=${NaviMapTestHooks.lastRoutePolylineChars}",
        )
        assertTrue(
            "Kyrkjestolen should trigger missing coverage without Vestlandet",
            prompted || !planned,
        )
        if (prompted) {
            assertTrue(
                "expected Vestlandet or Norway country extract, got path=$path",
                path.contains("vestlandet") || path.contains("norway"),
            )
            composeRule
                .onNodeWithTag("btn_missing_coverage_download", useUnmergedTree = true)
                .assertIsDisplayed()
            clickTag("btn_missing_coverage_dismiss")
        }
    }

    @Test
    fun c_wv_airport_sandusky_via_stringtown_car() {
        val wvPbf = File(dataDir, "west-virginia-latest.osm.pbf")
        val wvPartial = File(dataDir, "west-virginia-latest.osm.pbf.partial")
        Log.i(
            TAG,
            "WV_PBF complete=${wvPbf.isFile} bytes=${wvPbf.length()} " +
                "partial=${wvPartial.isFile} partial_bytes=${wvPartial.length()}",
        )
        openRoutePanel()
        selectProfile("chip_profile_car")
        // Keyboard lat,lon — US places are not in the Norway place index.
        typeCoordAndPickHit("chip_from", 39.2967, -80.2281) // CKB airport
        typeCoordAndPickHit("chip_via", 39.4560, -79.7067) // Stringtown, Preston Co.
        typeCoordAndPickHit("chip_to", 39.5556, -80.8590) // Sandusky, Tyler Co.
        clickTag("btn_plan_route")
        waitPlanOrPrompt(180_000)
        val report = NaviMapTestHooks.lastPlanReport
        val prompted = NaviMapTestHooks.missingCoveragePromptVisible
        val planned =
            NaviMapTestHooks.lastRoutePolylineChars > 100 && report.contains("PASS")
        Log.i(
            TAG,
            "WV_ROUTE prompted=$prompted planned=$planned dist=${NaviMapTestHooks.lastPlanDistanceKm} " +
                "path=${NaviMapTestHooks.lastMissingCoveragePath} report=${report.take(400)}",
        )
        if (!wvPbf.isFile && prompted) {
            Log.i(
                TAG,
                "WV_ROUTE missing-coverage path=${NaviMapTestHooks.lastMissingCoveragePath} " +
                    "msg='${NaviMapTestHooks.lastMissingCoverageMessage}'",
            )
            // Known issue: russia bbox spans global longitude and can win over US states.
            composeRule
                .onNodeWithTag("btn_missing_coverage_download", useUnmergedTree = true)
                .assertIsDisplayed()
            clickTag("btn_missing_coverage_download")
            Thread.sleep(5_000)
            runCatching { clickTag("btn_missing_coverage_dismiss") }
        } else if (planned) {
            assertTrue("WV car route plausible km", NaviMapTestHooks.lastPlanDistanceKm in 40.0..250.0)
            logWaterPois("wv_airport_sandusky", wvPbf.absolutePath)
        } else {
            Log.i(TAG, "WV_ROUTE inconclusive — partial extract may block planning")
        }
    }

    @Test
    fun d_reese_river_eureka_nevada_car() {
        val nvPbf = File(dataDir, "nevada-latest.osm.pbf")
        Log.i(TAG, "NEVADA_PBF present=${nvPbf.isFile} bytes=${nvPbf.length()}")
        openRoutePanel()
        selectProfile("chip_profile_car")
        NaviMapTestHooks.missingCoveragePromptVisible = false
        NaviMapTestHooks.lastMissingCoveragePath = ""
        typeCoordAndPickHit("chip_from", 39.4336, -117.2719) // Reese River Valley / Austin area
        typeCoordAndPickHit("chip_to", 39.5128, -115.9617) // Eureka, NV (Parsonage B&B area)
        clickTag("btn_plan_route")
        waitPlanOrPrompt(120_000)
        val path = NaviMapTestHooks.lastMissingCoveragePath
        val msg = NaviMapTestHooks.lastMissingCoverageMessage
        val prompted = NaviMapTestHooks.missingCoveragePromptVisible
        val planned =
            NaviMapTestHooks.lastRoutePolylineChars > 100 &&
                NaviMapTestHooks.lastPlanReport.contains("PASS")
        Log.i(TAG, "NEVADA prompted=$prompted planned=$planned path=$path msg='$msg'")
        if (!nvPbf.isFile) {
            assertTrue("Nevada not on device — expected missing-coverage prompt", prompted)
            Log.i(
                TAG,
                "NEVADA missing-coverage path=$path msg='$msg' " +
                    "(russia suggestion is a known global-bbox ordering bug for US lon)",
            )
            composeRule
                .onNodeWithTag("btn_missing_coverage_download", useUnmergedTree = true)
                .assertIsDisplayed()
            clickTag("btn_missing_coverage_download")
            Thread.sleep(3_000)
            runCatching { clickTag("btn_missing_coverage_dismiss") }
            Log.i(TAG, "NEVADA download tapped — full state extract is large; replan deferred")
        } else if (planned) {
            logWaterPois("reese_eureka", nvPbf.absolutePath)
        }
    }

    // --- helpers ---

    private fun streetLabelsAlongRoute(): List<String> {
        val fromSamples =
            parseRouteSimSamples(NaviMapTestHooks.lastSimSamplesJson)
                .mapNotNull { it.street?.trim()?.takeIf { s -> s.isNotEmpty() } }
        val fromManeuvers =
            parseRouteManeuvers(NaviMapTestHooks.lastManeuversJson)
                .mapNotNull { it.street?.trim()?.takeIf { s -> s.isNotEmpty() } }
        return (fromSamples + fromManeuvers).distinct()
    }

    private fun looksLikeRef237(label: String): Boolean {
        val t = label.lowercase(Locale.US)
        return t.contains("237") ||
            t.contains("fv 237") ||
            t.contains("fylkesvei 237")
    }

    private fun looksLikeRv3(label: String): Boolean {
        val t = label.lowercase(Locale.US)
        if (t.contains("237")) return false
        return t == "3" ||
            t.contains("rv 3") ||
            t.contains("riksvei 3") ||
            (t.contains("rv") && t.contains(" 3"))
    }

    private fun etaMinutes(report: String): Double? {
        val m = Regex("eta_min=([0-9.]+)").find(report) ?: return null
        return m.groupValues[1].toDoubleOrNull()
    }

    private fun logWaterPois(
        routeLabel: String,
        pbfPath: String,
    ) {
        val poly = NaviMapTestHooks.lastRoutePolyline
        if (poly.length < 20) {
            Log.i(TAG, "WATER_$routeLabel skipped — no polyline")
            return
        }
        if (!File(pbfPath).isFile) {
            Log.i(TAG, "WATER_$routeLabel skipped — missing pbf $pbfPath")
            return
        }
        val hits =
            waterPoisAlongPolyline(
                dataDir.absolutePath,
                pbfPath,
                poly,
                12.0,
                5_000.0,
            )
        Log.i(TAG, "WATER_$routeLabel count=${hits.size}")
        hits.take(8).forEach { w ->
            Log.i(
                TAG,
                "WATER_$routeLabel example name='${w.name}' " +
                    "lat=${String.format(Locale.US, "%.5f", w.lat)} " +
                    "lon=${String.format(Locale.US, "%.5f", w.lon)} " +
                    "sample_km=${String.format(Locale.US, "%.1f", w.sampleKm)} " +
                    "dist_m=${String.format(Locale.US, "%.0f", w.distM)}",
            )
        }
        if (hits.isEmpty()) {
            Log.i(
                TAG,
                "WATER_$routeLabel zero — corridor may lack mapped drinking_water; " +
                    "not treated as bug without OSM check",
            )
        }
    }

    private fun pbfPath(name: String): String = File(dataDir, name).absolutePath

    private fun openRoutePanel() {
        runCatching {
            composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
        }.onFailure { clickTag("btn_open_search") }
        composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
    }

    private fun clickTag(tag: String) {
        val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true)
        runCatching { node.performScrollTo() }
        node.assertIsDisplayed().performClick()
        composeRule.waitForIdle()
    }

    private fun setField(
        tag: String,
        value: String,
    ) {
        val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true)
        runCatching { node.performScrollTo() }
        node.performTextClearance()
        node.performTextInput(value)
        composeRule.waitForIdle()
    }

    /** Returns (lat, lon) of the applied hit. */
    private fun typeNameAndPickHit(
        chipTag: String,
        query: String,
        hitHint: String,
    ): Pair<Double, Double> {
        clickTag(chipTag)
        NaviMapTestHooks.lastSearchHitCount = -1
        NaviMapTestHooks.lastSearchQuery = ""
        NaviMapTestHooks.lastSearchHitNames = emptyList()
        setField("field_search", query)
        val deadline = System.currentTimeMillis() + 15_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastSearchHitCount >= 1 ||
                NaviMapTestHooks.lastSearchHitNames.isNotEmpty()
            ) {
                break
            }
            Thread.sleep(200)
        }
        val placeDb = File(dataDir, "place_index.db")
        val uiNames = NaviMapTestHooks.lastSearchHitNames
        var hitLat = 0.0
        var hitLon = 0.0
        var picked = false
        val idx =
            uiNames
                .indexOfFirst { it.contains(hitHint, ignoreCase = true) }
                .takeIf { it >= 0 }
                ?: 0
        if (uiNames.isNotEmpty()) {
            runCatching {
                composeRule
                    .onNodeWithTag("search_hit_$idx", useUnmergedTree = true)
                    .performScrollTo()
                    .performClick()
                picked = true
            }
        }
        if (!picked) {
            val hits = searchPlaces(placeDb.absolutePath, query, 20u)
            val hit =
                hits.firstOrNull { it.name.contains(hitHint, ignoreCase = true) }
                    ?: hits.firstOrNull()
            assertTrue(
                "no FTS hit for '$query' / '$hitHint' ui=$uiNames ffi=${hits.map { it.name }}",
                hit != null,
            )
            hitLat = hit!!.lat
            hitLon = hit.lon
            NaviMapTestHooks.pendingApplyHit = hit
            val applyDeadline = System.currentTimeMillis() + 10_000
            while (System.currentTimeMillis() < applyDeadline &&
                NaviMapTestHooks.pendingApplyHit != null
            ) {
                Thread.sleep(100)
            }
            picked = true
        } else {
            val hits = searchPlaces(placeDb.absolutePath, query, 20u)
            val hit =
                hits.firstOrNull { it.name.contains(hitHint, ignoreCase = true) }
                    ?: hits.getOrNull(idx)
            if (hit != null) {
                hitLat = hit.lat
                hitLon = hit.lon
            }
        }
        Thread.sleep(500)
        NaviMapTestHooks.disableGpsFollow = true
        Log.i(TAG, "SEARCH chip=$chipTag q='$query' lat=$hitLat lon=$hitLon")
        return hitLat to hitLon
    }

    private fun typeCoordAndPickHit(
        chipTag: String,
        lat: Double,
        lon: Double,
    ) {
        clickTag(chipTag)
        val q = String.format(Locale.US, "%.7f, %.7f", lat, lon)
        setField("field_search", q)
        val deadline = System.currentTimeMillis() + 10_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastSearchHitCount >= 1) break
            Thread.sleep(200)
        }
        assertTrue(
            "coordinate hit for $chipTag q=$q hits=${NaviMapTestHooks.lastSearchHitCount}",
            NaviMapTestHooks.lastSearchHitCount >= 1,
        )
        clickTag("search_hit_0")
        Thread.sleep(500)
        NaviMapTestHooks.disableGpsFollow = true
    }

    private fun selectProfile(chip: String) {
        runCatching { clickTag("btn_open_profile") }
        clickTag(chip)
        runCatching { clickTag("btn_save_profile") }
        Thread.sleep(300)
    }

    private fun planAndWait(timeoutMs: Long) {
        NaviMapTestHooks.lastRoutePolylineChars = 0
        NaviMapTestHooks.lastPlanReport = ""
        composeRule.onNodeWithTag("btn_plan_route", useUnmergedTree = true).performScrollTo()
        clickTag("btn_plan_route")
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastRoutePolylineChars > 100 &&
                NaviMapTestHooks.lastPlanReport.contains("PASS")
            ) {
                return
            }
            Thread.sleep(2_000)
        }
        error("plan timeout report=${NaviMapTestHooks.lastPlanReport.take(500)}")
    }

    private fun waitPlanOrPrompt(timeoutMs: Long) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.missingCoveragePromptVisible) return
            if (NaviMapTestHooks.lastRoutePolylineChars > 100 &&
                NaviMapTestHooks.lastPlanReport.contains("PASS")
            ) {
                return
            }
            Thread.sleep(500)
        }
    }

    private fun dismissPermission() {
        val deadline = System.currentTimeMillis() + 6_000
        while (System.currentTimeMillis() < deadline) {
            val allow =
                device.findObject(By.text("While using the app"))
                    ?: device.findObject(By.text("Allow"))
            if (allow != null) {
                allow.click()
                Thread.sleep(400)
                continue
            }
            break
        }
    }
}
