package no.navi.app

import android.graphics.Bitmap
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.concurrent.TimeUnit

/**
 * Manual rotate snap-back toggle: default on returns to mode bearing; off keeps
 * the manual bearing until an explicit Compass/Travel/N-up selection.
 */
@RunWith(AndroidJUnit4::class)
class RotationSnapBackInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

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
            runCatching {
                InstrumentationRegistry
                    .getInstrumentation()
                    .uiAutomation
                    .grantRuntimePermission(pkg, android.Manifest.permission.ACCESS_COARSE_LOCATION)
            }
            NaviMapTestHooks.hideUiChrome = false
            NaviMapTestHooks.hideSearchChrome = true
        }
    }

    @Before
    fun setUp() {
        composeRule.waitForIdle()
        Thread.sleep(200)
    }

    @Test
    fun snapBack_on_and_off_across_rotation_modes() {
        val targetCtx = InstrumentationRegistry.getInstrumentation().targetContext
        val shotDir = File(targetCtx.filesDir, "rotation_snap_shots").also { it.mkdirs() }
        val externalDir =
            File(targetCtx.getExternalFilesDir(null), "rotation_snap_shots").also { it.mkdirs() }

        waitUntil(45_000) {
            NaviMapTestHooks.styleReady ||
                NaviMapTestHooks.lastReportedLayerCount >= 1 ||
                NaviMapTestHooks.lastCameraZoom > 0.0
        }

        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            NaviMapTestHooks.requestSnapRotationBack = true
            NaviMapTestHooks.magneticHeadingDeg = 40.0
            NaviMapTestHooks.gpsBearingDeg = 80.0
        }
        Thread.sleep(500)

        for ((mode, expected) in listOf(
            MapRotationMode.NorthUp to 0.0,
            MapRotationMode.Compass to 40.0,
            MapRotationMode.DirectionOfTravel to 80.0,
        )) {
            InstrumentationRegistry.getInstrumentation().runOnMainSync {
                NaviMapTestHooks.requestRotationMode = mode
            }
            waitUntil(8_000) {
                NaviMapTestHooks.lastRotationMode == mode &&
                    kotlin.math.abs(NaviMapTestHooks.lastCameraBearing - expected) <= 1.0
            }
            shot(shotDir, externalDir, "before_${mode.name.lowercase()}.png")

            InstrumentationRegistry.getInstrumentation().runOnMainSync {
                NaviMapTestHooks.requestSimulateManualRotateDeg = 150.0
            }
            waitUntil(5_000) {
                NaviMapTestHooks.manualRotationOverrideActive &&
                    kotlin.math.abs(NaviMapTestHooks.lastCameraBearing - 150.0) <= 1.0
            }
            // Give MapLibre a frame to composite the rotated camera before capture.
            Thread.sleep(1_200)
            shot(shotDir, externalDir, "during_${mode.name.lowercase()}.png")

            waitUntil(5_000) {
                !NaviMapTestHooks.manualRotationOverrideActive &&
                    kotlin.math.abs(NaviMapTestHooks.lastCameraBearing - expected) <= 1.0
            }
            assertEquals(expected, NaviMapTestHooks.lastCameraBearing, 1.0)
            Thread.sleep(1_200)
            shot(shotDir, externalDir, "after_snap_${mode.name.lowercase()}.png")
        }

        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            NaviMapTestHooks.requestSnapRotationBack = false
            NaviMapTestHooks.requestRotationMode = MapRotationMode.NorthUp
        }
        waitUntil(8_000) {
            !NaviMapTestHooks.lastSnapRotationBack &&
                kotlin.math.abs(NaviMapTestHooks.lastCameraBearing - 0.0) <= 1.0
        }
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            NaviMapTestHooks.requestSimulateManualRotateDeg = 120.0
        }
        waitUntil(5_000) { NaviMapTestHooks.manualRotationOverrideActive }
        Thread.sleep(1_500)
        assertTrue(
            "snap-off must keep manual bearing",
            kotlin.math.abs(NaviMapTestHooks.lastCameraBearing - 120.0) <= 1.0,
        )
        shot(shotDir, externalDir, "snap_off_stays.png")

        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            NaviMapTestHooks.requestRotationMode = MapRotationMode.Compass
        }
        waitUntil(8_000) {
            kotlin.math.abs(NaviMapTestHooks.lastCameraBearing - 40.0) <= 1.0
        }
        assertFalse(NaviMapTestHooks.manualRotationOverrideActive)
        shot(shotDir, externalDir, "mode_chip_overrides_sticky.png")
    }

    private fun waitUntil(
        timeoutMs: Long,
        pred: () -> Boolean,
    ) {
        val deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(timeoutMs)
        while (System.nanoTime() < deadline) {
            if (pred()) return
            Thread.sleep(100)
        }
        assertTrue("timeout waiting", pred())
    }

    private fun shot(
        dir: File,
        externalDir: File,
        name: String,
    ) {
        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertNotNull(shot)
        val out = File(dir, name)
        out.outputStream().use {
            shot!!.compress(Bitmap.CompressFormat.PNG, 100, it)
        }
        runCatching { out.copyTo(File(externalDir, name), overwrite = true) }
        // Host-pullable path without requiring root (`su` is unavailable on this device).
        runCatching {
            val pfd =
                InstrumentationRegistry
                    .getInstrumentation()
                    .uiAutomation
                    .executeShellCommand("cp ${out.absolutePath} /data/local/tmp/$name")
            java.io.FileInputStream(pfd.fileDescriptor).use { input ->
                val buf = ByteArray(4096)
                while (input.read(buf) >= 0) {
                }
            }
        }
    }
}
