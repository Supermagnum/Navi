package no.navi.app

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiSearchHit
import uniffi.navi.FfiSearchHitKind
import uniffi.navi.comapsCategoryQuery
import uniffi.navi.comapsNativeAvailable
import uniffi.navi.comapsReverseGeocode
import uniffi.navi.comapsSearchEnabled
import uniffi.navi.comapsSessionIsOpen
import java.io.File

/**
 * Phase 5 search suite through the **production** path: starts [MainActivity] so
 * drape / Platform Context init runs, then uses [ComapsSearchBridge] (same bridge
 * the search panel ViewModel path calls).
 *
 * Per-query `user_context` (locale + position) matches the headless harness;
 * viewport is cleared (Everywhere); limit 80. Gate: every high-confidence row
 * must pass individually (no percentage gate). No warm-up query — the bridge
 * waits for engine readiness itself.
 *
 * Output: `{externalFiles}/phase5-inapp/search-suite-summary.json`.
 */
@RunWith(AndroidJUnit4::class)
class ComapsPhase5InAppSearchInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var outDir: File
    private lateinit var dataMwm: File

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
        dataMwm = File(NaviAppData.resolve(ctx), "mwm")
        val testRoot = File(ctx.getExternalFilesDir(null), "navi-mwm-test")
        val productionReady =
            File(dataMwm, "World.mwm").isFile ||
                File(dataMwm, "releases").listFiles()?.any { rel ->
                    File(rel, "leaves").listFiles()?.any { it.name.endsWith(".mwm") } == true
                } == true
        if (!productionReady) {
            assumeTrue("no test MWMs under ${testRoot.absolutePath}", File(testRoot, "World.mwm").isFile)
            ComapsPhase6Harness.stageHeadlessAssets(ctx, testRoot)
            ComapsSessionBootstrap.ensureOpen(ctx, force = true)
        }
        // Same Platform Context the headless suite sets, so address formatting
        // matches (drape may not have called nativeSetContext yet).
        ComapsHeadlessPlatform.requireContext(ctx)
        // Wait only for the activity-owned session — do not warm with a real query.
        val ready =
            ComapsPhase6Harness.waitFor(120_000L, 500L) {
                ComapsSearchBridge.enabled() && comapsSessionIsOpen()
            }
        assumeTrue("ComapsSearchBridge/session not ready after MainActivity start", ready)
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
            val hay = norm(hit.name + " " + hit.address + " " + hit.featureType)
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
                    val needle = norm(wanted.getString(i))
                    top.any { hit ->
                        norm(hit.address + hit.name + hit.featureType).contains(needle) ||
                            (hit.kind == FfiSearchHitKind.POSTCODE && norm(hit.name + hit.address).contains(needle))
                    }
                }
            val rank = if (ok) rankOf(hits, wanted) else -1
            return Triple(
                ok,
                rank,
                if (ok) {
                    ""
                } else {
                    "addresses=${top.map { Triple(it.name, it.address, it.kind) }} " +
                        "mwm=${top.map { it.mwmName }} leaves=${listProductionLeaves()}"
                },
            )
        }
        return Triple(hits.size >= q.optInt("min_results", 0), if (hits.isNotEmpty()) 1 else -1, "n=${hits.size}")
    }

    private fun listProductionLeaves(): List<String> {
        val releases = File(dataMwm, "releases")
        return releases
            .listFiles()
            .orEmpty()
            .flatMap { rel ->
                File(rel, "leaves").listFiles { f -> f.name.endsWith(".mwm") }.orEmpty().map { it.name }
            }.sorted()
    }

    /**
     * Cold start: first Everywhere query after activity/session ready must not
     * return empty solely because the lazy Engine was still building.
     */
    @Test
    fun coldStartImmediateLillehammer() {
        // Force a fresh engine by relying on bridge readiness wait — no Oslo warm-up.
        val outcome =
            ComapsSearchBridge.searchDetailed(
                query = "Lillehammer",
                biasLat = 61.115,
                biasLon = 10.466,
                limit = APP_LIMIT,
                locale = "nb",
            )
        assertFalse("engine must become ready for cold-start search", outcome.notReady)
        assertTrue(
            "cold-start Lillehammer must return hits (got ${outcome.hits.size}; " +
                "ready_ms=${outcome.timing.readyWaitMs} first_ms=${outcome.timing.firstResultMs})",
            outcome.hits.isNotEmpty(),
        )
        val rank =
            rankOf(
                outcome.hits,
                JSONArray().put("Lillehammer"),
            )
        assertTrue("Lillehammer should be in top 3 (rank=$rank)", rank in 1..3)
        File(outDir, "cold-start-lillehammer.json").writeText(
            JSONObject()
                .put("ready_ms", outcome.timing.readyWaitMs)
                .put("first_ms", outcome.timing.firstResultMs)
                .put("total_ms", outcome.timing.totalMs)
                .put("n", outcome.hits.size)
                .put("top", JSONArray(outcome.hits.take(5).map { it.name }))
                .toString(2),
        )
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
            var hit = false
            var rank = -1
            var detail = ""
            var topNames = emptyList<String>()
            var readyMs = -1L
            var firstMs = -1L
            var totalMs = -1L
            when (q.getString("kind")) {
                "free_text" -> {
                    val (locale, lat, lon) = userContext(q)
                    val outcome =
                        ComapsSearchBridge.searchDetailed(
                            query = q.getString("query"),
                            biasLat = lat,
                            biasLon = lon,
                            limit = APP_LIMIT,
                            locale = locale,
                        )
                    readyMs = outcome.timing.readyWaitMs
                    firstMs = outcome.timing.firstResultMs
                    totalMs = outcome.timing.totalMs
                    assertFalse("$id: engine not ready", outcome.notReady)
                    topNames = outcome.hits.take(5).map { it.name }
                    val ev = evaluateFreeText(q, outcome.hits)
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
                        val outcome =
                            ComapsSearchBridge.searchDetailed(
                                query = text,
                                biasLat = lat,
                                biasLon = lon,
                                limit = APP_LIMIT,
                                locale = locale,
                                isCategory = true,
                            )
                        readyMs = outcome.timing.readyWaitMs
                        firstMs = outcome.timing.firstResultMs
                        totalMs = outcome.timing.totalMs
                        topNames = outcome.hits.take(5).map { it.name }
                        val ev = evaluateFreeText(q, outcome.hits)
                        hit = ev.first
                        rank = ev.second
                        detail = listOf(ev.third, "q=$text").filter { it.isNotEmpty() }.joinToString(" ")
                        Thread.sleep(250)
                    }
                }
                "reverse" -> {
                    val t0 = System.currentTimeMillis()
                    val addr = comapsReverseGeocode(q.getDouble("lat"), q.getDouble("lon"))
                    totalMs = System.currentTimeMillis() - t0
                    firstMs = totalMs
                    readyMs = 0
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
                    .put("ready_ms", readyMs)
                    .put("first_ms", firstMs)
                    .put("total_ms", totalMs)
                    .put("ms", totalMs)
                    .put("detail", detail)
                    .put("top", JSONArray(topNames))
                    .put("non_gating_reason", q.optString("non_gating_reason", "")),
            )
        }

        val summary =
            JSONObject()
                .put("path", "in-app-ComapsSearchBridge+MainActivity")
                .put("mwm_root", dataMwm.absolutePath)
                .put("leaves", JSONArray(listProductionLeaves()))
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
