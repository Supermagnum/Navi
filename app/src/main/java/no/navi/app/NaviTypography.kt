package no.navi.app

import androidx.compose.material3.Typography
import androidx.compose.ui.text.PlatformTextStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontSynthesis
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.unit.sp

/**
 * One bundled face for every Compose string. Samsung One UI FlipFont remaps
 * [FontFamily.SansSerif] and paints doubled glyphs ("Frommy", "Planroute").
 * Liberation Sans Regular (SIL OFL 1.1, 401 KiB) is loaded from
 * `res/font/liberation_sans.ttf` so the system font cannot change spacing.
 */
val NaviFontFamily: FontFamily = FontFamily(Font(R.font.liberation_sans))

fun naviTypography(): Typography {
    val base = Typography()
    fun TextStyle.plain(): TextStyle =
        copy(
            letterSpacing = 0.sp,
            fontFamily = NaviFontFamily,
            fontSynthesis = FontSynthesis.None,
            platformStyle = PlatformTextStyle(includeFontPadding = false),
            lineHeightStyle =
                LineHeightStyle(
                    alignment = LineHeightStyle.Alignment.Center,
                    trim = LineHeightStyle.Trim.None,
                ),
        )
    return Typography(
        displayLarge = base.displayLarge.plain(),
        displayMedium = base.displayMedium.plain(),
        displaySmall = base.displaySmall.plain(),
        headlineLarge = base.headlineLarge.plain(),
        headlineMedium = base.headlineMedium.plain(),
        headlineSmall = base.headlineSmall.plain(),
        titleLarge = base.titleLarge.plain(),
        titleMedium = base.titleMedium.plain(),
        titleSmall = base.titleSmall.plain(),
        bodyLarge = base.bodyLarge.plain(),
        bodyMedium = base.bodyMedium.plain(),
        bodySmall = base.bodySmall.plain(),
        labelLarge = base.labelLarge.plain(),
        labelMedium = base.labelMedium.plain(),
        labelSmall = base.labelSmall.plain(),
    )
}

/** Chip / button labels: bundled face, no tracking. */
@androidx.compose.runtime.Composable
fun NaviText(
    text: String,
    modifier: androidx.compose.ui.Modifier = androidx.compose.ui.Modifier,
    style: TextStyle = androidx.compose.material3.LocalTextStyle.current,
) {
    androidx.compose.material3.Text(
        text = text,
        modifier = modifier,
        style =
            style.copy(
                letterSpacing = 0.sp,
                fontFamily = NaviFontFamily,
                fontSynthesis = FontSynthesis.None,
            ),
    )
}
