package no.navi.app

import kotlinx.coroutines.delay
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/**
 * App-wide online place-search limiter: at most one HTTP request per second,
 * a newer call replaces a still-pending one, and identical queries in this
 * session are answered from a small cache.
 */
class OnlinePlaceSearchLimiter(
    private val minIntervalMs: Long = 1_000L,
    private val cacheLimit: Int = 16,
    private val clockMs: () -> Long = { System.currentTimeMillis() },
    private val wait: suspend (Long) -> Unit = { delay(it) },
) {
    private val mutex = Mutex()
    private val cache =
        object : LinkedHashMap<String, String>(16, 0.75f, true) {
            override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, String>?): Boolean = size > cacheLimit
        }
    private val recorded = ArrayList<OnlineHttpRequest>()
    private var lastStartMs: Long = 0L
    private var generation: Int = 0

    fun recordedRequests(): List<OnlineHttpRequest> =
        synchronized(recorded) {
            recorded.toList()
        }

    fun resetForTests() {
        lastStartMs = 0L
        generation = 0
        cache.clear()
        synchronized(recorded) { recorded.clear() }
    }

    suspend fun get(
        cacheKey: String,
        request: OnlineHttpRequest,
        client: OnlineHttpClient,
    ): OnlineHttpResult {
        mutex.withLock {
            cache[cacheKey]?.let { return OnlineHttpResult.Ok(it) }
        }
        val myGen = mutex.withLock { ++generation }
        val waitMs =
            mutex.withLock {
                val elapsed = clockMs() - lastStartMs
                (minIntervalMs - elapsed).coerceAtLeast(0L)
            }
        if (waitMs > 0L) {
            wait(waitMs)
        }
        val superseded = mutex.withLock { myGen != generation }
        if (superseded) {
            return OnlineHttpResult.Failure(0, "replaced")
        }
        mutex.withLock {
            cache[cacheKey]?.let { return OnlineHttpResult.Ok(it) }
            lastStartMs = clockMs()
        }
        synchronized(recorded) { recorded.add(request) }
        val result =
            try {
                client.execute(request)
            } catch (e: Exception) {
                OnlineHttpResult.Failure(-1, e.message ?: "error")
            }
        if (result is OnlineHttpResult.Ok) {
            mutex.withLock { cache[cacheKey] = result.body }
        }
        return result
    }
}
