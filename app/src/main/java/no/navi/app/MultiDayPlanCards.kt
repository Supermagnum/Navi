package no.navi.app

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import org.json.JSONArray
import org.json.JSONObject

/** One planned day from `CorridorRouteResult.daysJson`. */
data class MultiDayCard(
    val dayIndex: Int,
    val date: String = "",
    val startKm: Double = 0.0,
    val endKm: Double = 0.0,
    val distanceKm: Double = 0.0,
    val drivingHours: Double = 0.0,
    val profile: String = "",
    val restKind: String = "",
    val restHours: Double = 0.0,
    val restLabel: String = "",
    val overnightName: String = "",
    val overnightFound: Boolean = false,
    val safetyRejected: Boolean = false,
    val safetyReason: String = "",
    val membershipRequired: Boolean = false,
    val notInCab: Boolean = false,
    val compensation: String = "",
    val isFinal: Boolean = false,
)

fun parseDaysJson(raw: String): List<MultiDayCard> {
    if (raw.isBlank() || raw == "[]") return emptyList()
    return try {
        val arr = JSONArray(raw)
        buildList {
            for (i in 0 until arr.length()) {
                val o = arr.optJSONObject(i) ?: continue
                add(multiDayCardFromJson(o))
            }
        }
    } catch (_: Exception) {
        emptyList()
    }
}

private fun multiDayCardFromJson(o: JSONObject): MultiDayCard =
    MultiDayCard(
        dayIndex = o.optInt("day_index", 0),
        date = o.optString("date").orEmpty(),
        startKm = o.optDouble("start_km", 0.0),
        endKm = o.optDouble("end_km", 0.0),
        distanceKm = o.optDouble("distance_km", 0.0),
        drivingHours = o.optDouble("driving_hours", 0.0),
        profile = o.optString("profile").orEmpty(),
        restKind = o.optString("rest_kind").orEmpty(),
        restHours = o.optDouble("rest_hours", 0.0),
        restLabel = o.optString("rest_label").orEmpty(),
        overnightName = o.optString("overnight_name").orEmpty(),
        overnightFound = o.optBoolean("overnight_found", false),
        safetyRejected = o.optBoolean("safety_rejected", false),
        safetyReason = o.optString("safety_reason").orEmpty(),
        membershipRequired = o.optBoolean("membership_required", false),
        notInCab = o.optBoolean("not_in_cab", false),
        compensation = o.optString("compensation").orEmpty(),
        isFinal = o.optBoolean("is_final", false),
    )

/**
 * Day-by-day plan cards for multi-day corridors (truck / motor / hiking).
 * Shown only when [days] has more than one entry.
 */
@Composable
fun MultiDayPlanCards(
    days: List<MultiDayCard>,
    modifier: Modifier = Modifier,
    unitSystem: UnitSystem = UnitSystem.METRIC,
) {
    if (days.size <= 1) return
    Column(
        modifier =
            modifier
                .fillMaxWidth()
                .testTag("multi_day_plan_cards"),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(
            "Multi-day plan (${days.size} days)",
            style = MaterialTheme.typography.titleSmall,
            modifier = Modifier.testTag("multi_day_plan_title"),
        )
        days.forEach { day ->
            Surface(
                shape = RoundedCornerShape(10.dp),
                tonalElevation = 3.dp,
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .testTag("multi_day_card_${day.dayIndex}"),
            ) {
                Column(modifier = Modifier.padding(10.dp)) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        val title =
                            buildString {
                                append("Day ${day.dayIndex}")
                                if (day.date.isNotBlank()) append(" · ${day.date}")
                                if (day.isFinal) append(" (arrival)")
                            }
                        Text(title, style = MaterialTheme.typography.bodyLarge)
                        if (day.distanceKm > 0.0) {
                            Text(
                                DisplayUnits.formatDistanceKm(day.distanceKm, unitSystem),
                                style = MaterialTheme.typography.bodyMedium,
                            )
                        }
                    }
                    val driveLine =
                        buildList {
                            if (day.drivingHours > 0.0) {
                                add("%.1f h driving".format(day.drivingHours))
                            }
                            if (day.startKm > 0.0 || day.endKm > 0.0) {
                                add(
                                    DisplayUnits.formatDistanceKmRange(
                                        day.startKm,
                                        day.endKm,
                                        unitSystem,
                                    ),
                                )
                            }
                        }.joinToString(" · ")
                    if (driveLine.isNotBlank()) {
                        Text(driveLine, style = MaterialTheme.typography.bodySmall)
                    }
                    if (!day.isFinal) {
                        val overnight =
                            when {
                                day.safetyRejected && day.safetyReason.isNotBlank() ->
                                    " — ${day.safetyReason}"
                                day.overnightFound && day.overnightName.isNotBlank() ->
                                    " @ ${day.overnightName}"
                                day.overnightName.isNotBlank() ->
                                    " @ ${day.overnightName} (approx)"
                                else ->
                                    " — no match found; plan continued"
                            }
                        val rest =
                            when {
                                day.safetyRejected && day.safetyReason.isNotBlank() ->
                                    day.safetyReason
                                day.restLabel.isNotBlank() -> day.restLabel
                                day.membershipRequired ->
                                    "Network hut nearby (membership required)"
                                day.restHours > 0.0 -> "%.0f h rest".format(day.restHours)
                                else -> "Overnight"
                            }
                        val cab = if (day.notInCab) " — not in cab" else ""
                        // When rest_label already is the exclusion reason, avoid duplicating it.
                        val line =
                            if (day.safetyRejected && day.safetyReason.isNotBlank() && rest == day.safetyReason) {
                                rest + cab
                            } else {
                                "$rest$overnight$cab"
                            }
                        Text(
                            line,
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.testTag("multi_day_overnight_${day.dayIndex}"),
                        )
                    }
                    if (day.compensation.isNotBlank()) {
                        Text(
                            day.compensation,
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                }
            }
        }
    }
}
