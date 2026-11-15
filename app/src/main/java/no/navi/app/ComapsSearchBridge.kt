package no.navi.app

import android.os.SystemClock
import android.util.Log
import uniffi.navi.FfiSearchBatch
import uniffi.navi.FfiSearchHit
import uniffi.navi.FfiSearchHitKind
import uniffi.navi.FfiSearchListener
import uniffi.navi.PlaceHit
import uniffi.navi.comapsSearchCancel
import uniffi.navi.comapsSearchEnabled
import uniffi.navi.comapsSearchMerge
import uniffi.navi.comapsSearchSetLocale
import uniffi.navi.comapsSearchSetPosition
import uniffi.navi.comapsSearchStart
import java.util.Locale
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * Phase 5 search on MWMs for the app search panel.
 *
 * With the MWM client flag ON and an open CoMaps session, [search] collects the
 * streamed batches of `comapsSearchStart` and [merge] combines them with the
 * `place_index` FTS hits through `comapsSearchMerge` (`navi:place_index`
 * label). An empty result (flag off, no session, no hit, timeout) means the
 * caller keeps the FTS list unchanged. Both entry points block; call them off
 * the main thread.
 */
object ComapsSearchBridge {
    private const val TAG = "NaviSearch"
    private const val NAVI_SOURCE = "navi:place_index"
    private const val COMAPS_KIND_PREFIX = "comaps:"
    private const val POLL_MS = 100L

    /** The engine's own cut-off is 8 s; stay just above it. */
    private const val TIMEOUT_MS = 10_000L

    fun enabled(): Boolean =
        MwmClientBootstrap.isEnabled() &&
            runCatching { comapsSearchEnabled() }.getOrDefault(false)

    private class Collector : FfiSearchListener {
        val hits = CopyOnWriteArrayList<FfiSearchHit>()
        val done = CountDownLatch(1)

        override fun onBatch(batch: FfiSearchBatch) {
            hits.addAll(batch.hits)
            if (batch.isFinal) done.countDown()
        }
    }

    /**
     * CoMaps hits for [query], in engine order. Empty when search is not
     * available, nothing matched, or [stillActive] turned false (the request is
     * then cancelled).
     */
    fun search(
        query: String,
        biasLat: Double?,
        biasLon: Double?,
        limit: Int,
        stillActive: () -> Boolean = { true },
    ): List<FfiSearchHit> {
        if (!enabled()) return emptyList()
        return runCatching {
            comapsSearchSetLocale(Locale.getDefault().language.ifBlank { "en" })
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
            val collector = Collector()
            val id = comapsSearchStart(query, false, limit.toUInt(), collector)
            if (id == 0uL) {
                // No session or search unavailable: caller falls back to FTS.
                return@runCatching emptyList()
            }
            val t0 = SystemClock.elapsedRealtime()
            while (!collector.done.await(POLL_MS, TimeUnit.MILLISECONDS)) {
                if (!stillActive() || SystemClock.elapsedRealtime() - t0 > TIMEOUT_MS) {
                    comapsSearchCancel(id)
                    // A cancelled request still delivers a final batch.
                    collector.done.await(2_000L, TimeUnit.MILLISECONDS)
                    break
                }
            }
            if (!stillActive()) emptyList() else collector.hits.toList()
        }.getOrElse { e ->
            Log.w(TAG, "comaps_search failed q=$query: ${e.message}")
            emptyList()
        }
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
