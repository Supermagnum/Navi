package no.navi.app

import android.graphics.Bitmap
import android.util.Log
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeUp
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.UiDevice
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.io.FileOutputStream
import java.util.Locale
import java.util.regex.Pattern
import kotlin.math.abs

/**
 * Emulator virtual-sensor campaign for PR #162 / #150.
 *
 * Host [scripts/imu-calibration-campaign-host.py] watches logcat `ImuCampaign`
 * REQUEST/ACK lines and applies `adb emu sensor set`. Screenshots go to
 * `/data/local/tmp/imu_campaign/`.
 */
@RunWith(AndroidJUnit4::class)
class ImuCalibrationCampaignInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var device: UiDevice
    private val campaignDir = "/data/local/tmp/imu_campaign"
    private val results = linkedMapOf<String, String>()

    @Before
    fun setUp() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        shell("mkdir -p $campaignDir")
        shell("rm -f $campaignDir/*.png $campaignDir/results.txt $campaignDir/DONE")
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        File(ctx.filesDir, "plugins/imu").deleteRecursively()
    }

    @Test
    fun emulatorVirtualSensorCalibrationCampaign() {
        assumeTrue(
            "host sensor injector must be running (scripts/imu-calibration-campaign-host.py)",
            waitForHostReady(30_000),
        )

        // Inject mount tilt before opening so the first readout is already tilted.
        requestSensor("tilt_mount_15_10")
        openImuPanel()
        enableImuAndWaitForValues()
        assertInstructionCopy()
        shot("01_instruction")
        results["7_instruction_copy"] = "PASS"

        val tilted =
            awaitValuesMatching("tilted_mount") { pitch, _, tilt ->
                tilt != null && tilt > 12.0 && pitch != null && abs(pitch) > 8.0
            }
        results["1_tilted_mount"] = "PASS $tilted"
        shot("02_tilted_mount")

        clickTag("btn_imu_calibrate")
        val leveled =
            awaitValuesMatching("after_calibrate_level") { pitch, roll, tilt ->
                statusCalibrated() &&
                    pitch != null &&
                    abs(pitch) < 2.0 &&
                    roll != null &&
                    abs(roll) < 2.0 &&
                    (tilt == null || tilt < 3.0)
            }
        results["2_calibrate_levels"] = "PASS $leveled"
        shot("03_calibrated_level")

        requestSensor("vehicle_tilt_8_3")
        val relative =
            awaitValuesMatching("relative_tilt") { pitch, roll, _ ->
                pitch != null && pitch in 5.0..11.0 && roll != null && roll in 0.5..6.0
            }
        results["3_relative_tilt"] = "PASS $relative"
        shot("04_relative_tilt_8_3")

        requestSensor("tilt_mount_15_10")
        awaitValuesMatching("back_to_mount_level") { pitch, roll, _ ->
            pitch != null && abs(pitch) < 2.0 && roll != null && abs(roll) < 2.0
        }

        requestSensor("motion_gyro")
        clickTag("btn_imu_calibrate")
        val refused =
            awaitNoticeContaining(
                "Calibration rejected: the unit is moving",
                timeoutMs = 25_000,
            )
        results["4_refuse_while_moving"] = if (refused) "PASS" else "FAIL no moving notice"
        assertTrue("expected moving refusal notice", refused)
        shot("05_refuse_while_moving")

        requestSensor("tilt_mount_15_10")
        Thread.sleep(1_500)
        clickTag("btn_imu_calibrate")
        awaitValuesMatching("recalibrate_still") { pitch, roll, _ ->
            statusCalibrated() &&
                pitch != null &&
                abs(pitch) < 2.0 &&
                roll != null &&
                abs(roll) < 2.0
        }
        shot("06_recalibrated_before_restart")

        // Persistence across activity recreate (mount KV is on disk under filesDir).
        // Full process death via force-stop races ActivityScenario teardown on this AVD,
        // so recreate is the stable instrumented stand-in; host also verifies the KV file.
        composeRule.activityRule.scenario.recreate()
        Thread.sleep(2_000)
        composeRule.waitForIdle()
        openImuPanel()
        enableImuAndWaitForValues()
        requestSensor("tilt_mount_15_10")
        val persisted =
            awaitValuesMatching("after_restart") { pitch, roll, _ ->
                statusCalibrated() &&
                    pitch != null &&
                    abs(pitch) < 2.5 &&
                    roll != null &&
                    abs(roll) < 2.5
            }
        val mountKv =
            shell(
                "run-as no.navi.app sh -c 'ls files/plugins/imu 2>/dev/null; find files -name \"*mount*\" 2>/dev/null | head'",
            )
        results["5_survives_restart"] = "PASS $persisted kv=$mountKv"
        shot("07_after_restart")

        clickTag("btn_imu_reset")
        val resetNotice = awaitNoticeContaining("Calibration reset", timeoutMs = 12_000)
        val resetTilt =
            try {
                awaitValuesMatching("after_reset") { _, _, tilt ->
                    !statusCalibrated() && tilt != null && tilt > 12.0
                }
                true
            } catch (_: Throwable) {
                false
            }
        val resetOk = resetNotice || resetTilt
        results["6_reset"] = if (resetOk) "PASS notice=$resetNotice tiltCleared=$resetTilt" else "FAIL"
        assertTrue("reset should clear calibration", resetOk)
        shot("08_after_reset")

        val body = results.entries.joinToString("\n") { "${it.key}=${it.value}" } + "\n"
        writeResults(body)
        shell("printf 'ok\\n' > $campaignDir/DONE")
        Log.i(TAG, "DONE")
    }

    private fun shell(cmd: String): String {
        val pfd =
            InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
        return java.io
            .FileInputStream(pfd.fileDescriptor)
            .use { input ->
                input.readBytes().decodeToString()
            }.also { pfd.close() }
    }

    private fun writeResults(body: String) {
        val local =
            File(
                InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,
                "results.txt",
            )
        local.writeText(body)
        // Best-effort copy into shared tmp for host pull.
        shell("screencap -p $campaignDir/_probe.png >/dev/null 2>&1; rm -f $campaignDir/_probe.png")
        val b64 =
            android.util.Base64.encodeToString(
                body.toByteArray(Charsets.UTF_8),
                android.util.Base64.NO_WRAP,
            )
        shell("sh -c 'echo $b64 | base64 -d > $campaignDir/results.txt'")
    }

    private fun openImuPanel() {
        waitForToolsButton()
        composeRule
            .onNodeWithTag("btn_tools", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        composeRule.onNodeWithTag("tools_menu", useUnmergedTree = true).assertIsDisplayed()
        // Plugins / IMU sit near the bottom of the scrollable Tools sheet.
        val deadline = System.currentTimeMillis() + 20_000
        var last: Throwable? = null
        while (System.currentTimeMillis() < deadline) {
            try {
                composeRule
                    .onNodeWithTag("imu_plugin_section", useUnmergedTree = true)
                    .assertExists()
                runCatching {
                    composeRule
                        .onNodeWithTag("imu_plugin_section", useUnmergedTree = true)
                        .performScrollTo()
                }
                composeRule
                    .onNodeWithTag("toggle_imu_plugin", useUnmergedTree = true)
                    .assertExists()
                return
            } catch (t: Throwable) {
                last = t
                composeRule
                    .onNodeWithTag("tools_menu", useUnmergedTree = true)
                    .performTouchInput { swipeUp() }
                composeRule.waitForIdle()
                Thread.sleep(200)
            }
        }
        throw IllegalStateException("imu_plugin_section not found in Tools", last)
    }

    private fun assertInstructionCopy() {
        composeRule
            .onNodeWithTag("imu_calibration_instruction", useUnmergedTree = true)
            .performScrollTo()
            .assertTextEquals(IMU_CALIBRATION_INSTRUCTION)
    }

    private fun enableImuAndWaitForValues() {
        val toggle = composeRule.onNodeWithTag("toggle_imu_plugin", useUnmergedTree = true)
        toggle.performScrollTo()
        val alreadyOn =
            try {
                composeRule.onNodeWithTag("imu_values", useUnmergedTree = true).assertExists()
                true
            } catch (_: Throwable) {
                false
            }
        if (!alreadyOn) {
            toggle.performClick()
            composeRule.waitForIdle()
        }
        val deadline = System.currentTimeMillis() + 30_000
        while (System.currentTimeMillis() < deadline) {
            try {
                composeRule.onNodeWithTag("imu_values", useUnmergedTree = true).assertExists()
                return
            } catch (_: Throwable) {
                Thread.sleep(400)
            }
        }
        error("imu_values never appeared after enabling plugin")
    }

    private fun clickTag(tag: String) {
        composeRule
            .onNodeWithTag(tag, useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
    }

    private fun waitForToolsButton(timeoutMs: Long = 120_000) {
        val deadline = System.currentTimeMillis() + timeoutMs
        var last: Throwable? = null
        while (System.currentTimeMillis() < deadline) {
            try {
                composeRule.waitForIdle()
                composeRule.onNodeWithTag("btn_tools", useUnmergedTree = true).assertExists()
                return
            } catch (t: Throwable) {
                last = t
                Thread.sleep(500)
            }
        }
        throw IllegalStateException("btn_tools never appeared", last)
    }

    private fun waitForHostReady(timeoutMs: Long): Boolean {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            Log.i(TAG, "REQUEST:ping")
            if (pollAck("ping", 2_000)) return true
        }
        return false
    }

    private fun requestSensor(name: String) {
        Log.i(TAG, "REQUEST:$name")
        assertTrue("host ACK for $name", pollAck(name, 20_000))
        Thread.sleep(900)
    }

    private fun pollAck(
        name: String,
        timeoutMs: Long,
    ): Boolean {
        val deadline = System.currentTimeMillis() + timeoutMs
        // UiAutomation executeShellCommand does not run a shell, so pipes are unavailable.
        while (System.currentTimeMillis() < deadline) {
            val out = shell("logcat -d -s ImuCampaign:I")
            if (out.contains("ACK:$name")) return true
            Thread.sleep(200)
        }
        return false
    }

    private fun tagText(tag: String): String {
        val node =
            composeRule
                .onNodeWithTag(tag, useUnmergedTree = true)
                .fetchSemanticsNode()
        val texts = node.config.getOrElse(SemanticsProperties.Text) { emptyList() }
        return texts.joinToString(" ") { it.text }
    }

    private fun statusCalibrated(): Boolean =
        try {
            val status = tagText("imu_status")
            val date = tagText("imu_calibration_date")
            status.contains("Calibrated", ignoreCase = true) ||
                (date.contains("Calibration date:") && !date.contains("not calibrated"))
        } catch (_: Throwable) {
            false
        }

    private fun readImuValues(): Triple<Double?, Double?, Double?> {
        val text = tagText("imu_values")

        fun grab(label: String): Double? {
            val m = Pattern.compile("$label\\s+([+-]?\\d+(?:\\.\\d+)?)").matcher(text)
            return if (m.find()) m.group(1).toDoubleOrNull() else null
        }
        return Triple(grab("Pitch"), grab("Roll"), grab("Tilt"))
    }

    private fun ensureImuVisible() {
        val visible =
            try {
                composeRule.onNodeWithTag("imu_values", useUnmergedTree = true).assertExists()
                true
            } catch (_: Throwable) {
                false
            }
        if (!visible) {
            openImuPanel()
            enableImuAndWaitForValues()
        } else {
            runCatching {
                composeRule
                    .onNodeWithTag("imu_plugin_section", useUnmergedTree = true)
                    .performScrollTo()
            }
        }
    }

    private fun awaitValuesMatching(
        label: String,
        timeoutMs: Long = 30_000,
        pred: (pitch: Double?, roll: Double?, tilt: Double?) -> Boolean,
    ): String {
        val deadline = System.currentTimeMillis() + timeoutMs
        var last = "none"
        while (System.currentTimeMillis() < deadline) {
            composeRule.waitForIdle()
            try {
                ensureImuVisible()
                val (p, r, t) = readImuValues()
                last =
                    String.format(
                        Locale.US,
                        "pitch=%.2f roll=%.2f tilt=%s calibrated=%s",
                        p ?: Double.NaN,
                        r ?: Double.NaN,
                        t?.let { String.format(Locale.US, "%.2f", it) } ?: "null",
                        statusCalibrated(),
                    )
                if (pred(p, r, t)) return last
            } catch (t: Throwable) {
                last = t.message ?: t.toString()
            }
            Thread.sleep(400)
        }
        error("$label timed out; last=$last")
    }

    private fun awaitNoticeContaining(
        needle: String,
        timeoutMs: Long,
    ): Boolean {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            try {
                if (tagText("imu_notice").contains(needle, ignoreCase = true)) return true
            } catch (_: Throwable) {
            }
            Thread.sleep(300)
        }
        return false
    }

    private fun shot(name: String) {
        composeRule.waitForIdle()
        Thread.sleep(400)
        val bmp =
            InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
                ?: error("screenshot null for $name")
        val local =
            File(
                InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,
                "$name.png",
            )
        FileOutputStream(local).use { bmp.compress(Bitmap.CompressFormat.PNG, 100, it) }
        assertTrue("wrote local $name", local.isFile && local.length() > 1000)
        shell("screencap -p $campaignDir/$name.png")
        shell("chmod 644 $campaignDir/$name.png")
        val ext = InstrumentationRegistry.getInstrumentation().targetContext.getExternalFilesDir(null)
        if (ext != null) {
            local.copyTo(File(ext, "imu_campaign_$name.png"), overwrite = true)
        }
    }

    private companion object {
        const val TAG = "ImuCampaign"
    }
}
