package no.navi.app

import org.json.JSONArray
import java.util.Locale

/**
 * Contiguous ferry / tunnel runs from CoMaps `turns_json` (`is_ferry` / `is_tunnel`
 * plus `dist_m` and street/ref/destination), matching the PBF report tokens
 * `route_ferry_legs` / `route_ferry_fp` and `route_tunnel_count` / `route_tunnel_fp`.
 *
 * CoMaps tags a turn step when any segment between the previous and current turn
 * is ferry or tunnel (see `routes_builder.cpp`). Contiguous flagged steps with the
 * same label merge into one leg, matching the PBF graph `path_*_legs` merge rule.
 */
object MwmRouteLegStats {
    data class NamedLeg(
        val name: String,
        val lengthM: Double,
    )

    fun ferryLegsFromTurns(turnsJson: String): List<NamedLeg> = namedLegsFromTurns(turnsJson, flagKey = "is_ferry", ferryStyleLabel = true)

    fun tunnelLegsFromTurns(turnsJson: String): List<NamedLeg> = namedLegsFromTurns(turnsJson, flagKey = "is_tunnel", ferryStyleLabel = false)

    fun formatNamedFp(legs: List<NamedLeg>): String =
        legs.joinToString("|") { leg ->
            val safe =
                leg.name
                    .replace('|', '/')
                    .replace(';', ',')
            String.format(Locale.US, "%s@%.2f", safe, leg.lengthM / 1000.0)
        }

    fun appendReportTokens(
        report: StringBuilder,
        turnsJson: String,
    ) {
        val ferries = ferryLegsFromTurns(turnsJson)
        val tunnels = tunnelLegsFromTurns(turnsJson)
        report.append("route_ferry_legs=${ferries.size}\n")
        report.append("route_ferry_fp=${formatNamedFp(ferries)}\n")
        report.append("route_uses_ferry=${ferries.isNotEmpty()}\n")
        report.append("route_tunnel_count=${tunnels.size}\n")
        report.append("route_tunnel_fp=${formatNamedFp(tunnels)}\n")
    }

    private fun namedLegsFromTurns(
        turnsJson: String,
        flagKey: String,
        ferryStyleLabel: Boolean,
    ): List<NamedLeg> {
        val arr = runCatching { JSONArray(turnsJson.ifBlank { "[]" }) }.getOrNull() ?: return emptyList()
        val out = ArrayList<NamedLeg>()
        var curName: String? = null
        var curM = 0.0

        fun flush() {
            val name = curName
            if (name != null && curM > 0.0) {
                out.add(NamedLeg(name, curM))
            }
            curName = null
            curM = 0.0
        }

        for (i in 0 until arr.length()) {
            val o = arr.optJSONObject(i) ?: continue
            if (!o.optBoolean(flagKey, false)) {
                flush()
                continue
            }
            val dist = o.optDouble("dist_m", 0.0).coerceAtLeast(0.0)
            val label = legLabel(o, ferryStyleLabel)
            when {
                curName == null -> {
                    curName = label
                    curM = dist
                }
                curName == label -> curM += dist
                else -> {
                    flush()
                    curName = label
                    curM = dist
                }
            }
        }
        flush()
        return out
    }

    private fun legLabel(
        o: org.json.JSONObject,
        ferryStyle: Boolean,
    ): String {
        val street = o.optString("street", "").trim()
        val ref = o.optString("ref", "").trim()
        val destination = o.optString("destination", "").trim()
        if (ferryStyle) {
            // Prefer an explicit A - B street, else destination harbour, else ref.
            if (street.isNotEmpty()) return street
            if (destination.isNotEmpty()) {
                return destination
                    .split(';')
                    .first()
                    .trim()
                    .ifEmpty { destination }
            }
            if (ref.isNotEmpty()) return ref
            return "unnamed"
        }
        if (street.isNotEmpty()) return street
        if (ref.isNotEmpty()) return ref
        return "unnamed"
    }
}
