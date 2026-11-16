package no.navi.app

import android.os.SystemClock
import android.util.Log
import uniffi.navi.FfiSearchBatch
import uniffi.navi.FfiSearchHit
import uniffi.navi.FfiSearchHitKind
import uniffi.navi.FfiSearchListener
import uniffi.navi.PlaceHit
import uniffi.navi.comapsSearchCancel
import uniffi.navi.comapsSearchClearViewport
import uniffi.navi.comapsSearchEnabled
import uniffi.navi.comapsSearchMerge
import uniffi.navi.comapsSearchSetLocale
import uniffi.navi.comapsSearchSetPosition
import uniffi.navi.comapsSearchStart
import uniffi.navi.comapsSessionIsOpen
import java.util.Locale
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong

/**
 * Phase 5 search on MWMs for the app search panel.
 *
 * With the MWM client flag ON and an open CoMaps session, [search] collects the
 * streamed batches of `comapsSearchStart` and [merge] combines them with the
 * `place_index` FTS hits through `comapsSearchMerge` (`navi:place_index`
 * label). An empty result (flag off, no session after wait, no hit, timeout)
 * means the caller keeps the FTS list unchanged. Both entry points block; call
 * them off the main thread.
 *
 * Production context (Everywhere / To-field):
 * - locale: [Locale.getDefault] language (or [locale] override), fallback `en`
 * - position: optional bias lat/lon when finite and not (0,0)
 * - viewport: none — always [comapsSearchClearViewport] (Everywhere mode)
 * - mode: free-text (`isCategory=false`); no category/country hints
 * - max results: caller [limit] (UI uses 80)
 *
 * Readiness: [enabled] only means the flag/native are present. The search
 * Engine is created lazily on first successful start. [search] waits until the
 * session is open and a start returns a non-zero id before treating the query
 * as running — it never reports "no results" solely because the engine was
 * still starting.
 */
object ComapsSearchBridge {
    private const val TAG = "NaviSearch"
    private const val NAVI_SOURCE = "navi:place_index"
    private const val COMAPS_KIND_PREFIX = "comaps:"
    private const val POLL_MS = 100L

    /** The engine's own cut-off is 8 s; stay just above it. */
    private const val TIMEOUT_MS = 10_000L

    /** Wait for session + first successful start (map registration can be slow). */
    private const val READY_TIMEOUT_MS = 90_000L

    data class SearchTiming(
        val readyWaitMs: Long,
        val firstResultMs: Long,
        val totalMs: Long,
        val requestId: ULong,
    )

    data class SearchOutcome(
        val hits: List<FfiSearchHit>,
        val timing: SearchTiming,
        /** True when the engine never became ready (not the same as zero hits). */
        val notReady: Boolean = false,
    )

    fun enabled(): Boolean =
        MwmClientBootstrap.isEnabled() &&
            runCatching { comapsSearchEnabled() }.getOrDefault(false)

    private class Collector(
        private val onPartial: ((List<FfiSearchHit>) -> Unit)?,
    ) : FfiSearchListener {
        val hits = CopyOnWriteArrayList<FfiSearchHit>()
        val done = CountDownLatch(1)
        val firstHitAt = AtomicLong(-1L)
        private val startedAt = SystemClock.elapsedRealtime()

        override fun onBatch(batch: FfiSearchBatch) {
            if (batch.hits.isNotEmpty()) {
                if (firstHitAt.get() < 0L) {
                    firstHitAt.compareAndSet(-1L, SystemClock.elapsedRealtime() - startedAt)
                }
                hits.addAll(batch.hits)
                onPartial?.invoke(hits.toList())
            } else if (batch.isFinal && hits.isNotEmpty()) {
                // Final empty batch — still notify with the cumulative set.
                onPartial?.invoke(hits.toList())
            }
            if (batch.isFinal) done.countDown()
        }
    }

    /**
     * Apply the production Everywhere search context: locale, optional position,
     * cleared viewport. Shared by [search], [searchCategory], and instrumented
     * harnesses so headless / in-app match the app.
     */
    fun applyEverywhereContext(
        locale: String?,
        biasLat: Double?,
        biasLon: Double?,
    ) {
        val lang =
            locale?.trim()?.takeIf { it.isNotEmpty() }
                ?: Locale.getDefault().language.ifBlank { "en" }
        comapsSearchSetLocale(lang)
        comapsSearchClearViewport()
        val pos =
            if (biasLat != null &&
                biasLon != null &&
                biasLat.isFinite() &&
                biasLon.isFinite() &&
                !(biasLat == 0.0 && biasLon == 0.0)
            ) {
                biasLat to biasLon
            } else {
                null
            }
        comapsSearchSetPosition(pos?.first ?: 0.0, pos?.second ?: 0.0, pos != null)
    }

    /**
     * CoMaps hits for [query], in engine order. Waits for the search engine to
     * become ready rather than treating a cold start as an empty result.
     *
     * @param locale when null, uses the device default language (production path)
     * @param onPartial optional cumulative-hit callback (search worker thread)
     */
    fun search(
        query: String,
        biasLat: Double?,
        biasLon: Double?,
        limit: Int,
        locale: String? = null,
        stillActive: () -> Boolean = { true },
        onPartial: ((List<FfiSearchHit>) -> Unit)? = null,
    ): List<FfiSearchHit> = searchDetailed(query, biasLat, biasLon, limit, locale, false, stillActive, onPartial).hits

    fun searchDetailed(
        query: String,
        biasLat: Double?,
        biasLon: Double?,
        limit: Int,
        locale: String? = null,
        isCategory: Boolean = false,
        stillActive: () -> Boolean = { true },
        onPartial: ((List<FfiSearchHit>) -> Unit)? = null,
    ): SearchOutcome {
        if (!enabled()) {
            return SearchOutcome(
                hits = emptyList(),
                timing = SearchTiming(0, -1, 0, 0u),
                notReady = true,
            )
        }
        return runCatching {
            applyEverywhereContext(locale, biasLat, biasLon)
            val collector = Collector(onPartial)
            val searchStartedAt = SystemClock.elapsedRealtime()
            // Poll session open + start until the engine accepts the request.
            val tReady0 = SystemClock.elapsedRealtime()
            var id = 0uL
            var readyWait = 0L
            while (stillActive() && SystemClock.elapsedRealtime() - tReady0 < READY_TIMEOUT_MS) {
                if (!runCatching { comapsSessionIsOpen() }.getOrDefault(false)) {
                    Thread.sleep(POLL_MS)
                    continue
                }
                // Re-apply context after session becomes open (locale/viewport).
                applyEverywhereContext(locale, biasLat, biasLon)
                id = comapsSearchStart(query, isCategory, limit.toUInt(), collector)
                if (id != 0uL) {
                    readyWait = SystemClock.elapsedRealtime() - tReady0
                    break
                }
                Thread.sleep(POLL_MS)
            }
            readyWait = SystemClock.elapsedRealtime() - tReady0
            if (id == 0uL) {
                // Cancelled mid-wait: not a timeout — caller should not show an error.
                val timedOut = stillActive()
                if (timedOut) {
                    Log.w(TAG, "comaps_search not ready after ${readyWait}ms q=$query")
                }
                return@runCatching SearchOutcome(
                    hits = emptyList(),
                    timing = SearchTiming(readyWait, -1, readyWait, 0u),
                    notReady = timedOut,
                )
            }
            while (!collector.done.await(POLL_MS, TimeUnit.MILLISECONDS)) {
                if (!stillActive() || SystemClock.elapsedRealtime() - searchStartedAt > TIMEOUT_MS + readyWait) {
                    comapsSearchCancel(id)
                    collector.done.await(2_000L, TimeUnit.MILLISECONDS)
                    break
                }
            }
            val total = SystemClock.elapsedRealtime() - searchStartedAt
            val first = collector.firstHitAt.get()
            val hits = if (!stillActive()) emptyList() else collector.hits.toList()
            Log.i(
                TAG,
                "comaps_search q=$query id=$id ready_ms=$readyWait first_ms=$first total_ms=$total " +
                    "n=${hits.size} not_ready=false",
            )
            SearchOutcome(
                hits = hits,
                timing = SearchTiming(readyWait, first, total, id),
                notReady = false,
            )
        }.getOrElse { e ->
            Log.w(TAG, "comaps_search failed q=$query: ${e.message}")
            SearchOutcome(
                hits = emptyList(),
                timing = SearchTiming(0, -1, 0, 0u),
                notReady = true,
            )
        }
    }

    /**
     * Category-mode search with the same Everywhere locale/position/viewport
     * context as [search]. Used by the Phase 5 suite; production chips typically
     * go through free-text [search] after resolving a synonym.
     */
    fun searchCategory(
        queryText: String,
        biasLat: Double?,
        biasLon: Double?,
        limit: Int,
        locale: String? = null,
        timeoutSec: Long = 60,
        stillActive: () -> Boolean = { true },
    ): List<FfiSearchHit> {
        val outcome =
            searchDetailed(
                query = queryText,
                biasLat = biasLat,
                biasLon = biasLon,
                limit = limit,
                locale = locale,
                isCategory = true,
                stillActive = stillActive,
            )
        // Category paths historically waited up to [timeoutSec]; searchDetailed
        // already caps near the engine timeout once started.
        if (outcome.timing.totalMs > timeoutSec * 1000L && outcome.hits.isEmpty()) {
            Log.w(TAG, "comaps_search_category timed out q=$queryText")
        }
        return outcome.hits
    }

    /**
     * Merge [comaps] with the Navi FTS [navi] hits. Result order is the
     * engine's (CoMaps order, exact Navi name matches first). Navi-origin rows
     * keep their original [PlaceHit] (region id included); CoMaps rows get an
     * empty region id like other synthetic hits.
     */
    fun merge(
        query: String,
        comaps: List<FfiSearchHit>,
        navi: List<PlaceHit>,
        limit: Int,
    ): List<PlaceHit> {
        if (comaps.isEmpty()) return navi
        val originals = HashMap<String, PlaceHit>()
        for (h in navi) originals.putIfAbsent(hitKey(h.name, h.lat, h.lon), h)
        val merged =
            runCatching {
                comapsSearchMerge(query, false, comaps, navi.map(::naviToFfi), limit.toUInt())
            }.getOrElse { e ->
                Log.w(TAG, "comaps_search_merge failed q=$query: ${e.message}")
                return navi
            }
        return merged.map { h ->
            val original =
                if (h.source.startsWith("navi:")) originals[hitKey(h.name, h.lat, h.lon)] else null
            original ?: ffiToPlace(h)
        }
    }

    internal fun ffiToPlace(h: FfiSearchHit): PlaceHit {
        val trimmed = h.featureType.trim()
        val type = trimmed.lowercase().ifBlank { "feature" }
        return PlaceHit(
            osmId = 0L,
            name = h.name,
            kind = COMAPS_KIND_PREFIX + type,
            lat = h.lat,
            lon = h.lon,
            subArea = h.address,
            municipality = "",
            regionId = "",
        )
    }

    private fun naviToFfi(h: PlaceHit): FfiSearchHit =
        FfiSearchHit(
            name = h.name,
            address = joinNonBlank(h.subArea, h.municipality),
            featureType = h.kind,
            lat = h.lat,
            lon = h.lon,
            distanceM = -1.0,
            kind = FfiSearchHitKind.FEATURE,
            source = NAVI_SOURCE,
            mergedSources = emptyList(),
            categoryKey = null,
            featureId = null,
            mwmName = null,
        )

    private fun joinNonBlank(vararg parts: String): String =
        parts
            .filter { it.isNotBlank() }
            .joinToString(", ")

    private fun hitKey(
        name: String,
        lat: Double,
        lon: Double,
    ) = "$name|$lat|$lon"
}
