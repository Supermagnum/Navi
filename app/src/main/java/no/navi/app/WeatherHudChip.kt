package no.navi.app

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import org.json.JSONObject
import uniffi.navi.rasterizeWeatherIconPng
import java.text.DateFormat
import java.util.Date
import java.util.Locale

data class WeatherHudState(
    val active: Boolean = false,
    val iconSlug: String = "",
    val tempC: Double? = null,
    val summary: String = "",
    val fetchedAtUnix: Long? = null,
    val nextFetchUnix: Long? = null,
    val providerDiag: String = "",
)

fun weatherHudFromRefreshJson(raw: String): WeatherHudState {
    if (raw.isBlank() || raw == "{}") return WeatherHudState()
    return runCatching {
        val o = JSONObject(raw)
        val sample = o.optJSONObject("sample") ?: return WeatherHudState()
        WeatherHudState(
            active = true,
            iconSlug = sample.optString("icon_slug", ""),
            tempC =
                if (sample.has("temp_c") && !sample.isNull("temp_c")) {
                    sample.optDouble("temp_c")
                } else {
                    null
                },
            summary = sample.optString("summary", ""),
            fetchedAtUnix = unixOrNull(sample, "fetched_at_unix"),
            nextFetchUnix = unixOrNull(o, "next_fetch_unix"),
            providerDiag = o.optString("provider", sample.optString("provider", "")),
        )
    }.getOrDefault(WeatherHudState())
}

fun weatherHudFromSampleJson(raw: String): WeatherHudState {
    if (raw.isBlank() || raw == "[]" || raw == "{}") return WeatherHudState()
    return runCatching {
        val arr = org.json.JSONArray(raw)
        if (arr.length() == 0) return WeatherHudState()
        val sample = arr.getJSONObject(0)
        WeatherHudState(
            active = true,
            iconSlug = sample.optString("icon_slug", ""),
            tempC =
                if (sample.has("temp_c") && !sample.isNull("temp_c")) {
                    sample.optDouble("temp_c")
                } else {
                    null
                },
            summary = sample.optString("summary", ""),
            fetchedAtUnix = unixOrNull(sample, "fetched_at_unix"),
            nextFetchUnix = unixOrNull(sample, "next_fetch_unix"),
            providerDiag = sample.optString("provider", ""),
        )
    }.getOrDefault(WeatherHudState())
}

/**
 * Timing line for the GPS weather pill: prefer upcoming refresh time, else last
 * fetch. Never mentions stale / throttled / offline.
 */
fun weatherHudTimingLabel(
    fetchedAtUnix: Long?,
    nextFetchUnix: Long?,
    nowUnix: Long,
    locale: Locale = Locale.getDefault(),
): String? {
    if (nextFetchUnix != null && nextFetchUnix > nowUnix) {
        return "Next update ${formatHudClock(nextFetchUnix, locale)}"
    }
    if (fetchedAtUnix != null && fetchedAtUnix > 0L) {
        return "Updated ${formatHudClock(fetchedAtUnix, locale)}"
    }
    return null
}

private fun unixOrNull(
    o: JSONObject,
    key: String,
): Long? {
    if (!o.has(key) || o.isNull(key)) return null
    val v = o.optLong(key, 0L)
    return if (v > 0L) v else null
}

private fun formatHudClock(
    unix: Long,
    locale: Locale,
): String {
    val fmt = DateFormat.getTimeInstance(DateFormat.SHORT, locale)
    return fmt.format(Date(unix * 1000L))
}

private val WeatherHudFill = Color(0xE8C8E6C9)
private val WeatherHudBorder = Color(0xFF81C784)

@Composable
fun WeatherHudChip(
    state: WeatherHudState,
    weatherIconsDir: String,
    onRefresh: (() -> Unit)? = null,
    nowUnix: Long = System.currentTimeMillis() / 1000L,
    modifier: Modifier = Modifier,
) {
    if (!state.active || state.iconSlug.isBlank()) return
    val png =
        remember(weatherIconsDir, state.iconSlug) {
            runCatching {
                rasterizeWeatherIconPng(
                    slug = state.iconSlug,
                    width = 72u,
                    height = 72u,
                    weatherIconsDir = weatherIconsDir,
                )
            }.getOrDefault(ByteArray(0))
        }
    val bmp =
        remember(png) {
            if (png.isEmpty()) null else BitmapFactory.decodeByteArray(png, 0, png.size)
        }
    val temp =
        state.tempC?.let { t ->
            val rounded = kotlin.math.round(t).toInt()
            "$rounded°C"
        } ?: ""
    val timing =
        weatherHudTimingLabel(
            fetchedAtUnix = state.fetchedAtUnix,
            nextFetchUnix = state.nextFetchUnix,
            nowUnix = nowUnix,
        )
    val label = temp.ifBlank { state.summary.ifBlank { "Weather" } }
    val desc =
        buildString {
            append("Weather")
            if (temp.isNotEmpty()) append(" $temp")
            if (timing != null) append(", $timing")
        }
    Row(
        modifier =
            modifier
                .background(WeatherHudFill, RoundedCornerShape(8.dp))
                .border(1.dp, WeatherHudBorder, RoundedCornerShape(8.dp))
                .padding(horizontal = 10.dp, vertical = 6.dp)
                .testTag("weather_hud_chip")
                .semantics { contentDescription = desc },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (bmp != null) {
            Image(
                bitmap = bmp.asImageBitmap(),
                contentDescription = null,
                modifier = Modifier.size(36.dp),
            )
        }
        Column {
            Text(label, style = MaterialTheme.typography.titleSmall)
            if (timing != null) {
                Text(
                    timing,
                    style = MaterialTheme.typography.bodySmall,
                    color = Color(0xFF2E7D32),
                    modifier = Modifier.testTag("weather_timing_label"),
                )
            }
        }
        if (onRefresh != null) {
            TextButton(
                onClick = onRefresh,
                modifier = Modifier.testTag("btn_weather_refresh"),
            ) {
                Text("Refresh")
            }
        }
    }
}
