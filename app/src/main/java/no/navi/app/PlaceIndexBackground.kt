package no.navi.app

import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.navi.ensurePlaceIndex
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * Place-index rebuild that outlives Compose [LaunchedEffect] cancellation.
 * Indexing a regional PBF takes minutes; tying it to composition cancelled the
 * work before `place_index.db` was created.
 */
object PlaceIndexBackground {
    private const val TAG = "PlaceIndexBg"
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val mutex = Mutex()
    private val running = AtomicBoolean(false)
    private val lastStatus = AtomicReference("idle")

    fun isRunning(): Boolean = running.get()

    fun statusLine(): String = lastStatus.get()

    fun ensureStarted(
        pbf: File,
        indexDb: File,
    ) {
        if (!pbf.isFile) return
        scope.launch {
            mutex.withLock {
                if (running.get()) {
                    Log.i(TAG, "already running; skip")
                    return@withLock
                }
                running.set(true)
                lastStatus.set("building")
                Log.i(TAG, "start ensurePlaceIndex pbf=${pbf.absolutePath} db=${indexDb.absolutePath}")
                try {
                    val report = ensurePlaceIndex(pbf.absolutePath, indexDb.absolutePath)
                    val bytes = if (indexDb.isFile) indexDb.length() else 0L
                    lastStatus.set(
                        if (report.contains("PASS")) {
                            "done bytes=$bytes"
                        } else {
                            "failed"
                        },
                    )
                    Log.i(TAG, "finished bytes=$bytes report=$report")
                } catch (t: Throwable) {
                    lastStatus.set("failed: ${t.message}")
                    Log.e(TAG, "ensurePlaceIndex crashed", t)
                } finally {
                    running.set(false)
                }
            }
        }
    }
}
