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
 * Rasterize representative Norwegian road-sign icons from the lean pack and write
 * PNGs for host `adb pull` (same pattern as [SpeedCameraIconScreenshotTest]).
 */
@RunWith(AndroidJUnit4::class)
class RoadSignIconScreenshotTest {
    private fun iconsDir(): File {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dir = File(ctx.filesDir, "icons-road-sign-screenshot")
        dir.mkdirs()
        copyAssetTree(ctx.assets, "icons", dir)
        return dir
    }

    private fun copyAssetTree(
        am: android.content.res.AssetManager,
        assetPath: String,
        destDir: File,
    ) {
        val names = am.list(assetPath) ?: return
        destDir.mkdirs()
        for (name in names) {
            val childAsset = "$assetPath/$name"
            val children = am.list(childAsset)
            if (children != null && children.isNotEmpty()) {
                copyAssetTree(am, childAsset, File(destDir, name))
            } else {
                runCatching {
                    am.open(childAsset).use { input ->
                        File(destDir, name).outputStream().use { output -> input.copyTo(output) }
                    }
                }
            }
        }
    }

    @Test
    fun roadSignCategoryIconsRasterizeFromLeanPack() {
        val iconsDir = iconsDir()
        val samples =
            listOf(
                "no_sign_100_1" to "road_sign_fareskilt_100_1.png",
                "no_sign_366" to "road_sign_speed_limit_366.png",
                "no_sign_640_10" to "road_sign_serviceskilt_640_10.png",
                "no_sign_755" to "road_sign_vegvisning_755.png",
            )
        for ((key, filename) in samples) {
            val png =
                rasterizeIconPng(
                    key = key,
                    theme = FfiIconTheme.DAY,
                    width = 128u,
                    height = 128u,
                    bundledDir = iconsDir.absolutePath,
                )
            assertTrue("empty PNG for $key", png.isNotEmpty())
            val bmp = BitmapFactory.decodeByteArray(png, 0, png.size)
            assertTrue("decode failed for $key", bmp != null && bmp.width > 0)
            var alphaSum = 0L
            val px = IntArray(bmp!!.width * bmp.height)
            bmp.getPixels(px, 0, bmp.width, 0, 0, bmp.width, bmp.height)
            for (c in px) {
                alphaSum += (c ushr 24) and 0xff
            }
            assertTrue("trivial alpha for $key", alphaSum > 1000)
            File(
                InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,
                filename,
            ).writeBytes(png)
            runCatching {
                File("/sdcard/Download/$filename").writeBytes(png)
            }
        }
    }
}
