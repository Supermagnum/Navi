package no.navi.app

import android.os.Looper
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.navi.CampingCallResult
import uniffi.navi.campingPluginRunSuggest
import uniffi.navi.campingPluginSetTimezone
import uniffi.navi.campingPluginSuggestAlongRoute
import java.util.TimeZone

/**
 * Host-side camping suggest entry. Always runs off the main thread and refreshes
 * the device IANA timezone at call time (clock_read freshness).
 */
object CampingPluginApi {
    /**
     * Blocking UniFFI entry — debug builds fail loudly if invoked on the main thread.
     * Prefer [runSuggest].
     */
    fun runSuggestBlocking(jobJson: String): CampingCallResult {
        assertOffMainThread("campingPluginRunSuggest")
        val tz = TimeZone.getDefault().id
        campingPluginSetTimezone(tz)
        return campingPluginRunSuggest(jobJson, tz)
    }

    /** Suspend API: switches to [Dispatchers.Default] then calls the FFI. */
    suspend fun runSuggest(jobJson: String): CampingCallResult =
        withContext(Dispatchers.Default) {
            runSuggestBlocking(jobJson)
        }

    /**
     * Native corridor overnight suggest (Phase 5b). Refreshes timezone on the worker thread.
     */
    suspend fun suggestAlongRoute(max: UInt = 12u): CampingCallResult =
        withContext(Dispatchers.Default) {
            assertOffMainThread("campingPluginSuggestAlongRoute")
            campingPluginSetTimezone(TimeZone.getDefault().id)
            campingPluginSuggestAlongRoute(max)
        }

    fun assertOffMainThread(label: String) {
        if (BuildConfig.DEBUG && Looper.getMainLooper().isCurrentThread) {
            error("$label must not be called on the Android main thread")
        }
    }
}
