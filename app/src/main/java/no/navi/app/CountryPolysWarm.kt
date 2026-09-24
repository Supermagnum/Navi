package no.navi.app

import android.os.SystemClock
import android.util.Log
import kotlinx.coroutines.delay
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Cold Natural Earth country-grid build (`country_polys::build_index`) takes on
 * the order of minutes on Automotive AVDs. Kick it off once on a background
 * thread at process start; Stay-in-Country planning waits briefly or refuses.
 */
object CountryPolysWarm {
    private const val TAG = "CountryPolysWarm"

    /** Max wait on the plan IO path before refusing Stay-in-Country. */
    const val PLAN_WAIT_MS = 90_000L

    private val started = AtomicBoolean(false)
    private val finished = AtomicBoolean(false)

    /** Fire-and-forget warm. Safe to call from [MainActivity.onCreate]. */
    fun startBackground() {
        if (!started.compareAndSet(false, true)) return
        Thread(
            {
                val t0 = SystemClock.elapsedRealtime()
                val bytes =
                    runCatching { uniffi.navi.warmCountryPolys() }
                        .onFailure { e -> Log.w(TAG, "warm failed", e) }
                        .getOrDefault(0uL)
                finished.set(true)
                Log.i(
                    TAG,
                    "warm done bytes=$bytes ready=${uniffi.navi.countryPolysReady()} " +
                        "ms=${SystemClock.elapsedRealtime() - t0}",
                )
            },
            "country-polys-warm",
        ).apply { isDaemon = true }
            .start()
    }

    fun isReady(): Boolean =
        finished.get() ||
            runCatching { uniffi.navi.countryPolysReady() }.getOrDefault(false)

    /**
     * Suspend until the index is ready or [timeoutMs] elapses.
     * Call from [Dispatchers.IO] / plan workers — never the main looper.
     */
    suspend fun awaitReady(timeoutMs: Long = PLAN_WAIT_MS): Boolean {
        if (isReady()) return true
        startBackground()
        val deadline = SystemClock.elapsedRealtime() + timeoutMs
        while (SystemClock.elapsedRealtime() < deadline) {
            if (isReady()) return true
            delay(100)
        }
        return isReady()
    }
}
