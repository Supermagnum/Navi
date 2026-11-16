package no.navi.app

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.provider.OpenableColumns
import android.util.Log
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.util.concurrent.Executors

/**
 * GPX 1.1 import (tracks, routes, waypoints). Entry points: the open-with / share intent filters of
 * [MainActivity] and the "Import GPX" button of the Tools menu. The file is parsed by
 * `core::import::gpx_import` (Rust); what the parser could not use is listed to the user.
 *
 * The line is drawn on the drape (CoMaps tracks, `navi-gpx` group) by [ComapsDrapeMap]. "Navigate along
 * track" turns the track geometry itself into the active route ([MwmProductionPlanner.trackRoute]) so
 * the HUD, ETA, progress and simulation work on the file's line; "Route to start" plans a normal
 * route from the current position to the first point.
 */
object GpxImport {
    private const val TAG = "GpxImport"
    const val MAX_BYTES = 40 * 1024 * 1024
    private const val MAX_POINTS_PER_LINE = 6000

    data class Line(
        val name: String,
        val kind: String,
        /** Flattened `[lat, lon, ...]`, simplified for display and navigation. */
        val latlon: DoubleArray,
        val originalPoints: Int,
        val lengthM: Double,
        val ascentM: Double?,
    ) {
        val pointCount: Int get() = latlon.size / 2
    }

    data class Wpt(
        val name: String,
        val desc: String,
        val lat: Double,
        val lon: Double,
        val sym: String,
    )

    data class Doc(
        val source: String,
        val name: String,
        val lines: List<Line>,
        val waypoints: List<Wpt>,
        val unsupported: List<String>,
        /** minLat, minLon, maxLat, maxLon. */
        val bounds: DoubleArray?,
    )

    data class UiState(
        val loading: Boolean = false,
        val doc: Doc? = null,
        val selected: Int = 0,
        val error: String? = null,
        val panelVisible: Boolean = false,
        /** Bumped when the camera should fit the imported content. */
        val fitEpoch: Int = 0,
        val lastAction: String? = null,
    )

    var ui by mutableStateOf(UiState())
        private set

    private val executor = Executors.newSingleThreadExecutor { r -> Thread(r, "gpx-import").apply { isDaemon = true } }
    private val main = Handler(Looper.getMainLooper())

    private fun update(f: (UiState) -> UiState) {
        if (Looper.myLooper() == Looper.getMainLooper()) ui = f(ui) else main.post { ui = f(ui) }
    }

    /** True for an intent that carries a GPX file (VIEW with data, or SEND with a stream). */
    fun uriFromIntent(intent: Intent?): Uri? {
        intent ?: return null
        return when (intent.action) {
            Intent.ACTION_VIEW -> intent.data
            Intent.ACTION_SEND ->
                if (android.os.Build.VERSION.SDK_INT >= 33) {
                    intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
                } else {
                    @Suppress("DEPRECATION")
                    intent.getParcelableExtra(Intent.EXTRA_STREAM) as? Uri
                }
            else -> null
        }
    }

    fun importFromIntent(
        context: Context,
        intent: Intent?,
    ): Boolean {
        val uri = uriFromIntent(intent) ?: return false
        importUri(context, uri)
        return true
    }

    private fun displayName(
        context: Context,
        uri: Uri,
    ): String {
        runCatching {
            context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
                if (c.moveToFirst()) c.getString(0)?.let { return it }
            }
        }
        return uri.lastPathSegment?.substringAfterLast('/') ?: "track.gpx"
    }

    fun importUri(
        context: Context,
        uri: Uri,
    ) {
        val app = context.applicationContext
        update { it.copy(loading = true, error = null, panelVisible = true) }
        executor.execute {
            val name = displayName(app, uri)
            val t0 = System.nanoTime()
            val result =
                runCatching {
                    val bytes = readCapped(app, uri)
                    parse(name, bytes)
                }.getOrElse {
                    Log.w(TAG, "import failed for $name: ${it.message}")
                    Result.failure(it)
                }
            val ms = (System.nanoTime() - t0) / 1_000_000
            result
                .onSuccess { doc ->
                    Log.i(
                        TAG,
                        "imported $name in ${ms}ms lines=${doc.lines.map { "${it.kind}:${it.name}:${it.originalPoints}pts:${it.lengthM.toInt()}m" }} " +
                            "waypoints=${doc.waypoints.size} unsupported=${doc.unsupported}",
                    )
                    update { it.copy(loading = false, doc = doc, selected = 0, error = null, panelVisible = true, fitEpoch = it.fitEpoch + 1) }
                }.onFailure { e ->
                    update { it.copy(loading = false, error = e.message ?: "import failed", panelVisible = true) }
                }
        }
    }

    private fun readCapped(
        context: Context,
        uri: Uri,
    ): ByteArray {
        val out = ByteArrayOutputStream()
        context.contentResolver.openInputStream(uri).use { input ->
            requireNotNull(input) { "cannot open the file" }
            val buf = ByteArray(64 * 1024)
            while (true) {
                val n = input.read(buf)
                if (n < 0) break
                out.write(buf, 0, n)
                require(out.size() <= MAX_BYTES) { "file is larger than ${MAX_BYTES / (1024 * 1024)} MB" }
            }
        }
        return out.toByteArray()
    }

    internal fun parse(
        source: String,
        bytes: ByteArray,
    ): Result<Doc> {
        // Compressed input is not GPX text: say so instead of a confusing XML error.
        if (bytes.size > 2 && ((bytes[0] == 0x1f.toByte() && bytes[1] == 0x8b.toByte()) || (bytes[0] == 'P'.code.toByte() && bytes[1] == 'K'.code.toByte()))) {
            return Result.failure(IllegalArgumentException("Compressed files (zip / gzip) are not supported. Open the .gpx file itself."))
        }
        val text = String(bytes, Charsets.UTF_8).removePrefix("\uFEFF")
        return parseJson(source, uniffi.navi.gpxParseJson(text, MAX_POINTS_PER_LINE.toUInt()))
    }

    internal fun parseJson(
        source: String,
        json: String,
    ): Result<Doc> {
        val j = JSONObject(json)
        if (!j.optBoolean("ok")) return Result.failure(IllegalArgumentException(j.optString("error", "not a GPX file")))

        fun lines(
            arr: JSONArray?,
            kind: String,
        ): List<Line> =
            (0 until (arr?.length() ?: 0)).mapNotNull { i ->
                val o = arr!!.getJSONObject(i)
                val pts = o.getJSONArray("points")
                if (pts.length() < 2) return@mapNotNull null
                val flat = DoubleArray(pts.length() * 2)
                for (k in 0 until pts.length()) {
                    val p = pts.getJSONArray(k)
                    flat[2 * k] = p.getDouble(0)
                    flat[2 * k + 1] = p.getDouble(1)
                }
                Line(
                    name = o.optString("name").takeIf { it.isNotBlank() && it != "null" } ?: "${kind.replaceFirstChar { c -> c.uppercase() }} ${i + 1}",
                    kind = kind,
                    latlon = flat,
                    originalPoints = o.optInt("original_point_count", pts.length()),
                    lengthM = o.optDouble("length_m", 0.0),
                    ascentM = if (o.has("ascent_m") && !o.isNull("ascent_m")) o.optDouble("ascent_m") else null,
                )
            }
        val all = lines(j.optJSONArray("routes"), "route") + lines(j.optJSONArray("tracks"), "track")
        val wp = j.optJSONArray("waypoints")
        val wpts =
            (0 until (wp?.length() ?: 0)).map { i ->
                val o = wp!!.getJSONObject(i)
                Wpt(
                    o.optString("name").takeIf { it != "null" }.orEmpty(),
                    o.optString("desc").takeIf { it != "null" }.orEmpty(),
                    o.getDouble("lat"),
                    o.getDouble("lon"),
                    o.optString("sym").takeIf { it != "null" }.orEmpty(),
                )
            }
        val un = j.optJSONArray("unsupported")
        val b = j.optJSONArray("bounds")
        return Result.success(
            Doc(
                source = source,
                name = j.optString("name").takeIf { it.isNotBlank() && it != "null" } ?: source,
                lines = all,
                waypoints = wpts,
                unsupported = (0 until (un?.length() ?: 0)).map { un!!.getString(it) },
                bounds = b?.let { DoubleArray(4) { i -> it.getDouble(i) } },
            ),
        )
    }

    fun select(index: Int) = update { it.copy(selected = index.coerceIn(0, ((it.doc?.lines?.size ?: 1) - 1).coerceAtLeast(0)), fitEpoch = it.fitEpoch + 1) }

    fun remove() = update { UiState() }

    fun hidePanel() = update { it.copy(panelVisible = false) }

    fun showPanel() = update { it.copy(panelVisible = ui.doc != null || ui.error != null) }

    fun noteAction(text: String) = update { it.copy(lastAction = text) }

    /** Lines for the drape overlay: all lines; the selected one is drawn stronger. */
    internal fun drapeLines(state: UiState): List<DrapeLine> {
        val doc = state.doc ?: return emptyList()
        return doc.lines.mapIndexed { i, l ->
            val sel = i == state.selected
            DrapeLine(l.latlon, if (sel) 0xFFE91E63.toInt() else 0xAA9C27B0.toInt(), if (sel) 6f else 4f)
        }
    }

    internal fun drapeMarks(state: UiState): List<DrapeMark> {
        val doc = state.doc ?: return emptyList()
        return doc.waypoints.take(500).map { DrapeMark(it.lat, it.lon, it.name, DrapeIcon.NONE, DrapeColor.PURPLE) }
    }
}

private fun km(m: Double): String = if (m >= 1000) "%.1f km".format(java.util.Locale.US, m / 1000.0) else "${m.toInt()} m"

/**
 * The GPX card: shows what was imported and offers "Navigate along track" / "Route to start".
 * [onNavigateAlong] and [onRouteToStart] are supplied by the map screen (they own the route state).
 */
@Composable
internal fun GpxImportHost(
    modifier: Modifier = Modifier,
    onNavigateAlong: (GpxImport.Line) -> Unit,
    onRouteToStart: (GpxImport.Line) -> Unit,
) {
    val ui = GpxImport.ui
    if (!ui.panelVisible || (ui.doc == null && ui.error == null && !ui.loading)) return
    Surface(
        shape = RoundedCornerShape(12.dp),
        tonalElevation = 8.dp,
        modifier =
            modifier
                .padding(10.dp)
                .fillMaxWidth()
                .zIndex(3f)
                .testTag("gpx_card"),
    ) {
        Column(
            Modifier.padding(12.dp).verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            when {
                ui.loading -> Text(stringResource(R.string.gpx_loading))
                ui.error != null -> {
                    Text(stringResource(R.string.gpx_import_failed, ui.error ?: ""), color = MaterialTheme.colorScheme.error, modifier = Modifier.testTag("gpx_error"))
                    TextButton(onClick = { GpxImport.remove() }, modifier = Modifier.testTag("btn_gpx_close")) { Text(stringResource(R.string.mwm_mig_close)) }
                }
                else -> {
                    val doc = ui.doc ?: return@Column
                    Text(doc.name, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold, modifier = Modifier.testTag("gpx_title"))
                    val tracks = doc.lines.count { it.kind == "track" }
                    val routes = doc.lines.count { it.kind == "route" }
                    Text(stringResource(R.string.gpx_summary, tracks, routes, doc.waypoints.size), modifier = Modifier.testTag("gpx_summary"))
                    if (doc.lines.size > 1) {
                        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            doc.lines.forEachIndexed { i, l ->
                                OutlinedButton(
                                    onClick = { GpxImport.select(i) },
                                    enabled = i != ui.selected,
                                    modifier = Modifier.testTag("btn_gpx_line_$i"),
                                ) { Text("${l.name} (${km(l.lengthM)})", maxLines = 1) }
                            }
                        }
                    }
                    val sel = doc.lines.getOrNull(ui.selected)
                    if (sel != null) {
                        Text(
                            stringResource(R.string.gpx_selected, sel.name, km(sel.lengthM), sel.originalPoints) +
                                (sel.ascentM?.let { " " + stringResource(R.string.gpx_ascent, it.toInt()) } ?: ""),
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.testTag("gpx_selected"),
                        )
                    } else {
                        Text(stringResource(R.string.gpx_no_line), style = MaterialTheme.typography.bodySmall)
                    }
                    if (doc.unsupported.isNotEmpty()) {
                        Text(
                            stringResource(R.string.gpx_unsupported, doc.unsupported.joinToString("; ")),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.error,
                            modifier = Modifier.testTag("gpx_unsupported"),
                        )
                    }
                    ui.lastAction?.let { Text(it, style = MaterialTheme.typography.bodySmall, modifier = Modifier.testTag("gpx_action_status")) }
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                        Button(
                            onClick = { sel?.let(onNavigateAlong) },
                            enabled = sel != null,
                            modifier = Modifier.testTag("btn_gpx_navigate"),
                        ) { Text(stringResource(R.string.gpx_navigate_along)) }
                        OutlinedButton(
                            onClick = { sel?.let(onRouteToStart) },
                            enabled = sel != null,
                            modifier = Modifier.testTag("btn_gpx_route_to_start"),
                        ) { Text(stringResource(R.string.gpx_route_to_start)) }
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        TextButton(onClick = { GpxImport.hidePanel() }, modifier = Modifier.testTag("btn_gpx_hide")) { Text(stringResource(R.string.gpx_hide)) }
                        TextButton(onClick = { GpxImport.remove() }, modifier = Modifier.testTag("btn_gpx_remove")) { Text(stringResource(R.string.gpx_remove)) }
                    }
                }
            }
        }
    }
}

/** Tools-menu entry: pick a `.gpx` file with the system file picker. */
@Composable
internal fun GpxImportButton(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val launcher =
        rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            if (uri != null) GpxImport.importUri(context, uri)
        }
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(stringResource(R.string.gpx_title), style = MaterialTheme.typography.titleSmall)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(
                onClick = { launcher.launch(arrayOf("application/gpx+xml", "application/octet-stream", "text/xml", "application/xml", "*/*")) },
                modifier = Modifier.testTag("btn_gpx_import"),
            ) { Text(stringResource(R.string.gpx_import)) }
            if (GpxImport.ui.doc != null) {
                OutlinedButton(onClick = { GpxImport.showPanel() }, modifier = Modifier.testTag("btn_gpx_show_card")) {
                    Text(stringResource(R.string.gpx_show_card))
                }
            }
        }
    }
}
