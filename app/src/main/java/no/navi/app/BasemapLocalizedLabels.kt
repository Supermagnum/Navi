package no.navi.app

import android.util.Log
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.SymbolLayer
import org.maplibre.android.maps.Style
import java.util.Locale

/**
 * Rebuilds MapLibre symbol [text-field] coalesce chains so labels follow the
 * device locale (see docs/i18n/names.md). Refs stay as last-resort road labels.
 */
object BasemapLocalizedLabels {
    private const val TAG = "BasemapLocalizedLabels"

    /** Languages whose UI orthography is treated as Latin for fallback inserts. */
    private val NON_LATIN_LANGS =
        setOf(
            "ar", "fa", "ur", "he", "yi", "hi", "mr", "ne", "bn", "pa", "gu", "ta",
            "te", "kn", "ml", "si", "th", "lo", "my", "ka", "hy", "am", "ti", "zh",
            "ja", "ko", "ru", "uk", "be", "bg", "sr", "mk", "el",
        )

    /**
     * Ordered attribute keys for [Expression.coalesce], excluding the final
     * bare [name]/[ref] handled by [nameCoalesce].
     */
    fun preferredNameKeys(locale: Locale = Locale.getDefault()): List<String> {
        val lang = locale.language.lowercase(Locale.ROOT).ifBlank { "en" }
        val country = locale.country.lowercase(Locale.ROOT)
        val full =
            if (country.isNotBlank()) {
                "$lang-$country"
            } else {
                lang
            }
        val keys = mutableListOf<String>()
        keys += "name:$full"
        if (country.isNotBlank()) {
            keys += "name:$lang-${country.uppercase(Locale.ROOT)}"
        }
        keys += "name:$lang"
        when (lang) {
            "nb", "nn" -> keys += "name:no"
            "no" -> keys += "name:nb"
        }
        if (lang !in NON_LATIN_LANGS) {
            keys += listOf("int_name", "name:en", "name:latin")
        }
        return keys.distinct()
    }

    fun nameCoalesce(
        locale: Locale = Locale.getDefault(),
        includeRef: Boolean = false,
    ): Expression {
        val parts = preferredNameKeys(locale).map { Expression.get(it) }.toMutableList()
        parts += Expression.get("name")
        if (includeRef) {
            parts += Expression.get("ref")
        }
        return Expression.coalesce(*parts.toTypedArray())
    }

    /**
     * Patch every symbol layer that uses a simple `{name}` / get-name text field
     * so localized attrs win. Skips layers whose text-field is clearly not a
     * place/road name (housenumbers, contour elevations, etc.).
     */
    fun applyToStyle(
        style: Style,
        locale: Locale = Locale.getDefault(),
    ) {
        val nameExpr = nameCoalesce(locale, includeRef = false)
        val roadExpr = nameCoalesce(locale, includeRef = true)
        var patched = 0
        for (layer in style.layers) {
            if (layer !is SymbolLayer) continue
            val id = layer.id.lowercase(Locale.ROOT)
            if (id.contains("housenumber") || id.contains("contour") || id.contains("elevation")) {
                continue
            }
            val includeRef = id.contains("road") || id.contains("highway") || id.contains("street")
            try {
                layer.setProperties(
                    PropertyFactory.textField(if (includeRef) roadExpr else nameExpr),
                )
                patched++
            } catch (e: Exception) {
                Log.w(TAG, "skip layer ${layer.id}: ${e.message}")
            }
        }
        Log.i(TAG, "patched $patched symbol layers locale=$locale")
    }
}
