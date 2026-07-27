package no.navi.app

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Current-street bottom HUD + Norwegian special-character rendering.
 *
 * Uses real Østlandet name strings (Mjøsvegen / Trollåsveien / Ævongsli) that
 * already appear in the place-index fixtures — not invented placeholders.
 */
@RunWith(AndroidJUnit4::class)
class CurrentStreetInstrumentedTest {
    companion object {
        @JvmStatic
        @BeforeClass
        fun beforeClass() {
            val pkg = InstrumentationRegistry.getInstrumentation().targetContext.packageName
            runCatching {
                InstrumentationRegistry
                    .getInstrumentation()
                    .uiAutomation
                    .grantRuntimePermission(pkg, android.Manifest.permission.ACCESS_FINE_LOCATION)
            }
            NaviMapTestHooks.hideUiChrome = false
            NaviMapTestHooks.hideSearchChrome = true
        }
    }

    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    @Before
    fun resetHooks() {
        NaviMapTestHooks.pendingCurrentStreet = null
        NaviMapTestHooks.lastCurrentStreet = null
    }

    @Test
    fun formatCurrentRoadLabel_preservesNorwegianCharsAndClassFallback() {
        assertEquals("Mjøsvegen", formatCurrentRoadLabel("Mjøsvegen", "tertiary"))
        assertTrue(formatCurrentRoadLabel("Mjøsvegen", null).contains('ø'))
        assertTrue(formatCurrentRoadLabel("Trollåsveien", null).contains('å'))
        assertTrue(formatCurrentRoadLabel("Ævongsli", "residential").contains('Æ'))
        assertEquals("Service road", formatCurrentRoadLabel(null, "service"))
        assertEquals("Path", formatCurrentRoadLabel("  ", "path"))
        assertEquals("E6", formatCurrentRoadLabel("E6", "trunk"))
    }

    @Test
    fun bottomHud_showsCurrentlyOn_withOslashARingAndAsh() {
        // Real fixture names from docs/unicode-road-names.md / place index.
        val cases =
            listOf(
                "Mjøsvegen" to 'ø',
                "Trollåsveien" to 'å',
                "Ævongsli" to 'Æ',
            )
        for ((name, ch) in cases) {
            NaviMapTestHooks.pendingCurrentStreet = name
            composeRule.waitUntil(timeoutMillis = 8_000) {
                NaviMapTestHooks.lastCurrentStreet == name
            }
            composeRule
                .onNodeWithTag("hud_current_street", useUnmergedTree = true)
                .assertIsDisplayed()
                .assertTextContains("Currently on $name", substring = true)
            assertTrue(
                "expected '$ch' in HUD label for $name",
                NaviMapTestHooks.lastCurrentStreet.orEmpty().contains(ch),
            )
        }
        // Screenshot evidence for å/æ/ø (last case Æ; re-show ø for gallery).
        NaviMapTestHooks.pendingCurrentStreet = "Mjøsvegen"
        composeRule.waitUntil(timeoutMillis = 8_000) {
            NaviMapTestHooks.lastCurrentStreet == "Mjøsvegen"
        }
        composeRule.waitForIdle()
        Thread.sleep(500)
        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertTrue("UiAutomation screenshot null", shot != null)
        val cache = File(composeRule.activity.cacheDir, "hud_current_street_mjosevegen.png")
        java.io.FileOutputStream(cache).use { out ->
            shot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, out)
        }
        val ua = InstrumentationRegistry.getInstrumentation().uiAutomation
        ua
            .executeShellCommand("screencap -p /data/local/tmp/hud_current_street_mjosevegen.png")
            .close()
        ua
            .executeShellCommand("chmod 644 /data/local/tmp/hud_current_street_mjosevegen.png")
            .close()
        Thread.sleep(300)
        // Also inject å and æ cases briefly and capture combined evidence names.
        for ((name, file) in listOf(
            "Trollåsveien" to "hud_current_street_trollaas.png",
            "Ævongsli" to "hud_current_street_aevongsli.png",
        )) {
            NaviMapTestHooks.pendingCurrentStreet = name
            composeRule.waitUntil(timeoutMillis = 8_000) {
                NaviMapTestHooks.lastCurrentStreet == name
            }
            composeRule.waitForIdle()
            Thread.sleep(300)
            ua.executeShellCommand("screencap -p /data/local/tmp/$file").close()
            ua.executeShellCommand("chmod 644 /data/local/tmp/$file").close()
            Thread.sleep(200)
        }
    }
}
