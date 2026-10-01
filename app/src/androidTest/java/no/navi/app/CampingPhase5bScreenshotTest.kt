package no.navi.app

import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertNotNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.TravelProfile
import java.io.File

@RunWith(AndroidJUnit4::class)
class CampingPhase5bScreenshotTest {
    @get:Rule
    val composeRule = createComposeRule()

    private fun capture(tag: String) {
        composeRule.waitForIdle()
        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertNotNull("screenshot null for $tag", shot)
        val outDir = File("/sdcard/Download/navi_camping_screenshots")
        outDir.mkdirs()
        val out = File(outDir, "$tag.png")
        out.outputStream().use { os ->
            shot!!.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, os)
        }
    }

    private fun showSheet(
        json: String,
        profile: TravelProfile,
        sessionDisable: String? = null,
    ) {
        val parsed = parseCampingSuggestResultJson(json)
        composeRule.setContent {
            MaterialTheme {
                CampingSuggestionSheet(
                    result = parsed,
                    profile = profile,
                    listDisclaimer = parsed.disclaimer,
                    sessionDisableMessage = sessionDisable,
                    onReEnableSession = {},
                    onClose = {},
                )
            }
        }
        composeRule.onNodeWithTag("camping_suggestion_sheet").assertIsDisplayed()
    }

    @Test
    fun screenshot_hikingJulyFireBan() {
        showSheet(CampingSuggestFixtures.hikingJulyFireBan(), TravelProfile.HIKING)
        composeRule.onNodeWithTag("camping_fire_text").assertIsDisplayed()
        capture("camping_hiking_july_fire")
    }

    @Test
    fun screenshot_hikingOctoberFire() {
        showSheet(CampingSuggestFixtures.hikingOctoberOutsideBan(), TravelProfile.HIKING)
        capture("camping_hiking_october_fire")
    }

    @Test
    fun screenshot_mobileHomeOnFoot() {
        showSheet(CampingSuggestFixtures.mobileHomeVehicleEmptyOnFoot(), TravelProfile.MOBILE_HOME)
        composeRule.onNodeWithTag("camping_section_vehicle").assertIsDisplayed()
        composeRule.onNodeWithTag("camping_section_on_foot").assertIsDisplayed()
        capture("camping_mobile_home_on_foot")
    }

    @Test
    fun screenshot_swedishTierA() {
        showSheet(CampingSuggestFixtures.swedishTierASafetyDefault(), TravelProfile.HIKING)
        capture("camping_sweden_tier_a")
    }

    @Test
    fun screenshot_svalbardDecline() {
        showSheet(CampingSuggestFixtures.svalbardDecline(), TravelProfile.HIKING)
        capture("camping_svalbard_decline")
    }

    @Test
    fun screenshot_sessionDisableBanner() {
        composeRule.setContent {
            MaterialTheme {
                CampingSessionDisableBanner(
                    message = "Fuel exhausted in wasm guest",
                    onReEnable = {},
                )
            }
        }
        composeRule.onNodeWithTag("camping_session_disable_banner").assertIsDisplayed()
        capture("camping_session_disable_banner")
    }
}
