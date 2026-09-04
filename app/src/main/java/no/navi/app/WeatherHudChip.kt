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

data class WeatherHudState(
    val active: Boolean = false,
    val iconSlug: String = "",
    val tempC: Double? = null,
    val summary: String = "",
    val stale: Boolean = false,
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
            stale = sample.optBoolean("stale", false) || !o.optBoolean("fetched", false),
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
            stale = sample.optBoolean("stale", true),
            providerDiag = sample.optString("provider", ""),
        )
    }.getOrDefault(WeatherHudState())
}

@Composable
fun WeatherHudChip(
    state: WeatherHudState,
    weatherIconsDir: String,
    onRefresh: (() -> Unit)? = null,
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
    val label =
        buildString {
            append(temp)
            if (state.stale) {
                if (isNotEmpty()) append(" · ")
                append("Stale")
            }
        }.ifBlank { state.summary.ifBlank { "Weather" } }
    val desc =
        buildString {
            append("Weather")
            if (temp.isNotEmpty()) append(" $temp")
            if (state.stale) append(", stale data")
        }
    Row(
        modifier =
            modifier
                .background(Color(0xE6FFFFFF), RoundedCornerShape(8.dp))
                .border(1.dp, Color(0xFF90A4AE), RoundedCornerShape(8.dp))
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
            if (state.stale) {
                Text(
                    "Last known — offline or throttled",
                    style = MaterialTheme.typography.bodySmall,
                    color = Color(0xFF546E7A),
                    modifier = Modifier.testTag("weather_stale_label"),
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
