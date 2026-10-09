package no.navi.app

import android.util.Log
import uniffi.navi.downloadProgressSnapshot
import uniffi.navi.ensurePlaceIndex
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * Place-index rebuild that outlives Compose [LaunchedEffect] cancellation.
 * Indexing a regional PBF takes minutes; tying it to composition cancelled the
 * work before `place_index.db` was created.
 *
 * When [regionId] is non-blank, rows are merged under that Geofabrik path so
 * other regions already in the shared DB stay searchable.
 */
object PlaceIndexBackground {
    private const val TAG = "PlaceIndexBg"
    private val running = AtomicBoolean(false)
    private val lastStatus = AtomicReference("idle")
    private val activeRegionId = AtomicReference("")

    fun isRunning(): Boolean = running.get()

    /** Geofabrik path currently being indexed, or blank when idle. */
    fun activeRegionId(): String = activeRegionId.get()

    /**
     * Claim before launching IO so a concurrent [ensureStarted] sees busy.
     * Pair with [RegionDownloadBackground.isRunning] at call sites: the region
     * pipeline owns place-index when a download/resume is already claimed.
     */
    internal fun claimWorker(): Boolean = running.compareAndSet(false, true)

    internal fun releaseWorker() {
        running.set(false)
    }

    /** True when this process must not start a second `ensurePlaceIndex`. */
    internal fun shouldSkipStandaloneIndex(): Boolean = RegionDownloadBackground.isRunning() || running.get()

    private fun annotate(
        label: String,
        regionId: String = activeRegionId.get(),
    ): String {
        val id = regionId.trim().trim('/')
        if (id.isEmpty()) return label
        val seq = RegionProgressMessages.sequenceFor(id)
        return RegionProgressMessages.annotate(label, id, seq?.first, seq?.second)
    }

    fun statusLine(): String {
        if (running.get()) {
            val snap = runCatching { downloadProgressSnapshot() }.getOrNull()
            if (snap != null && snap.label.isNotBlank()) {
                val label = annotate(snap.label)
                val tot = snap.unitsTotal
                val done = snap.unitsDone
                val pct =
                    if (tot != null && tot > 0uL) {
                        ((done.toDouble() * 100.0) / tot.toDouble()).toInt().coerceIn(0, 100)
                    } else {
                        null
                    }
                return when {
                    pct != null && tot != null -> "$label $pct% ($done / $tot)"
                    pct != null -> "$label $pct%"
                    else -> label
                }
            }
        }
        return lastStatus.get()
    }

    fun ensureStarted(
        pbf: File,
        indexDb: File,
        regionId: String? = null,
    ) {
        if (!OfflineIndexGate.isIndexablePbf(pbf)) {
            Log.i(TAG, "skip ensurePlaceIndex: no indexable PBF (${pbf.name})")
            return
        }
        if (RegionDownloadBackground.isRunning()) {
            Log.i(TAG, "region pipeline already running; skip standalone ensurePlaceIndex")
            return
        }
        if (PlaceIndexReady.deferWritesDuringPlan()) {
            Log.i(TAG, "skip ensurePlaceIndex: route plan in flight")
            return
        }
        val rid = regionId?.trim()?.trim('/')?.ifBlank { null }
        if (rid != null && !GeofabrikDownloadCatalog.isKnownPackRegionId(rid)) {
            Log.e(TAG, "refusing ensurePlaceIndex under unknown region_id=$rid")
            lastStatus.set(annotate("failed (unknown region)", rid))
            return
        }
        if (!rid.isNullOrBlank() &&
            !PackRegionAvailability.pbfMatchesRegionForPlaceIndex(pbf, rid)
        ) {
            Log.e(
                TAG,
                "FAIL: refusing place index under wrong PBF region_id=$rid pbf=${pbf.name} " +
                    "expected_stem=${PackRegionAvailability.localStem(rid)} — " +
                    OfflineIndexGate.CANNOT_INDEX_YET,
            )
            lastStatus.set(annotate(OfflineIndexGate.CANNOT_INDEX_YET, rid))
            return
        }
        val dataDir = indexDb.parentFile
        if (dataDir != null && !rid.isNullOrBlank()) {
            if (PlaceIndexIntact.isIntact(dataDir, rid)) {
                Log.i(TAG, "place index intact for $rid; skip")
                return
            }
            if (!PlaceIndexAutoBuild.mayStart(dataDir, rid)) {
                Log.i(
                    TAG,
                    "place-index missing for $rid; not auto-building (list via InstalledMaps)",
                )
                return
            }
        }
        IdlePackJobs.offerPlaceIndex(pbf, indexDb, rid.orEmpty())
    }

    internal fun runJob(
        pbf: File,
        indexDb: File,
        regionId: String?,
    ) {
        runPackJob(pbf.parentFile ?: indexDb.parentFile ?: File("."), indexDb, regionId, pbf)
    }

    internal fun runPackJob(
        packDir: File,
        indexDb: File,
        regionId: String?,
        pbf: File?,
    ) {
        if (!claimWorker()) {
            Log.i(TAG, "already running; skip")
            return
        }
        val rid = regionId?.trim()?.trim('/')?.ifBlank { null }
        activeRegionId.set(rid.orEmpty())
        lastStatus.set(annotate("building", rid.orEmpty()))
        Log.i(
            TAG,
            "start place-index pack=${packDir.absolutePath} pbf=${pbf?.absolutePath} " +
                "db=${indexDb.absolutePath} region=$rid",
        )
        try {
            val stamp = rid?.let { File(packDir, "${PackRegionAvailability.localStem(it)}.navi-server-install.json") }
            val report =
                if (rid != null && stamp?.isFile == true) {
                    uniffi.navi.ensurePlaceIndexForPackRegion(
                        packDir.absolutePath,
                        indexDb.absolutePath,
                        rid,
                    )
                } else if (pbf != null) {
                    ensurePlaceIndex(
                        pbf.absolutePath,
                        indexDb.absolutePath,
                        rid,
                    )
                } else {
                    "FAIL: no pack-server stamp and no extract\n"
                }
            val bytes = if (indexDb.isFile) indexDb.length() else 0L
            if (report.contains("PASS")) {
                val dataDir = indexDb.parentFile
                if (dataDir != null && !rid.isNullOrBlank()) {
                    PlaceIndexReady.markReady(dataDir, rid)
                }
                lastStatus.set(annotate("Place index ready 100% (6 / 6)", rid.orEmpty()))
                if (DownloadProgressClear.shouldClear(
                        regionRunning = RegionDownloadBackground.isRunning(),
                        placeIndexRunning = false,
                    )
                ) {
                    runCatching { uniffi.navi.downloadProgressClear() }
                }
            } else {
                lastStatus.set(annotate("failed", rid.orEmpty()))
            }
            Log.i(TAG, "finished bytes=$bytes report=$report")
        } catch (t: Throwable) {
            lastStatus.set(annotate("failed: ${t.message}", rid.orEmpty()))
            Log.e(TAG, "ensurePlaceIndex crashed", t)
        } finally {
            activeRegionId.set("")
            releaseWorker()
        }
    }
}
