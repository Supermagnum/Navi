package no.navi.app

import android.content.Context
import java.io.File
import org.json.JSONObject

/**
 * Canonical on-device location for a finished plan's hop log, polyline, and
 * hops sidecar. Native [plan_file_log] writes the same folder under
 * `filesDir/long-trip-ui-report/`. The app mirrors those files to
 * [Context.getExternalFilesDir] so a host can `adb pull` without `run-as`.
 *
 * Do not scatter `route-polyline.txt` under a different directory than the hop
 * log — a long plan must leave all three artifacts together.
 */
object PlanReportStore {
    const val DIR_NAME = "long-trip-ui-report"
    const val LOG_NAME = "routing-plan.log"
    const val POLYLINE_NAME = "route-polyline.txt"
    const val HOPS_NAME = "hops.json"
    const val RESULT_NAME = "route-result.json"

    fun internalDir(dataDir: File): File = File(dataDir, DIR_NAME)

    fun externalDir(context: Context): File? = context.getExternalFilesDir(null)?.let { File(it, DIR_NAME) }

    /** Host-visible copy when external storage exists; otherwise internal. */
    fun canonicalDir(
        context: Context,
        dataDir: File,
    ): File = externalDir(context) ?: internalDir(dataDir)

    fun writeText(
        dir: File,
        name: String,
        body: String,
    ) {
        dir.mkdirs()
        File(dir, name).writeText(body)
    }

    fun mirrorTo(
        dest: File?,
        src: File,
    ) {
        if (dest == null || !src.isFile) return
        if (dest.canonicalPath == src.canonicalPath) return
        dest.parentFile?.mkdirs()
        src.copyTo(dest, overwrite = true)
    }

    fun publishFinishedPlan(
        context: Context,
        dataDir: File,
        polyline: String,
        hopsJson: String?,
        resultJson: String,
    ) {
        val internal = internalDir(dataDir).also { it.mkdirs() }
        if (polyline.isNotBlank()) {
            writeText(internal, POLYLINE_NAME, polyline)
        }
        if (!hopsJson.isNullOrBlank()) {
            writeText(internal, HOPS_NAME, hopsJson)
        }
        writeText(internal, RESULT_NAME, resultJson)
        val ext = externalDir(context)?.also { it.mkdirs() }
        if (ext != null) {
            for (name in listOf(LOG_NAME, POLYLINE_NAME, HOPS_NAME, RESULT_NAME)) {
                mirrorTo(File(ext, name), File(internal, name))
            }
        }
    }

    /**
     * Rewrite `enumerations` on an already-published [RESULT_NAME] after async
     * camping / attractions tallies finish. Mirrors to external when present.
     */
    fun patchResultEnumerations(
        context: Context,
        dataDir: File,
        enumerations: JSONObject,
    ) {
        val internal = File(internalDir(dataDir), RESULT_NAME)
        if (!internal.isFile) return
        val body =
            runCatching {
                val o = JSONObject(internal.readText())
                o.put("enumerations", enumerations)
                o.toString(2)
            }.getOrNull() ?: return
        writeText(internalDir(dataDir), RESULT_NAME, body)
        val ext = externalDir(context)
        if (ext != null) {
            mirrorTo(File(ext, RESULT_NAME), File(internalDir(dataDir), RESULT_NAME))
        }
    }
}
