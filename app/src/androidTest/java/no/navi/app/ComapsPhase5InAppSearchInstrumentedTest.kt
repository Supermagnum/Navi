package no.navi.app

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiSearchBatch
import uniffi.navi.FfiSearchHit
import uniffi.navi.FfiSearchListener
import uniffi.navi.comapsCategoryQuery
import uniffi.navi.comapsNativeAvailable
import uniffi.navi.comapsReverseGeocode
import uniffi.navi.comapsSearchEnabled
import uniffi.navi.comapsSearchSetPosition
import uniffi.navi.comapsSearchSetViewport
import uniffi.navi.comapsSearchStart
import java.io.File
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * Phase 5 search suite through the **production** path: starts [MainActivity] so
 * drape / Platform Context init runs, then uses [ComapsSearchBridge] (same bridge
 * the search panel ViewModel path calls) — not the headless FFI harness alone.
 *
 * Output: `{externalFiles}/phase5-inapp/search-suite-summary.json`.
 */
@RunWith(AndroidJUnit4::class)
class ComapsPhase5InAppSearchInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var outDir: File

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
    }

    private fun norm(s: String): String = s.filter { it.isLetterOrDigit() }.lowercase()

    private fun rankOf(
        hits: List<Pair<String, String>>,
        wanted: JSONArray,
    ): Int {
        for ((idx, hit) in hits.withIndex()) {
            val hay = norm(hit.first + " " + hit.second)
            for (i in 0 until wanted.length()) {
                if (hay.contains(norm(wanted.getString(i)))) return idx + 1
            }
        }
        return -1
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
                    val near = q.optJSONObject("near")
                    val biasLat = near?.optDouble("lat")
                    val biasLon = near?.optDouble("lon")
                    val hits =
                        ComapsSearchBridge.search(
                            q.getString("query"),
                            biasLat,
                            biasLon,
                            30,
                        )
                    topNames = hits.take(5).map { it.name }
                    val pairs = hits.map { it.name to it.address }
                    q.optJSONArray("expect_top_any")?.let { wanted ->
                        rank = rankOf(pairs, wanted)
                        hit = rank in 1..q.optInt("top_n", 3)
                        if (!hit) detail = "top=${pairs.take(q.optInt("top_n", 3))}"
                    } ?: run {
                        hit = hits.size >= q.optInt("min_results", 0)
                        detail = "n=${hits.size}"
                    }
                }
                "category" -> {
                    val key = q.getString("query")
                    val text = comapsCategoryQuery(key, "nb")
                    if (text == null) {
                        hit = false
                        detail = "no category string for $key"
                    } else {
                        val near = q.optJSONObject("near")
                        val lat = near?.optDouble("lat") ?: 61.0
                        val lon = near?.optDouble("lon") ?: 10.5
                        comapsSearchSetPosition(lat, lon, true)
                        comapsSearchSetViewport(lat - 0.5, lon - 0.8, lat + 0.5, lon + 0.8)
                        val collector =
                            object : FfiSearchListener {
                                val hits = CopyOnWriteArrayList<FfiSearchHit>()
                                val done = CountDownLatch(1)

                                override fun onBatch(batch: FfiSearchBatch) {
                                    hits.addAll(batch.hits)
                                    if (batch.isFinal) done.countDown()
                                }
                            }
                        val sid = comapsSearchStart(text, true, 30u, collector)
                        assertTrue("category search '$text' did not start", sid != 0uL)
                        assertTrue("category search '$text' timed out", collector.done.await(60, TimeUnit.SECONDS))
                        topNames = collector.hits.take(5).map { it.name }
                        hit = collector.hits.size >= q.optInt("min_results", 1)
                        detail = "n=${collector.hits.size} q=$text"
                        if (hit) rank = 1
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
                if (hit) highPass++
            }
            results.put(
                JSONObject()
                    .put("id", id)
                    .put("kind", q.getString("kind"))
                    .put("confidence", q.getString("confidence"))
                    .put("hit", hit)
                    .put("rank", rank)
                    .put("ms", System.currentTimeMillis() - started)
                    .put("detail", detail)
                    .put("top", JSONArray(topNames)),
            )
        }

        val summary =
            JSONObject()
                .put("path", "in-app-ComapsSearchBridge+MainActivity")
                .put("query_count", queries.length())
                .put("high_total", highTotal)
                .put("high_pass", highPass)
                .put("results", results)
        File(outDir, "search-suite-summary.json").writeText(summary.toString(2))

        assertTrue(
            "high-confidence in-app pass rate too low: $highPass / $highTotal (see phase5-inapp)",
            highTotal > 0 && highPass * 5 >= highTotal * 4,
        )
    }
}
