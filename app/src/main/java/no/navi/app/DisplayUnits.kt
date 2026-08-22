package no.navi.app

import java.util.Locale

/**
 * Display unit profile for distance, speed, and altitude.
 *
 * Internal values stay metric (metres, km/h). First-install default is inferred
 * once from SIM/network country ([UnitSystem.defaultForCountryIso]); after that
 * the stored preference is never re-inferred. Language selection is not handled
 * here — see `docs/plugins/i18n-translation-spec.md`.
 */
enum class UnitSystem {
    /** Metres / km, km/h, altitude in metres. */
    METRIC,

    /** US customary: feet under 1000 ft else miles; mph; altitude in feet. */
    IMPERIAL_US,

    /**
     * UK road convention: yards under 0.1 mi else miles; mph; altitude in metres.
     *
     * Metres for altitude is deliberate, not an incomplete imperial port. Feet
     * on a consumer altimeter is a US (aviation-influenced) quirk. UK driving
     * and hiking use metres for elevation — OS maps, hill/mountain heights
     * (Ben Nevis is 1,345 m). Do not switch this profile to feet to "match"
     * [IMPERIAL_US].
     */
    IMPERIAL_UK,
    ;

    val isMetric: Boolean get() = this == METRIC

    val persistId: String
        get() =
            when (this) {
                METRIC -> "metric"
                IMPERIAL_US -> "imperial_us"
                IMPERIAL_UK -> "imperial_uk"
            }

    companion object {
        fun fromPersistId(raw: String?): UnitSystem? =
            when (raw?.trim()?.lowercase()) {
                "metric" -> METRIC
                "imperial_us" -> IMPERIAL_US
                "imperial_uk" -> IMPERIAL_UK
                else -> null
            }

        fun fromPreferMetric(preferMetric: Boolean): UnitSystem = if (preferMetric) METRIC else IMPERIAL_US

        /**
         * First-install inference from ISO 3166-1 alpha-2 (SIM/network).
         * GB → UK profile; US/LR/MM → US customary; anything else (including
         * null/blank) → metric.
         */
        fun defaultForCountryIso(iso: String?): UnitSystem {
            val code = iso?.trim()?.uppercase(Locale.US).orEmpty()
            if (code.isEmpty()) return METRIC
            return when (code) {
                "GB", "UK" -> IMPERIAL_UK
                "US", "LR", "MM" -> IMPERIAL_US
                else -> METRIC
            }
        }

        fun looksLikeEmulator(
            fingerprint: String,
            product: String,
            model: String,
        ): Boolean {
            val fp = fingerprint.lowercase(Locale.US)
            val prod = product.lowercase(Locale.US)
            val mdl = model.lowercase(Locale.US)
            return fp.contains("generic") ||
                fp.contains("emulator") ||
                fp.contains("robolectric") ||
                prod.contains("sdk") ||
                prod.contains("emulator") ||
                mdl.contains("emulator") ||
                mdl.contains("android sdk")
        }
    }
}

/**
 * Display formatting for distance, speed, and altitude.
 *
 * Android-side twin of `format_distance_m` / `format_speed_kmh` /
 * `format_altitude_m` in `core/src/nav/mod.rs` for metric and US imperial.
 * UK miles/yards is Android-only (not in the boolean UniFFI distance helper).
 */
object DisplayUnits {
    private const val METERS_PER_MILE = 1609.344
    private const val FEET_PER_METER = 3.28084
    private const val METERS_PER_YARD = 0.9144
    private const val KM_PER_MILE = 1.609344
    private const val UK_YARD_BELOW_MI = 0.1

    fun formatDistanceM(
        distanceM: Double,
        preferMetric: Boolean,
    ): String = formatDistanceM(distanceM, UnitSystem.fromPreferMetric(preferMetric))

    fun formatDistanceM(
        distanceM: Double,
        unitSystem: UnitSystem,
    ): String {
        if (!distanceM.isFinite() || distanceM < 0.0) return ""
        return when (unitSystem) {
            UnitSystem.METRIC -> {
                if (distanceM < 1000.0) {
                    String.format(Locale.US, "%.0f m", distanceM)
                } else {
                    String.format(Locale.US, "%.1f km", distanceM / 1000.0)
                }
            }
            UnitSystem.IMPERIAL_US -> {
                val feet = distanceM * FEET_PER_METER
                if (feet < 1000.0) {
                    String.format(Locale.US, "%.0f ft", feet)
                } else {
                    String.format(Locale.US, "%.1f mi", distanceM / METERS_PER_MILE)
                }
            }
            UnitSystem.IMPERIAL_UK -> {
                val miles = distanceM / METERS_PER_MILE
                if (miles < UK_YARD_BELOW_MI) {
                    String.format(Locale.US, "%.0f yd", distanceM / METERS_PER_YARD)
                } else {
                    String.format(Locale.US, "%.1f mi", miles)
                }
            }
        }
    }

    fun formatSpeedKmh(
        speedKmh: Double,
        preferMetric: Boolean,
    ): String = formatSpeedKmh(speedKmh, UnitSystem.fromPreferMetric(preferMetric))

    fun formatSpeedKmh(
        speedKmh: Double,
        unitSystem: UnitSystem,
    ): String {
        if (!speedKmh.isFinite() || speedKmh < 0.0) return ""
        return if (unitSystem.isMetric) {
            String.format(Locale.US, "%.0f km/h", speedKmh)
        } else {
            String.format(Locale.US, "%.0f mph", speedKmh / KM_PER_MILE)
        }
    }

    fun formatSpeedNumber(
        speedKmh: Double,
        preferMetric: Boolean,
    ): String = formatSpeedNumber(speedKmh, UnitSystem.fromPreferMetric(preferMetric))

    fun formatSpeedNumber(
        speedKmh: Double,
        unitSystem: UnitSystem,
    ): String {
        if (!speedKmh.isFinite() || speedKmh < 0.0) return ""
        val value = if (unitSystem.isMetric) speedKmh else speedKmh / KM_PER_MILE
        return String.format(Locale.US, "%.0f", value)
    }

    fun speedUnit(preferMetric: Boolean): String = speedUnit(UnitSystem.fromPreferMetric(preferMetric))

    fun speedUnit(unitSystem: UnitSystem): String = if (unitSystem.isMetric) "km/h" else "mph"

    fun formatAltitudeM(
        altitudeM: Double,
        preferMetric: Boolean,
    ): String = formatAltitudeM(altitudeM, UnitSystem.fromPreferMetric(preferMetric))

    /**
     * Altitude: feet only for [UnitSystem.IMPERIAL_US]. Metric and UK stay in
     * metres — see [UnitSystem.IMPERIAL_UK].
     */
    fun formatAltitudeM(
        altitudeM: Double,
        unitSystem: UnitSystem,
    ): String {
        if (!altitudeM.isFinite()) return ""
        return if (unitSystem == UnitSystem.IMPERIAL_US) {
            String.format(Locale.US, "%.0f ft", altitudeM * FEET_PER_METER)
        } else {
            String.format(Locale.US, "%.0f m", altitudeM)
        }
    }

    fun formatDistanceKm(
        distanceKm: Double,
        preferMetric: Boolean,
    ): String = formatDistanceKm(distanceKm, UnitSystem.fromPreferMetric(preferMetric))

    fun formatDistanceKm(
        distanceKm: Double,
        unitSystem: UnitSystem,
    ): String {
        if (!distanceKm.isFinite() || distanceKm < 0.0) return ""
        return if (unitSystem.isMetric) {
            String.format(Locale.US, "%.1f km", distanceKm)
        } else {
            String.format(Locale.US, "%.1f mi", distanceKm / KM_PER_MILE)
        }
    }

    fun formatDistanceKmWhole(
        distanceKm: Double,
        preferMetric: Boolean,
    ): String = formatDistanceKmWhole(distanceKm, UnitSystem.fromPreferMetric(preferMetric))

    fun formatDistanceKmWhole(
        distanceKm: Double,
        unitSystem: UnitSystem,
    ): String {
        if (!distanceKm.isFinite() || distanceKm < 0.0) return ""
        return if (unitSystem.isMetric) {
            String.format(Locale.US, "%.0f km", distanceKm)
        } else {
            String.format(Locale.US, "%.0f mi", distanceKm / KM_PER_MILE)
        }
    }

    fun formatDistanceKmRange(
        startKm: Double,
        endKm: Double,
        preferMetric: Boolean,
    ): String = formatDistanceKmRange(startKm, endKm, UnitSystem.fromPreferMetric(preferMetric))

    fun formatDistanceKmRange(
        startKm: Double,
        endKm: Double,
        unitSystem: UnitSystem,
    ): String {
        if (unitSystem.isMetric) {
            return String.format(Locale.US, "%.0f–%.0f km", startKm, endKm)
        }
        return String.format(
            Locale.US,
            "%.0f–%.0f mi",
            startKm / KM_PER_MILE,
            endKm / KM_PER_MILE,
        )
    }

    fun formatRoutePlanned(
        distanceKm: Double,
        preferMetric: Boolean,
    ): String = formatRoutePlanned(distanceKm, UnitSystem.fromPreferMetric(preferMetric))

    fun formatRoutePlanned(
        distanceKm: Double,
        unitSystem: UnitSystem,
    ): String = "Route planned · ${formatDistanceKm(distanceKm, unitSystem)}"
}
