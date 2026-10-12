package no.navi.app

import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import uniffi.navi.ImuAlarmSettings
import uniffi.navi.ImuCallKind
import uniffi.navi.ImuTickResult

/** The native plugin session surface (UniFFI in production, a fake in JVM tests). */
interface ImuPluginApi {
    fun tick(): ImuTickResult

    fun isEnabled(): Boolean

    fun setEnabled(enabled: Boolean): String

    fun requestCalibrate(): String

    fun requestReset(): String

    fun releaseSensors()

    fun alarmSettings(): ImuAlarmSettings

    fun setAlarmSettings(settings: ImuAlarmSettings): ImuAlarmSettings
}

/** What the settings UI renders. */
data class ImuUiState(
    val initialized: Boolean = false,
    val enabled: Boolean = false,
    /** Latest guest display state; null until the first successful tick. */
    val readout: ImuReadout? = null,
    /** Why the plugin is not producing readings (disabled for the session, unavailable, ...). */
    val message: String? = null,
    /** The last tick failed; the loop stays stopped until the user toggles the plugin. */
    val failed: Boolean = false,
)

/**
 * Host-side IMU plugin logic: tick the guest, publish its state, sound the alarm
 * tone for the reported level, and route Calibrate / Reset / alarm settings.
 */
class ImuPluginController(
    private val api: ImuPluginApi,
    private val player: ImuAlarmTonePlayer,
    private val policy: ImuAlarmPolicy = ImuAlarmPolicy(),
    private val clockMs: () -> Long = { System.nanoTime() / 1_000_000L },
) {
    private val mutable = MutableStateFlow(ImuUiState())
    val state: StateFlow<ImuUiState> = mutable.asStateFlow()

    @Volatile
    private var foreground = false

    /** Mark the native session ready and pick up the persisted enable flag. */
    fun markInitialized() {
        mutable.value = mutable.value.copy(initialized = true, enabled = api.isEnabled())
    }

    fun markUnavailable(message: String) {
        mutable.value = mutable.value.copy(initialized = false, message = message, failed = true)
    }

    fun setForeground(value: Boolean) {
        foreground = value
    }

    fun shouldRun(): Boolean = mutable.value.let { it.initialized && it.enabled && !it.failed } && foreground

    fun setEnabled(enabled: Boolean): String {
        val result = api.setEnabled(enabled)
        if (!result.startsWith("OK")) {
            mutable.value = mutable.value.copy(message = result)
            return result
        }
        if (!enabled) {
            api.releaseSensors()
            policy.reset()
        }
        mutable.value =
            mutable.value.copy(
                enabled = enabled,
                readout = null,
                message = null,
                failed = false,
            )
        return result
    }

    /** Run one guest tick and publish the outcome. Blocking: call off the main thread. */
    fun tickOnce() {
        if (!mutable.value.let { it.initialized && it.enabled }) return
        val result = api.tick()
        if (result.kind != ImuCallKind.OK) {
            api.releaseSensors()
            policy.reset()
            mutable.value = mutable.value.copy(readout = null, message = result.message, failed = true)
            return
        }
        val readout = ImuReadoutParser.parse(result.stateJson)
        mutable.value = mutable.value.copy(readout = readout, message = null, failed = false)
        if (readout != null) {
            policy.next(readout.alarm, readout.alarmEnabled, clockMs())?.let(player::play)
        }
    }

    /** Tick at [tickMs] while [shouldRun]; unregisters sensors when it stops. */
    suspend fun runLoop(tickMs: Long = TICK_MS) {
        try {
            while (currentCoroutineContext().isActive && shouldRun()) {
                tickOnce()
                delay(tickMs)
            }
        } finally {
            api.releaseSensors()
            policy.reset()
        }
    }

    fun calibrate(): String = api.requestCalibrate()

    fun reset(): String = api.requestReset()

    fun alarmSettings(): ImuAlarmSettings = api.alarmSettings()

    fun setAlarmSettings(settings: ImuAlarmSettings): ImuAlarmSettings = api.setAlarmSettings(settings)

    fun testAlarm(tone: ImuAlarmTone) = player.play(tone)

    fun release() {
        api.releaseSensors()
        player.release()
    }

    companion object {
        /** About 8 Hz: enough for the steady-window calibration, light on the CPU. */
        const val TICK_MS = 125L
    }
}
