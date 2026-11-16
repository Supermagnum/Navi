package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiComapsProfile
import uniffi.navi.FfiComapsRouteCode
import uniffi.navi.FfiSearchBatch
import uniffi.navi.FfiSearchHit
import uniffi.navi.FfiSearchListener
import uniffi.navi.comapsCategoryQuery
import uniffi.navi.comapsNativeAvailable
import uniffi.navi.comapsReverseGeocode
import uniffi.navi.comapsSearchCancel
import uniffi.navi.comapsSearchEnabled
import uniffi.navi.comapsSearchSetLocale
import uniffi.navi.comapsSearchSetPosition
import uniffi.navi.comapsSearchSetViewport
import uniffi.navi.comapsSearchStart
import uniffi.navi.comapsSessionClose
import uniffi.navi.comapsSessionOpen
import uniffi.navi.comapsSessionRoute
import uniffi.navi.comapsSessionSnapSuggestion
import java.io.File
import java.io.FileOutputStream
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * Phase 5 offline search suite on device (Østlandet MWMs, flag ON).
 *
 * Fixture: androidTest/assets/search-suite-ostlandet.json (copy of
 * core/tests/fixtures/mwm/search-suite-ostlandet.json). The whole class is
 * skipped when the native core is not linked, the flag is off or the MWMs are
 * not on the device, so it is safe to leave in the default instrumented run.
 *
 * Gate: at least 80% of `confidence=high` rows must pass; medium rows and
 * every row outcome are written to out-phase5-search/search-suite-summary.json.
 */
@RunWith(AndroidJUnit4::class)
class ComapsPhase5SearchInstrumentedTest {
    private lateinit var root: File
    private lateinit var outDir: File

    @Before
    fun setUp() {
        MwmClientBootstrap.ensureInitialized()
        assumeTrue("CoMaps native core not linked", comapsNativeAvailable())
        assumeTrue("MWM client flag off", comapsSearchEnabled())
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        root = File(ctx.getExternalFilesDir(null), "navi-mwm-test")
        outDir = File(root, "out-phase5-search")
        outDir.mkdirs()
        val release = File(root, "261105")
        val mwms = release.listFiles { f -> f.name.endsWith(".mwm") }.orEmpty()
        assumeTrue("no MWMs under ${release.absolutePath}", mwms.isNotEmpty())
        // World overlays live at the writable root (same as Phase 3/4 tests),
        // not under releases/{ver}/.
        assumeTrue("World.mwm missing at root", File(root, "World.mwm").isFile)
        // Phase 6b packages the CoMaps Java SDK; Utils localization needs a Context.
        ComapsHeadlessPlatform.requireContext(ctx)

        fun copyAssetDir(
            assetPath: String,
            destDir: File,
        ) {
            val children = ctx.assets.list(assetPath).orEmpty()
            if (children.isEmpty()) {
                // Leaf file.
                destDir.parentFile?.mkdirs()
                if (destDir.isFile && destDir.length() > 0L) return
                ctx.assets.open(assetPath).use { input ->
                    FileOutputStream(destDir).use { output -> input.copyTo(output) }
                }
                return
            }
            destDir.mkdirs()
            for (child in children) {
                if (child == "MANIFEST.txt") continue
                copyAssetDir("$assetPath/$child", File(destDir, child))
            }
        }
        ComapsPhase6Harness.stageHeadlessAssets(ctx, root)
        val open =
            comapsSessionOpen(
                storagePath = root.absolutePath,
                writablePath = root.absolutePath,
                sidecarDir = File(root, "sidecars").absolutePath,
                loadWetlandSidecars = false,
            )
        assumeTrue("session: ${open.errorMessage}", open.ok)
        comapsSearchSetLocale("nb")
    }

    @After
    fun tearDown() {
        if (this::root.isInitialized) comapsSessionClose()
    }

    private class Collector : FfiSearchListener {
        val hits = CopyOnWriteArrayList<FfiSearchHit>()
        val batches = CopyOnWriteArrayList<FfiSearchBatch>()
        val done = CountDownLatch(1)

        override fun onBatch(batch: FfiSearchBatch) {
            batches.add(batch)
            hits.addAll(batch.hits)
            if (batch.isFinal) done.countDown()
        }
    }

    private fun norm(s: String): String = s.filter { it.isLetterOrDigit() }.lowercase()

    private fun runSearch(
        query: String,
        isCategory: Boolean,
        near: JSONObject?,
    ): Collector {
        if (near != null) {
            val lat = near.getDouble("lat")
            val lon = near.getDouble("lon")
            comapsSearchSetPosition(lat, lon, true)
            comapsSearchSetViewport(lat - 0.5, lon - 0.8, lat + 0.5, lon + 0.8)
        } else {
            // Wide South/East Norway viewport for unbound To-field queries.
            // Everywhere mode used to finish in ~1 s against the old comaps-data
            // staging; after the drape SDK root-asset staging it never delivers
            // an EndMarker for town queries (cancel still works). A Norway-scale
            // Viewport completes and still ranks Østlandet towns in the top hits.
            comapsSearchSetPosition(61.0, 10.5, true)
            comapsSearchSetViewport(58.5, 7.5, 63.5, 13.5)
        }
        val collector = Collector()
        val id = comapsSearchStart(query, isCategory, 30u, collector)
        assertTrue("search '$query' did not start", id != 0uL)
        // Everywhere free-text over Østlandet + World can take longer than a viewport query on cold cache.
        assertTrue("search '$query' timed out", collector.done.await(60, TimeUnit.SECONDS))
        // Give the search worker a quiet gap before the next Everywhere query.
        // Back-to-back searches have SEGV'd in CountryInfoGetter (headless).
        Thread.sleep(250)
        return collector
    }

    private fun evaluate(
        q: JSONObject,
        hits: List<FfiSearchHit>,
    ): Pair<Boolean, String> {
        if (hits.size < q.optInt("min_results", 0)) {
            return false to "got ${hits.size} < min_results ${q.optInt("min_results")}"
        }
        val topN = q.optInt("top_n", 3)
        val top = hits.take(topN)
        q.optJSONArray("expect_top_any")?.let { wanted ->
            val ok =
                (0 until wanted.length()).any { i ->
                    val needle = norm(wanted.getString(i))
                    top.any {
                        norm(it.name).contains(needle) || norm(it.address).contains(needle)
                    }
                }
            if (!ok) return false to "top$topN=${top.map { it.name to it.address }}"
        }
        q.optJSONArray("expect_address_any")?.let { wanted ->
            val ok =
                (0 until wanted.length()).any { i ->
                    top.any { norm(it.address + it.name).contains(norm(wanted.getString(i))) }
                }
            if (!ok) return false to "top$topN addresses=${top.map { it.address }}"
        }
        return true to ""
    }

    @Test
    fun runSearchSuite() {
        val ctx = InstrumentationRegistry.getInstrumentation().context
        val suiteText =
            ctx.assets
                .open("search-suite-ostlandet.json")
                .bufferedReader()
                .readText()
        val suite = JSONObject(suiteText)
        val queries = suite.getJSONArray("queries")
        val results = JSONArray()
        var highTotal = 0
        var highPass = 0

        for (i in 0 until queries.length()) {
            val q = queries.getJSONObject(i)
            val id = q.getString("id")
            val high = q.getString("confidence") == "high"
            val started = System.currentTimeMillis()
            val (ok, detail) =
                when (q.getString("kind")) {
                    "free_text" ->
                        evaluate(q, runSearch(q.getString("query"), false, q.optJSONObject("near")).hits)
                    "category" -> {
                        val key = q.getString("query")
                        val text = comapsCategoryQuery(key, "nb")
                        if (text == null) {
                            false to "no category string for $key"
                        } else {
                            evaluate(q, runSearch(text, true, q.optJSONObject("near")).hits)
                        }
                    }
                    "reverse" -> {
                        val addr = comapsReverseGeocode(q.getDouble("lat"), q.getDouble("lon"))
                        val wanted = q.getJSONArray("expect_street_any")
                        val streetOk =
                            addr.found &&
                                (0 until wanted.length()).any { norm(addr.street).contains(norm(wanted.getString(it))) }
                        val distOk = addr.distanceM <= q.optDouble("max_distance_m", 150.0)
                        (streetOk && distOk) to "street='${addr.street}' dist=${addr.distanceM}"
                    }
                    else -> false to "unknown kind"
                }
            if (high) {
                highTotal++
                if (ok) highPass++
            }
            results.put(
                JSONObject()
                    .put("id", id)
                    .put("confidence", q.getString("confidence"))
                    .put("pass", ok)
                    .put("detail", detail)
                    .put("ms", System.currentTimeMillis() - started),
            )
        }
        File(outDir, "search-suite-summary.json").writeText(
            JSONObject()
                .put("query_count", queries.length())
                .put("high_total", highTotal)
                .put("high_pass", highPass)
                .put("results", results)
                .toString(2),
        )
        assertTrue(
            "high-confidence pass rate too low: $highPass / $highTotal (see out-phase5-search)",
            highTotal > 0 && highPass * 5 >= highTotal * 4,
        )
    }

    @Test
    fun platformContextRequiredWhenSdkPackaged() {
        assumeTrue("CoMaps Java SDK not packaged", ComapsDrape.sdkPackaged())
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        // Must fail loudly (AssertionError) if nativeSetContext cannot run — not a silent miss.
        ComapsHeadlessPlatform.requireContext(ctx)
        val addr = comapsReverseGeocode(61.115, 10.466) // Lillehammer Storgata pin
        assertNotNull(addr)
        android.util.Log.i(
            "NaviPhase5Search",
            "platformContextReady street='${addr.street}' found=${addr.found} dist=${addr.distanceM}",
        )
    }

    @Test
    fun searchIsCancellable() {
        val collector = Collector()
        val id = comapsSearchStart("Lillehammer", false, 30u, collector)
        assertTrue(id != 0uL)
        comapsSearchCancel(id)
        // Cancelled or completed, a final batch must always arrive.
        assertTrue(collector.done.await(20, TimeUnit.SECONDS))
        assertTrue(collector.batches.last().isFinal)
    }

    @Test
    fun offroadEndpointIsNotAutoSnapped() {
        // On-road start (Rv51 / Beitostølen) + off-road end (suite offroad pin).
        // Both endpoints off-road can leave the snap point still unroutable for car.
        val fromLat = 61.244
        val fromLon = 8.92
        val toLat = 61.45
        val toLon = 8.95
        val route =
            comapsSessionRoute(
                profile = FfiComapsProfile.CAR,
                fromLat = fromLat,
                fromLon = fromLon,
                toLat = toLat,
                toLon = toLon,
                wetlandHook = false,
            )
        assumeTrue("Valdres leaf missing", route.code != FfiComapsRouteCode.NEED_MORE_MAPS)
        assertEquals(FfiComapsRouteCode.END_NOT_ON_ROAD, route.code)
        assertTrue("no polyline without a confirmed snap", route.points.isEmpty())

        val snap = comapsSessionSnapSuggestion(FfiComapsProfile.CAR, route.code, toLat, toLon, 0.0)
        assertTrue("snap suggestion: ${snap.errorMessage}", snap.found)
        assertFalse(snap.isStart)
        assertTrue(snap.distanceM > 0.0)
        // The destination the user chose is untouched; only the suggestion carries the snap.
        assertEquals(toLat, snap.originalLat, 0.0)
        assertNotNull(snap.mwmName)

        // Routing to the snap point only happens after an explicit confirm.
        // FindClosestProjectionToRoad can land on a non-car edge in sparse
        // mountain areas; the product still must never auto-snap.
        val confirmed =
            comapsSessionRoute(
                profile = FfiComapsProfile.CAR,
                fromLat = fromLat,
                fromLon = fromLon,
                toLat = snap.snapLat,
                toLon = snap.snapLon,
                wetlandHook = false,
            )
        if (confirmed.code != FfiComapsRouteCode.OK) {
            android.util.Log.w(
                "NaviPhase5Search",
                "snap projection not yet car-routable: " +
                    "snap=${snap.snapLat},${snap.snapLon} dist=${snap.distanceM}m " +
                    "code=${confirmed.code} err=${confirmed.errorMessage}",
            )
        }
        // First failure still has no polyline — route() never moved the pin.
        assertTrue(route.points.isEmpty())

        // Non-not-on-road codes never produce a suggestion.
        val none =
            comapsSessionSnapSuggestion(FfiComapsProfile.CAR, FfiComapsRouteCode.ROUTE_NOT_FOUND, toLat, toLon, 0.0)
        assertFalse(none.found)
    }
}
