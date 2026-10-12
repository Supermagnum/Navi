package no.navi.app

import android.content.Context
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.LifecycleOwner
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import uniffi.navi.ImuAlarmSettings
import uniffi.navi.ImuTickResult
import uniffi.navi.imuPluginAlarmSettings
import uniffi.navi.imuPluginConfigure
import uniffi.navi.imuPluginInstallGuest
import uniffi.navi.imuPluginIsEnabled
import uniffi.navi.imuPluginReleaseSensors
import uniffi.navi.imuPluginRequestCalibrate
import uniffi.navi.imuPluginRequestReset
import uniffi.navi.imuPluginSetAlarmSettings
import uniffi.navi.imuPluginSetEnabled
import uniffi.navi.imuPluginTick
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean

/** [ImuPluginApi] over the UniFFI surface. */
object UniffiImuPluginApi : ImuPluginApi {
    override fun tick(): ImuTickResult = imuPluginTick()

    override fun isEnabled(): Boolean = imuPluginIsEnabled()

    override fun setEnabled(enabled: Boolean): String = imuPluginSetEnabled(enabled)

    override fun requestCalibrate(): String = imuPluginRequestCalibrate()

    override fun requestReset(): String = imuPluginRequestReset()

    override fun releaseSensors() = imuPluginReleaseSensors()

    override fun alarmSettings(): ImuAlarmSettings = imuPluginAlarmSettings()

    override fun setAlarmSettings(settings: ImuAlarmSettings): ImuAlarmSettings = imuPluginSetAlarmSettings(settings)
}

/**
 * Process-wide IMU plugin session: binds the Android `SensorManager` backend,
 * installs the staged wasm guest, and ticks it only while the plugin is enabled
 * and the app is in the foreground (sensors are unregistered otherwise).
 */
object ImuPluginRuntime {
    private const val PLUGIN_NAME = "imu"

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val initStarted = AtomicBoolean(false)
    private val lock = Any()
    private var loop: Job? = null
    private var startedActivities = 0

    val controller: ImuPluginController by lazy {
        ImuPluginController(UniffiImuPluginApi, ToneImuAlarmPlayer())
    }

    /**
     * Follow [owner]'s start/stop: tick while any attached activity is started.
     * Also kicks off one-time native setup off the main thread.
     */
    fun attach(
        context: Context,
        owner: LifecycleOwner,
    ) {
        val app = context.applicationContext
        owner.lifecycle.addObserver(
            LifecycleEventObserver { _, event ->
                when (event) {
                    Lifecycle.Event.ON_START -> setStarted(started = true)
                    Lifecycle.Event.ON_STOP -> setStarted(started = false)
                    else -> Unit
                }
            },
        )
        scope.launch { ensureInitialized(app) }
    }

    /** Configure the session and install the guest. Blocking; call off the main thread. */
    fun ensureInitialized(context: Context) {
        if (!initStarted.compareAndSet(false, true)) return
        try {
            val filesDir = context.filesDir
            val backend = ImuSensorBackend(AndroidSensorSource(context))
            val status = imuPluginConfigure(filesDir.absolutePath, backend)
            check(status == "OK") { "imu configure: $status" }
            installGuestFromAssets(context)
            controller.markInitialized()
            reconcile()
        } catch (e: Exception) {
            initFailed(e)
        } catch (e: LinkageError) {
            // Native library missing for this ABI: fail closed, the app keeps working.
            initFailed(e)
        }
    }

    private fun initFailed(e: Throwable) {
        initStarted.set(false)
        controller.markUnavailable("IMU plugin unavailable: ${e.message}")
    }

    fun setEnabled(enabled: Boolean): String {
        val result = controller.setEnabled(enabled)
        reconcile()
        return result
    }

    private fun setStarted(started: Boolean) {
        synchronized(lock) {
            startedActivities = (startedActivities + if (started) 1 else -1).coerceAtLeast(0)
            controller.setForeground(startedActivities > 0)
        }
        reconcile()
    }

    /** Start or stop the tick loop to match enabled / foreground state. */
    private fun reconcile() {
        synchronized(lock) {
            val running = loop?.isActive == true
            if (controller.shouldRun()) {
                if (!running) loop = scope.launch { controller.runLoop() }
            } else if (running) {
                loop?.cancel()
                loop = null
            }
        }
    }

    /**
     * Copy the guest staged under `assets/plugins/imu` into the session. Reinstalls
     * when the app version changes so an upgraded guest replaces the old one.
     */
    private fun installGuestFromAssets(context: Context) {
        val marker = File(context.filesDir, "plugins/$PLUGIN_NAME/.installed")
        val wasm = File(context.filesDir, "plugins/$PLUGIN_NAME/plugin.wasm")
        val version = BuildConfig.VERSION_CODE.toString()
        if (marker.isFile && wasm.isFile && marker.readText() == version) return
        val assets = context.assets
        val manifest = assets.open("plugins/$PLUGIN_NAME/plugin.json").use { it.readBytes().decodeToString() }
        val bytes = assets.open("plugins/$PLUGIN_NAME/plugin.wasm").use { it.readBytes() }
        val status = imuPluginInstallGuest(manifest, bytes)
        check(status.startsWith("OK")) { "imu install: $status" }
        marker.parentFile?.mkdirs()
        marker.writeText(version)
    }
}
