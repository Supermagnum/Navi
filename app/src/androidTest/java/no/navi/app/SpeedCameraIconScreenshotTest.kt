package no.navi.app

import android.graphics.BitmapFactory
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiIconTheme
import uniffi.navi.rasterizeIconPng
import java.io.File

/**
 * Confirms the custom speed_camera icon ships in the lean Android pack and
 * rasterizes (not unknown.svg fallback).
 */
@RunWith(AndroidJUnit4::class)
class SpeedCameraIconScreenshotTest {
    @Test
    fun speedCameraIconRasterizesFromLeanPack() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val iconsDir =
            File(ctx.filesDir, "icons-speed-camera-test").also { dir ->
                dir.mkdirs()
                ctx.assets.open("icons/speed_camera.svg").use { input ->
                    File(dir, "speed_camera.svg").outputStream().use { output -> input.copyTo(output) }
                }
            }
        val lean = File(iconsDir, "speed_camera.svg")
        assertTrue("lean pack missing speed_camera.svg at ${lean.absolutePath}", lean.isFile)

        for (key in listOf("speed_camera", "speed-camera", "enforcement_maxspeed")) {
            val png =
                rasterizeIconPng(
                    key = key,
                    theme = FfiIconTheme.DAY,
                    width = 96u,
                    height = 96u,
                    bundledDir = iconsDir.absolutePath,
                )
            assertTrue("empty PNG for key=$key", png.isNotEmpty())
            val bmp = BitmapFactory.decodeByteArray(png, 0, png.size)
            assertTrue("decode failed for key=$key", bmp != null && bmp.width > 0)
            // Non-trivial alpha: not a blank unknown tile.
            var alphaSum = 0L
            val px = IntArray(bmp!!.width * bmp.height)
            bmp.getPixels(px, 0, bmp.width, 0, 0, bmp.width, bmp.height)
            for (c in px) {
                alphaSum += (c ushr 24) and 0xff
            }
            assertTrue("trivial alpha for key=$key (unknown fallback?)", alphaSum > 1000)
        }

        val out = File(ctx.cacheDir, "speed_camera_icon_confirm.png")
        val png =
            rasterizeIconPng(
                key = "speed_camera",
                theme = FfiIconTheme.DAY,
                width = 128u,
                height = 128u,
                bundledDir = iconsDir.absolutePath,
            )
        out.writeBytes(png)
        // Also pull to public path for host adb pull when present.
        runCatching {
            File("/sdcard/Download/speed_camera_icon_confirm.png").writeBytes(png)
        }
        assertTrue(out.isFile && out.length() > 100)
    }
}
