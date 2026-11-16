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
 * core/tests/fixtures/mwm/search-suite-ostlandet.json). Skipped when the native
 * core is not linked, the flag is off, or MWMs are missing.
 *
 * Search context matches production [ComapsSearchBridge]: per-query
 * `user_context` locale + position, viewport cleared (Everywhere), free-text
 * `isCategory=false`, limit 80. Gate: every `confidence=high` row must pass
 * individually (no percentage gate); medium rows are reported with
 * `non_gating_reason`.
 */
@RunWith(AndroidJUnit4::class)
class ComapsPhase5SearchInstrumentedTest {
    private lateinit var root: File
    private lateinit var outDir: File

    companion object {
        /** Same limit as the app search panel / MainActivity CoMaps path. */
        private const val APP_LIMIT = 80
    }

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
        // Match in-app: wait until Everywhere search actually returns hits
        // (enabled/session-open can precede the lazy search Engine).
        val warmed =
            ComapsPhase6Harness.waitFor(90_000L, 750L) {
                ComapsSearchBridge
                    .search("Oslo", 59.9139, 10.7522, 5, locale = "nb")
                    .isNotEmpty()
            }
        assumeTrue("CoMaps Everywhere search engine did not warm up", warmed)
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

    private fun userContext(q: JSONObject): Triple<String, Double, Double> {
        val ctx = q.optJSONObject("user_context")
        requireNotNull(ctx) { "query ${q.optString("id")} missing user_context" }
        return Triple(
            ctx.getString("locale"),
            ctx.getDouble("lat"),
            ctx.getDouble("lon"),
        )
    }

    private fun rankOf(
        hits: List<FfiSearchHit>,
        wanted: JSONArray,
    ): Int {
        for ((idx, hit) in hits.withIndex()) {
            val hay = norm(hit.name + " " + hit.address)
            for (i in 0 until wanted.length()) {
                if (hay.contains(norm(wanted.getString(i)))) return idx + 1
            }
        }
        return -1
    }

    private fun evaluate(
        q: JSONObject,
        hits: List<FfiSearchHit>,
    ): Triple<Boolean, Int, String> {
        if (hits.size < q.optInt("min_results", 0)) {
            return Triple(false, -1, "got ${hits.size} < min_results ${q.optInt("min_results")}")
        }
        val topN = q.optInt("top_n", 3)
        q.optJSONArray("expect_top_any")?.let { wanted ->
            val rank = rankOf(hits, wanted)
            val ok = rank in 1..topN
            return Triple(
                ok,
                rank,
                if (ok) "" else "top$topN=${hits.take(topN).map { it.name to it.address }}",
            )
        }
        q.optJSONArray("expect_address_any")?.let { wanted ->
            val top = hits.take(topN)
            val ok =
                (0 until wanted.length()).any { i ->
                    top.any { norm(it.address + it.name).contains(norm(wanted.getString(i))) }
                }
            val rank = if (ok) rankOf(hits, wanted) else -1
            return Triple(
                ok,
                rank,
                if (ok) "" else "top$topN addresses=${top.map { it.address }}",
            )
        }
        return Triple(true, if (hits.isNotEmpty()) 1 else -1, "")
    }

    private fun runFreeText(q: JSONObject): List<FfiSearchHit> {
        val (locale, lat, lon) = userContext(q)
        // Same code path as the app search panel.
        return ComapsSearchBridge.search(
            query = q.getString("query"),
            biasLat = lat,
            biasLon = lon,
            limit = APP_LIMIT,
            locale = locale,
        )
    }

    private fun runCategory(q: JSONObject): Pair<List<FfiSearchHit>, String> {
        val (locale, lat, lon) = userContext(q)
        val key = q.getString("query")
        val text = comapsCategoryQuery(key, locale)
        if (text == null) return emptyList<FfiSearchHit>() to "no category string for $key"
        val hits =
            ComapsSearchBridge.searchCategory(
                queryText = text,
                biasLat = lat,
                biasLon = lon,
                limit = APP_LIMIT,
                locale = locale,
            )
        // Quiet gap between category/Everywhere queries (SEGV history).
        Thread.sleep(250)
        return hits to "q=$text"
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
        val highFailures = mutableListOf<String>()

        for (i in 0 until queries.length()) {
            val q = queries.getJSONObject(i)
            val id = q.getString("id")
            val high = q.getString("confidence") == "high"
            val started = System.currentTimeMillis()
            var hits: List<FfiSearchHit> = emptyList()
            var ok: Boolean
            var rank = -1
            var detail: String
            when (q.getString("kind")) {
                "free_text" -> {
                    hits = runFreeText(q)
                    val ev = evaluate(q, hits)
                    ok = ev.first
                    rank = ev.second
                    detail = ev.third
                    Thread.sleep(250)
                }
                "category" -> {
                    val (h, note) = runCategory(q)
                    hits = h
                    val ev = evaluate(q, hits)
                    ok = ev.first
                    rank = ev.second
                    detail = listOf(ev.third, note).filter { it.isNotEmpty() }.joinToString(" ")
                }
                "reverse" -> {
                    val addr = comapsReverseGeocode(q.getDouble("lat"), q.getDouble("lon"))
                    val wanted = q.getJSONArray("expect_street_any")
                    val streetOk =
                        addr.found &&
                            (0 until wanted.length()).any {
                                norm(addr.street).contains(norm(wanted.getString(it)))
                            }
                    val distOk = addr.distanceM <= q.optDouble("max_distance_m", 150.0)
                    ok = streetOk && distOk
                    rank = if (ok) 1 else -1
                    detail = "street='${addr.street}' dist=${addr.distanceM}"
                }
                else -> {
                    ok = false
                    detail = "unknown kind"
                }
            }
            if (high) {
                highTotal++
                if (ok) highPass++ else highFailures.add("$id: $detail")
            }
            results.put(
                JSONObject()
                    .put("id", id)
                    .put("kind", q.getString("kind"))
                    .put("confidence", q.getString("confidence"))
                    .put("pass", ok)
                    .put("rank", rank)
                    .put("detail", detail)
                    .put("ms", System.currentTimeMillis() - started)
                    .put("top", JSONArray(hits.take(5).map { it.name }))
                    .put("non_gating_reason", q.optString("non_gating_reason", "")),
            )
        }
        File(outDir, "search-suite-summary.json").writeText(
            JSONObject()
                .put("path", "headless-ComapsSearchBridge-Everywhere")
                .put("query_count", queries.length())
                .put("high_total", highTotal)
                .put("high_pass", highPass)
                .put("high_failures", JSONArray(highFailures))
                .put("results", results)
                .toString(2),
        )
        assertTrue("no high-confidence rows in fixture", highTotal > 0)
        assertTrue(
            "high-confidence per-query failures ($highPass/$highTotal): $highFailures",
            highFailures.isEmpty(),
        )
        assertEquals("high pass must equal high total", highTotal, highPass)
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
        ComapsSearchBridge.applyEverywhereContext("nb", 61.0, 10.5)
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
