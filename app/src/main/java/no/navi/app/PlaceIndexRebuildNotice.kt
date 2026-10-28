package no.navi.app

import android.content.Context
import java.io.File
import java.util.concurrent.atomic.AtomicReference

/**
 * Short status line after a torn WAL quarantine marked regions incomplete.
 * Polled from the map screen; does not force a rebuild by itself (AutoBuild does).
 */
object PlaceIndexRebuildNotice {
    private val pendingLine = AtomicReference<String?>(null)

    fun offerRegions(
        context: Context,
        regions: List<String>,
    ) {
        val labels =
            regions
                .map { PackRegionAvailability.normalize(it) }
                .filter { it.isNotEmpty() }
                .map { it.substringAfterLast('/') }
                .distinct()
        if (labels.isEmpty()) return
        val line =
            context.getString(
                R.string.status_place_index_rebuilding_search,
                labels.joinToString(", "),
            )
        pendingLine.set(line)
    }

    fun takeLine(): String? = pendingLine.getAndSet(null)

    /**
     * Drain Rust + Kotlin quarantine queues, clear ready stamps, queue UI line.
     */
    fun drainAfterWalEvent(
        context: Context,
        dataDir: File,
    ) {
        val rust =
            runCatching { uniffi.navi.placeIndexTakeWalQuarantineRebuildRegions() }
                .getOrDefault(emptyList())
                .map { PackRegionAvailability.normalize(it) }
                .filter { it.isNotEmpty() }
        for (rid in rust) {
            PlaceIndexReady.clearReadyStampOnly(dataDir, rid)
        }
        val local = PlaceIndexIntact.takeRebuildNoticeRegions()
        val all = (rust + local).distinct()
        if (all.isNotEmpty()) {
            offerRegions(context, all)
        }
    }
}
