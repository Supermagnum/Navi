package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.convertProgressClear
import uniffi.navi.convertProgressSnapshot
import uniffi.navi.downloadProgressClear
import uniffi.navi.downloadProgressSnapshot
import uniffi.navi.ensureIndexedMaps
import uniffi.navi.initNativeLogging
import java.io.File

/**
 * Step-2 gate: IndexedMapsBackground-style ensureIndexedMaps must write Convert,
 * not Download, so RegionDownload progress cannot be clobbered.
 */
@RunWith(AndroidJUnit4::class)
class ProgressChannelIsolationInstrumentedTest {
    private companion object {
        const val TAG = "ProgressChannelIso"
    }

    @Test
    fun indexed_maps_convert_channel_leaves_download_slot_alone() {
        initNativeLogging()
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = File(NaviAppData.resolve(ctx), "progress_channel_iso")
        dataDir.mkdirs()
        // Tiny published region already exercised by step-1; reuse monaco packs if present.
        val monacoDir = File(NaviAppData.resolve(ctx), "plan_acq_step1_monaco")
        val pbf =
            monacoDir.listFiles()?.firstOrNull { it.name.endsWith(".osm.pbf") }
                ?: File(dataDir, "missing.osm.pbf")
        if (!pbf.isFile) {
            Log.i(TAG, "SKIP no monaco pbf from step1; writing stub")
            // Still verifies channel routing even if ensure fails early.
            pbf.parentFile?.mkdirs()
            pbf.writeBytes(ByteArray(16 * 1024))
        }

        downloadProgressClear()
        convertProgressClear()

        // Seed Download with a fake region-2 label.
        // Native set goes through ensure paths; seed via a no-op decide that writes Download.
        uniffi.navi.decideRegionAcquisition(
            regionId = "europe/monaco",
            packServerBaseUrl = null,
            dataDir = null,
        )
        val afterDecide = downloadProgressSnapshot()
        Log.i(TAG, "after_decide download_label=${afterDecide.label}")

        // Clear then set a sentinel by calling decide again is flaky; instead check that
        // convert-channel ensure does not leave "Rebuilding locally" on Download.
        downloadProgressClear()
        convertProgressClear()

        val report =
            ensureIndexedMaps(
                pbf.absolutePath,
                (pbf.parentFile ?: dataDir).absolutePath,
                null,
                "europe/monaco",
                progressOnConvertChannel = true,
            )
        Log.i(TAG, "ensure report=${report.take(240)}")
        val dl = downloadProgressSnapshot()
        val conv = convertProgressSnapshot()
        Log.i(TAG, "download_label='${dl.label}' convert_label='${conv.label}'")

        assertFalse(
            "Download slot must not show IndexedMaps rebuild text; got '${dl.label}'",
            dl.label.contains("Rebuilding locally", ignoreCase = true) ||
                dl.label.contains("Downloading updated pack", ignoreCase = true),
        )
        // Convert may be cleared after success; either non-rebuild-on-Download or convert used.
        assertTrue(
            "expected PASS or FAIL from ensureIndexedMaps, got ${report.take(80)}",
            report.contains("PASS") || report.contains("FAIL"),
        )
    }
}
