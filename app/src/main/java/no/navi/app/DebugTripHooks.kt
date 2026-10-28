package no.navi.app

import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import java.io.File

/**
 * Release-safe entry points for debug-only trip / settings seeding.
 *
 * The real implementation lives in the **debug** source set
 * ([NaviDebugIntent]) and is installed by [DebugTripInitProvider].
 * Release builds keep the no-op [backend] so none of that code is referenced
 * from production paths, and the debug class is absent from the release APK.
 */
object DebugTripHooks {
    const val FORCE_PBF_PACK_DIR = "__navi_force_pbf__"

    interface Backend {
        fun consumeTripExtras(
            context: Context,
            intent: Intent,
            dataDirPath: String,
        ): NaviMapTestHooks.PendingTripPlan?

        fun consumePendingTripFile(
            context: Context,
            dataDirPath: String,
        ): NaviMapTestHooks.PendingTripPlan?

        fun restoreAfterPlan(dataDirPath: String)

        fun debugBuild(context: Context?): Boolean
    }

    private object NoOpBackend : Backend {
        override fun consumeTripExtras(
            context: Context,
            intent: Intent,
            dataDirPath: String,
        ): NaviMapTestHooks.PendingTripPlan? = null

        override fun consumePendingTripFile(
            context: Context,
            dataDirPath: String,
        ): NaviMapTestHooks.PendingTripPlan? = null

        override fun restoreAfterPlan(dataDirPath: String) = Unit

        override fun debugBuild(context: Context?): Boolean {
            val ctx = context ?: return false
            return (ctx.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) != 0
        }
    }

    @Volatile
    var backend: Backend = NoOpBackend

    fun consumeTripExtras(
        context: Context,
        intent: Intent,
        dataDirPath: String,
    ): NaviMapTestHooks.PendingTripPlan? = backend.consumeTripExtras(context, intent, dataDirPath)

    fun consumePendingTripFile(
        context: Context,
        dataDirPath: String,
    ): NaviMapTestHooks.PendingTripPlan? = backend.consumePendingTripFile(context, dataDirPath)

    fun restoreAfterPlan(dataDirPath: String) = backend.restoreAfterPlan(dataDirPath)

    fun restoreAfterPlan(dataDir: File) = restoreAfterPlan(dataDir.absolutePath)

    fun debugBuild(context: Context? = null): Boolean = backend.debugBuild(context)
}

/**
 * Slim debug-trip snapshot for [NaviManeuverDump] (main source set — no dependency
 * on the debug-only [NaviDebugIntent] type).
 */
data class DebugTripDumpContext(
    val profile: String,
    val bikeCapability: String?,
    val vias: List<Pair<Double, Double>>,
    val graph: String,
    val avoidFerries: Boolean?,
    val appliedSettings: Map<String, Boolean>,
    val ignoredSettings: List<String>,
)
