package no.navi.app

import android.os.Looper
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.navi.CampingCallKind
import uniffi.navi.CampingCallResult
import uniffi.navi.campingPluginEvaluationBackend
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
        assertWasmtimeBackend()
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
     * Corridor overnight suggest. Host supplies junctions; the guest evaluates in wasmtime.
     */
    suspend fun suggestAlongRoute(max: UInt = 12u): CampingCallResult =
        withContext(Dispatchers.Default) {
            assertOffMainThread("campingPluginSuggestAlongRoute")
            assertWasmtimeBackend()
            campingPluginSetTimezone(TimeZone.getDefault().id)
            val result = campingPluginSuggestAlongRoute(max)
            assertGuestPath(result)
            result
        }

    fun assertOffMainThread(label: String) {
        if (BuildConfig.DEBUG && Looper.getMainLooper().isCurrentThread) {
            error("$label must not be called on the Android main thread")
        }
    }

    fun assertWasmtimeBackend() {
        if (BuildConfig.DEBUG) {
            val backend = campingPluginEvaluationBackend()
            if (backend != "wasmtime") {
                error("camping evaluation backend must be wasmtime, got $backend")
            }
        }
    }

    fun assertGuestPath(result: CampingCallResult) {
        if (!BuildConfig.DEBUG) return
        if (result.kind != CampingCallKind.OK) return
        val json = result.resultJson ?: error("wasmtime OK result must include JSON")
        if (!json.contains("\"via\":\"wasmtime\"")) {
            error("camping suggest result must come from the wasmtime guest")
        }
    }
}
