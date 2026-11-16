package no.navi.app

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiSearchHit
import uniffi.navi.comapsCategoryQuery
import uniffi.navi.comapsNativeAvailable
import uniffi.navi.comapsReverseGeocode
import uniffi.navi.comapsSearchEnabled
import java.io.File

/**
 * Phase 5 search suite through the **production** path: starts [MainActivity] so
 * drape / Platform Context init runs, then uses [ComapsSearchBridge] (same bridge
 * the search panel ViewModel path calls).
 *
 * Per-query `user_context` (locale + position) matches the headless harness;
 * viewport is cleared (Everywhere); limit 80. Gate: every high-confidence row
 * must pass individually (no percentage gate).
 *
 * Output: `{externalFiles}/phase5-inapp/search-suite-summary.json`.
 */
@RunWith(AndroidJUnit4::class)
class ComapsPhase5InAppSearchInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var outDir: File

    companion object {
        private const val APP_LIMIT = 80
    }

    @Before
    fun setUp() {
        MwmClientBootstrap.ensureInitialized()
        assumeTrue("CoMaps native core not linked", comapsNativeAvailable())
        assumeTrue("MWM client flag off", comapsSearchEnabled())
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        outDir = File(ctx.getExternalFilesDir(null), "phase5-inapp")
        outDir.mkdirs()
        // Production path only: MainActivity (compose rule) already opened the
        // CoMaps session. A second comapsSessionOpen from the test aborts inside
        // ~HttpMapFilesDownloader (CalledOnOriginalThread). Prefer installed
        // production leaves; fall back to navi-mwm-test only if production is empty
        // (stage assets, then let ComapsSessionBootstrap reopen once).
        val dataMwm = File(NaviAppData.resolve(ctx), "mwm")
        val testRoot = File(ctx.getExternalFilesDir(null), "navi-mwm-test")
        val productionReady =
            File(dataMwm, "World.mwm").isFile ||
                File(dataMwm, "releases").listFiles()?.any { rel ->
                    File(rel, "leaves").listFiles()?.any { it.name.endsWith(".mwm") } == true
                } == true
        if (!productionReady) {
            assumeTrue("no test MWMs under ${testRoot.absolutePath}", File(testRoot, "World.mwm").isFile)
            ComapsPhase6Harness.stageHeadlessAssets(ctx, testRoot)
            // Single reopen through the same bootstrap production uses.
            ComapsSessionBootstrap.ensureOpen(ctx, force = true)
        }
        // Wait until the activity-owned session can serve search.
        val ready =
            ComapsPhase6Harness.waitFor(120_000L, 500L) {
                ComapsSearchBridge.enabled()
            }
        assumeTrue("ComapsSearchBridge not enabled after MainActivity start", ready)
        // enabled() can race ahead of the lazy search Engine; first Everywhere
        // starts then return id=0 / empty. Warm until a known town yields hits.
        val warmed =
            ComapsPhase6Harness.waitFor(90_000L, 750L) {
                ComapsSearchBridge
                    .search("Oslo", 59.9139, 10.7522, 5, locale = "nb")
                    .isNotEmpty()
            }
        assumeTrue("CoMaps Everywhere search engine did not warm up", warmed)
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

    private fun evaluateFreeText(
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
                if (ok) "" else "top=${hits.take(topN).map { it.name to it.address }}",
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
                if (ok) "" else "addresses=${top.map { it.address }}",
            )
        }
        return Triple(hits.size >= q.optInt("min_results", 0), if (hits.isNotEmpty()) 1 else -1, "n=${hits.size}")
    }

    @Test
    fun runInAppSearchSuite() {
        val ctx = InstrumentationRegistry.getInstrumentation().context
        val suite =
            JSONObject(
                ctx.assets
                    .open("search-suite-ostlandet.json")
                    .bufferedReader()
                    .readText(),
            )
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
            var hit = false
            var rank = -1
            var detail = ""
            var topNames = emptyList<String>()
            when (q.getString("kind")) {
                "free_text" -> {
                    val (locale, lat, lon) = userContext(q)
                    val hits =
                        ComapsSearchBridge.search(
                            q.getString("query"),
                            lat,
                            lon,
                            APP_LIMIT,
                            locale = locale,
                        )
                    topNames = hits.take(5).map { it.name }
                    val ev = evaluateFreeText(q, hits)
                    hit = ev.first
                    rank = ev.second
                    detail = ev.third
                    Thread.sleep(250)
                }
                "category" -> {
                    val (locale, lat, lon) = userContext(q)
                    val key = q.getString("query")
                    val text = comapsCategoryQuery(key, locale)
                    if (text == null) {
                        hit = false
                        detail = "no category string for $key"
                    } else {
                        val hits =
                            ComapsSearchBridge.searchCategory(
                                queryText = text,
                                biasLat = lat,
                                biasLon = lon,
                                limit = APP_LIMIT,
                                locale = locale,
                            )
                        topNames = hits.take(5).map { it.name }
                        val ev = evaluateFreeText(q, hits)
                        hit = ev.first
                        rank = ev.second
                        detail = listOf(ev.third, "q=$text").filter { it.isNotEmpty() }.joinToString(" ")
                        Thread.sleep(250)
                    }
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
                    hit = streetOk && distOk
                    detail = "street='${addr.street}' dist=${addr.distanceM}"
                    if (hit) rank = 1
                }
                else -> {
                    hit = false
                    detail = "unknown kind"
                }
            }
            if (high) {
                highTotal++
                if (hit) highPass++ else highFailures.add("$id: $detail")
            }
            results.put(
                JSONObject()
                    .put("id", id)
                    .put("kind", q.getString("kind"))
                    .put("confidence", q.getString("confidence"))
                    .put("hit", hit)
                    .put("pass", hit)
                    .put("rank", rank)
                    .put("ms", System.currentTimeMillis() - started)
                    .put("detail", detail)
                    .put("top", JSONArray(topNames))
                    .put("non_gating_reason", q.optString("non_gating_reason", "")),
            )
        }

        val summary =
            JSONObject()
                .put("path", "in-app-ComapsSearchBridge+MainActivity")
                .put("query_count", queries.length())
                .put("high_total", highTotal)
                .put("high_pass", highPass)
                .put("high_failures", JSONArray(highFailures))
                .put("results", results)
        File(outDir, "search-suite-summary.json").writeText(summary.toString(2))

        assertTrue("no high-confidence rows in fixture", highTotal > 0)
        assertTrue(
            "high-confidence per-query failures ($highPass/$highTotal): $highFailures",
            highFailures.isEmpty(),
        )
        assertEquals("high pass must equal high total", highTotal, highPass)
    }
}
