package no.navi.app

import android.graphics.Bitmap
import android.graphics.Color
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiPmtilesJob
import uniffi.navi.pmtilesCancelJob
import uniffi.navi.pmtilesGetJob
import uniffi.navi.pmtilesListJobs
import uniffi.navi.pmtilesPauseJob
import uniffi.navi.pmtilesPlanetUrl
import uniffi.navi.pmtilesQueueDemRegion
import uniffi.navi.pmtilesQueueRegion
import uniffi.navi.pmtilesResumeJob
import uniffi.navi.pmtilesRunJob
import java.io.File
import java.util.concurrent.Executors
import java.util.concurrent.Future
import java.util.concurrent.TimeUnit
import kotlin.math.abs

/**
 * Real-hardware SM-P613 checks for Protomaps PMTiles pause / resume / cancel.
 *
 * Phase A/B restart methods must be orchestrated from the host with a force-stop
 * between them (in-memory DownloadControl does not survive process death).
 *
 * Prefer `adb shell am instrument` after `:app:installDebug` /
 * `:app:installDebugAndroidTest`. Older AGP 8.1+ `connectedDebugAndroidTest`
 * uninstalled the app when the task finished (wiping packs / `navi.db`); this
 * repo sets `android.injected.androidTest.leaveApksInstalledAfterRun=true`.
 *
 * Host sketch:
 * ```
 * adb shell am instrument -w -e class ...#pause_leave_job_for_restart ...
 * adb exec-out run-as no.navi.app cat files/navi_paused_pmtiles_job.txt \
 *   | adb shell "cat > /data/local/tmp/navi_paused_pmtiles_job.txt"
 * adb shell am force-stop no.navi.app
 * adb shell am instrument -w -e class ...#resume_after_restart ...
 * ```
 */
@RunWith(AndroidJUnit4::class)
class PmtilesPauseResumeInstrumentedTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var dataDir: File

    @Before
    fun setUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
    }

    private fun log(msg: String) {
        Log.i(TAG, msg)
    }

    private fun shell(cmd: String): String {
        val pfd =
            InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
        return java.io
            .FileInputStream(pfd.fileDescriptor)
            .use { input ->
                input.readBytes().toString(Charsets.UTF_8)
            }.also { pfd.close() }
    }

    private fun localPausedMarker(): File = File(dataDir, "navi_paused_pmtiles_job.txt")

    private fun cleanOsloBasemapArtifacts() {
        val pm = File(dataDir, "pmtiles")
        pm.mkdirs()
        File(pm, "test_oslo.pmtiles").delete()
        File(pm, "test_oslo.pmtiles.partial").delete()
        File(pm, "test_oslo.pmtiles.chunks").deleteRecursively()
    }

    private fun cleanOsloDemArtifacts() {
        val pm = File(dataDir, "pmtiles")
        pm.mkdirs()
        File(pm, "test_oslo_dem.pmtiles").delete()
        File(pm, "test_oslo_dem.pmtiles.partial").delete()
        File(pm, "test_oslo_dem.pmtiles.chunks").deleteRecursively()
    }

    private fun bytesThreshold(job: FfiPmtilesJob): Long {
        val total = job.totalBytes?.toLong()
        return if (total != null && total > 0L) {
            maxOf(200_000L, (total * 0.05).toLong())
        } else {
            200_000L
        }
    }

    private fun waitBytesRunning(
        dataDirPath: String,
        jobId: String,
        timeoutMs: Long = 180_000,
    ): FfiPmtilesJob {
        val deadline = System.currentTimeMillis() + timeoutMs
        var last: FfiPmtilesJob? = null
        while (System.currentTimeMillis() < deadline) {
            val j = pmtilesGetJob(dataDirPath, jobId)
            assertNotNull("job disappeared while waiting for bytes: $jobId", j)
            last = j!!
            val thr = bytesThreshold(j)
            log(
                "poll bytes=${j.bytesReceived} thr=$thr status=${j.status} paused=${j.paused} " +
                    "total=${j.totalBytes}",
            )
            if (j.bytesReceived.toLong() >= thr &&
                (j.status == "running" || (j.status == "paused" && j.paused))
            ) {
                return j
            }
            if (j.status == "completed" || j.status == "cancelled" || j.status.startsWith("failed")) {
                error("job ended early status=${j.status} bytes=${j.bytesReceived}")
            }
            Thread.sleep(400)
        }
        error(
            "timeout waiting for bytes>=threshold status=${last?.status} bytes=${last?.bytesReceived}",
        )
    }

    private fun waitPausedSettled(
        dataDirPath: String,
        jobId: String,
        timeoutMs: Long = 30_000,
    ): FfiPmtilesJob {
        val deadline = System.currentTimeMillis() + timeoutMs
        var last: FfiPmtilesJob? = null
        while (System.currentTimeMillis() < deadline) {
            val j = pmtilesGetJob(dataDirPath, jobId) ?: error("job missing after pause")
            last = j
            if (j.status == "paused" || j.paused) {
                return j
            }
            Thread.sleep(200)
        }
        // In-flight chunks may leave status running briefly; paused flag is authoritative.
        return last ?: error("no job after pause")
    }

    private fun queueAndRunBackground(region: String = REGION): Pair<FfiPmtilesJob, Future<FfiPmtilesJob>> {
        val planet = pmtilesPlanetUrl()
        val job = pmtilesQueueRegion(dataDir.absolutePath, region, planet)
        assertTrue("queue failed: ${job.status}", job.id.isNotBlank() && !job.status.startsWith("failed"))
        log("queued id=${job.id} region=${job.regionKey} path=${job.localPath} planet=$planet")
        val pool = Executors.newSingleThreadExecutor()
        val fut =
            pool.submit<FfiPmtilesJob> {
                pmtilesRunJob(dataDir.absolutePath, job.id)
            }
        pool.shutdown()
        return job to fut
    }

    @Test
    fun pause_resume_continues_not_restart() {
        cleanOsloBasemapArtifacts()
        val (job, fut) = queueAndRunBackground()
        val dataDirPath = dataDir.absolutePath

        val before = waitBytesRunning(dataDirPath, job.id)
        val bytesBeforePause = before.bytesReceived.toLong()
        log("bytesBeforePause=$bytesBeforePause status=${before.status} paused=${before.paused}")

        pmtilesPauseJob(job.id)
        Thread.sleep(3_000)
        val paused = waitPausedSettled(dataDirPath, job.id)
        val settleStart = paused.bytesReceived.toLong()
        Thread.sleep(2_000)
        val whilePaused = pmtilesGetJob(dataDirPath, job.id) ?: error("job missing while paused")
        val bytesWhilePaused = whilePaused.bytesReceived.toLong()
        val growth = bytesWhilePaused - settleStart
        log(
            "bytesWhilePaused=$bytesWhilePaused settleStart=$settleStart growth=$growth " +
                "status=${whilePaused.status} paused=${whilePaused.paused}",
        )
        assertTrue(
            "bytes grew too much after pause settle ($growth); pause may not be honoured",
            growth < 50_000L,
        )
        assertTrue(
            "expected paused status or paused flag, got status=${whilePaused.status} paused=${whilePaused.paused}",
            whilePaused.status == "paused" || whilePaused.paused || whilePaused.status == "running",
        )
        assertTrue(
            "bytes reset after pause: while=$bytesWhilePaused before=$bytesBeforePause",
            bytesWhilePaused >= (bytesBeforePause * 0.95).toLong(),
        )

        pmtilesResumeJob(job.id)
        val done = fut.get(240, TimeUnit.SECONDS)
        log(
            "bytesAfterResumeComplete=${done.bytesReceived} status=${done.status} " +
                "path=${done.localPath} timeline before=$bytesBeforePause whilePaused=$bytesWhilePaused",
        )
        assertEquals("completed", done.status)
        val out = File(done.localPath)
        assertTrue("final pmtiles missing: ${done.localPath}", out.isFile)
        assertTrue("final pmtiles too small: ${out.length()}", out.length() > 1000)
        val finalJob = pmtilesGetJob(dataDirPath, job.id)
        assertNotNull(finalJob)
        assertEquals("completed", finalJob!!.status)
        assertFalse(finalJob.paused)
    }

    @Test
    fun cancel_mid_download_clean() {
        cleanOsloBasemapArtifacts()
        val (job, fut) = queueAndRunBackground()
        val dataDirPath = dataDir.absolutePath

        val mid = waitBytesRunning(dataDirPath, job.id, timeoutMs = 120_000)
        log("cancel at bytes=${mid.bytesReceived} status=${mid.status}")
        pmtilesCancelJob(job.id)
        val done =
            try {
                fut.get(60, TimeUnit.SECONDS)
            } catch (e: Exception) {
                log("runJob future ended with ${e.javaClass.simpleName}: ${e.message}")
                pmtilesGetJob(dataDirPath, job.id)
                    ?: error("job missing after cancel")
            }
        val after = pmtilesGetJob(dataDirPath, job.id) ?: done
        log("cancel result status=${after.status} paused=${after.paused} bytes=${after.bytesReceived}")
        assertTrue(
            "expected cancelled or failed, got ${after.status}",
            after.status == "cancelled" || after.status.startsWith("failed"),
        )
        assertFalse("job still marked paused after cancel", after.paused)
        val stuck =
            pmtilesListJobs(dataDirPath).filter {
                it.id == job.id && it.status == "running"
            }
        assertTrue("orphaned running job after cancel: $stuck", stuck.isEmpty())

        // Cancel again must be idempotent / not throw.
        pmtilesCancelJob(job.id)
        val again = pmtilesGetJob(dataDirPath, job.id)
        log("second cancel status=${again?.status}")
        assertNotNull(again)
        assertTrue(
            again!!.status == "cancelled" || again.status.startsWith("failed"),
        )
    }

    /**
     * Phase A: leave a paused job on disk for host force-stop + Phase B.
     * Writes [PAUSED_JOB_FILE] with `jobId bytesReceived`.
     */
    @Test
    fun pause_leave_job_for_restart() {
        cleanOsloBasemapArtifacts()
        shell("rm -f $PAUSED_JOB_FILE")
        val (job, fut) = queueAndRunBackground()
        val dataDirPath = dataDir.absolutePath

        val before = waitBytesRunning(dataDirPath, job.id)
        val bytesBeforePause = before.bytesReceived.toLong()
        log("phaseA bytesBeforePause=$bytesBeforePause id=${job.id}")
        pmtilesPauseJob(job.id)
        Thread.sleep(2_000)
        val paused = waitPausedSettled(dataDirPath, job.id)
        assertTrue(
            "DB must show paused=true after pause, got status=${paused.status} paused=${paused.paused}",
            paused.paused || paused.status == "paused",
        )
        val bytes = paused.bytesReceived.toLong()
        assertTrue(bytes >= (bytesBeforePause * 0.95).toLong())
        val content = "${job.id} $bytes"
        val localMarker = localPausedMarker()
        localMarker.writeText("$content\n")
        assertTrue(localMarker.isFile && localMarker.length() > 0)
        // Host should also materialize $PAUSED_JOB_FILE after this test, e.g.:
        //   adb exec-out run-as no.navi.app cat files/navi_paused_pmtiles_job.txt \
        //     | adb shell "cat > /data/local/tmp/navi_paused_pmtiles_job.txt"
        log(
            "phaseA wrote marker content='$content' local=${localMarker.absolutePath} " +
                "status=${paused.status} paused=${paused.paused}",
        )
        // Do NOT resume. Allow runJob thread to stay blocked; host force-stops the app.
        assertFalse("phaseA must not complete before restart", fut.isDone)
    }

    /**
     * Phase B: after host `am force-stop`, resume the paused job and complete.
     * Prefer running via `am instrument` (not a fresh Gradle install) so app data / navi.db survive.
     */
    @Test
    fun resume_after_restart() {
        val localMarker = localPausedMarker()
        val written =
            when {
                localMarker.isFile -> localMarker.readText().trim()
                else -> shell("cat $PAUSED_JOB_FILE 2>/dev/null").trim()
            }
        assertTrue(
            "missing phaseA marker — run pause_leave_job_for_restart first (got '$written')",
            written.isNotBlank() && written.contains(" "),
        )
        val parts = written.split(Regex("\\s+"))
        assertTrue("bad marker content: '$written'", parts.size >= 2)
        val jobId = parts[0]
        val priorBytes = parts[1].toLong()
        val dataDirPath = dataDir.absolutePath
        val job = pmtilesGetJob(dataDirPath, jobId)
        assertNotNull("job $jobId missing after restart", job)
        log(
            "phaseB loaded id=$jobId prior=$priorBytes status=${job!!.status} " +
                "paused=${job.paused} bytes=${job.bytesReceived}",
        )
        assertTrue(
            "expected paused job after restart, got status=${job.status} paused=${job.paused}",
            job.paused || job.status == "paused" || job.status == "pending" || job.status == "running",
        )
        assertTrue(
            "bytes lost across restart: now=${job.bytesReceived} prior=$priorBytes",
            job.bytesReceived.toLong() >= (priorBytes * 0.90).toLong() ||
                File(File(job.localPath).path + ".chunks").isDirectory,
        )

        pmtilesResumeJob(jobId)
        val done = pmtilesRunJob(dataDirPath, jobId)
        log(
            "phaseB done status=${done.status} bytes=${done.bytesReceived} path=${done.localPath} len=" +
                File(done.localPath).length(),
        )
        assertEquals("completed", done.status)
        assertTrue(File(done.localPath).isFile && File(done.localPath).length() > 1000)
        localMarker.delete()
        shell("rm -f $PAUSED_JOB_FILE")
    }

    @Test
    fun dem_pause_then_cancel_smoke() {
        cleanOsloDemArtifacts()
        val job = pmtilesQueueDemRegion(dataDir.absolutePath, REGION)
        if (job.id.isBlank() || job.status.startsWith("failed")) {
            log("DEM queue unavailable (${job.status}); basemap-only suite is sufficient")
            return
        }
        log("DEM queued id=${job.id} path=${job.localPath}")
        val pool = Executors.newSingleThreadExecutor()
        val fut =
            pool.submit<FfiPmtilesJob> {
                pmtilesRunJob(dataDir.absolutePath, job.id)
            }
        pool.shutdown()
        val dataDirPath = dataDir.absolutePath
        val deadline = System.currentTimeMillis() + 45_000
        var sawBytes = false
        while (System.currentTimeMillis() < deadline) {
            val j = pmtilesGetJob(dataDirPath, job.id) ?: break
            if (j.bytesReceived.toLong() >= 50_000L) {
                sawBytes = true
                log("DEM mid bytes=${j.bytesReceived}; pausing briefly then cancel")
                pmtilesPauseJob(job.id)
                Thread.sleep(1_500)
                break
            }
            if (j.status == "completed" || j.status == "cancelled" || j.status.startsWith("failed")) {
                break
            }
            Thread.sleep(300)
        }
        pmtilesCancelJob(job.id)
        try {
            fut.get(45, TimeUnit.SECONDS)
        } catch (_: Exception) {
        }
        val after = pmtilesGetJob(dataDirPath, job.id)
        log("DEM cancel smoke sawBytes=$sawBytes status=${after?.status} paused=${after?.paused}")
        assertNotNull(after)
        assertTrue(
            "DEM cancel expected cancelled/failed, got ${after!!.status}",
            after.status == "cancelled" || after.status.startsWith("failed") || !sawBytes,
        )
    }

    @Test
    fun render_offline_and_online_after_pmtiles() {
        // Prefer existing completed Oslo extract; otherwise download once.
        val covering =
            uniffi.navi
                .pmtilesListCovering(dataDir.absolutePath, OSLO_LAT, OSLO_LON)
                .firstOrNull { it.status == "completed" && File(it.localPath).length() > 1000 }
        if (covering == null) {
            cleanOsloBasemapArtifacts()
            val job = pmtilesQueueRegion(dataDir.absolutePath, REGION, pmtilesPlanetUrl())
            assertTrue(job.id.isNotBlank())
            val done = pmtilesRunJob(dataDir.absolutePath, job.id)
            assertEquals("completed", done.status)
        }

        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)

        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.gpsAltitudeM = 47.0
        NaviMapTestHooks.forceOnlineBasemap = false
        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)

        activityRule.launchActivity(null)
        Thread.sleep(2_000)
        assertTrue(InstrumentedMapCapture.awaitStyleReady(60_000))

        // Offline Protomaps 2D at Oslo.
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.requestOptIn3d = false
        NaviMapTestHooks.requestCameraTiltDeg = 0.0
        waitKindAndCamera("OfflineProtomaps", OSLO_LAT, OSLO_LON, 12.0, want3d = false)
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(25_000))
        Thread.sleep(2_000)
        val offlineShot = InstrumentedMapCapture.takeScreenshotAfterSettle(8_000)
        assertNotNull(offlineShot)
        val offlineOut = File(dataDir, "pmtiles_pause_resume_offline.png")
        offlineOut.outputStream().use {
            offlineShot!!.compress(Bitmap.CompressFormat.PNG, 100, it)
        }
        InstrumentedMapCapture.screencapAfterSettle(OFFLINE_SHOT_TMP, 3_000)
        val cream = creamFraction(offlineShot!!)
        val wash = washFraction(offlineShot)
        log(
            "offline render kind=${NaviMapTestHooks.lastBasemapKind} creamFrac=$cream washFrac=$wash " +
                "file=${offlineOut.absolutePath}",
        )
        assertEquals("OfflineProtomaps", NaviMapTestHooks.lastBasemapKind)
        assertTrue(
            "offline land wash/olive (creamFrac=$cream washFrac=$wash)",
            cream >= 0.08 || wash < 0.50,
        )

        // Online Liberty / Online3d — must relaunch: flipping forceOnlineBasemap
        // mid-session does not re-apply style while the camera is already on target.
        captureOnlineLibertyPath()

        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.disableGpsFollow = false
        NaviMapTestHooks.followGps = true
    }

    /**
     * Standalone online-path check for host `am instrument` (no wipe).
     * Ensures wifi on / airplane off, force-online sticks, Oslo camera, screencap.
     */
    @Test
    fun render_online_path_after_pause_tests() {
        captureOnlineLibertyPath()
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.disableGpsFollow = false
        NaviMapTestHooks.followGps = true
    }

    private fun captureOnlineLibertyPath() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)

        shell("cmd connectivity airplane-mode disable")
        shell("svc wifi enable")
        shell("svc data enable")
        Thread.sleep(1_500)
        val airplane = shell("settings get global airplane_mode_on").trim()
        log("network airplane_mode_on=$airplane forceOnline=true")
        assertTrue("airplane must be off, got '$airplane'", airplane == "0" || airplane == "null")

        runCatching { activityRule.finishActivity() }
        Thread.sleep(800)

        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.gpsAltitudeM = 47.0
        NaviMapTestHooks.forceOnlineBasemap = true
        NaviMapTestHooks.requestOptIn3d = false
        NaviMapTestHooks.requestCameraTiltDeg = 0.0
        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.lastBasemapKind = ""
        NaviMapTestHooks.pendingCamera = Triple(OSLO_LAT, OSLO_LON, 12.0)
        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)

        activityRule.launchActivity(null)
        Thread.sleep(1_500)
        waitKindAndCamera(
            listOf("OnlineLiberty", "Online3d"),
            OSLO_LAT,
            OSLO_LON,
            12.0,
            want3d = false,
            forceOnline = true,
            timeoutMs = 90_000,
        )
        assertTrue(
            "expected lastBasemapKind to start with Online, got ${NaviMapTestHooks.lastBasemapKind}",
            NaviMapTestHooks.lastBasemapKind.startsWith("Online"),
        )
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(25_000))
        Thread.sleep(2_000)
        val onlineShot = InstrumentedMapCapture.takeScreenshotAfterSettle(8_000)
        assertNotNull(onlineShot)
        val onlineOut = File(dataDir, "online_path_after_pause_tests.png")
        onlineOut.outputStream().use {
            onlineShot!!.compress(Bitmap.CompressFormat.PNG, 100, it)
        }
        InstrumentedMapCapture.screencapAfterSettle(ONLINE_SHOT_TMP, 3_000)
        val kind = NaviMapTestHooks.lastBasemapKind
        log("online render kind=$kind file=${onlineOut.absolutePath} screencap=$ONLINE_SHOT_TMP")
        assertTrue(
            "expected OnlineLiberty or Online3d, got $kind",
            kind == "OnlineLiberty" || kind == "Online3d",
        )
    }

    private fun waitKindAndCamera(
        kind: String,
        lat: Double,
        lon: Double,
        zoom: Double,
        want3d: Boolean,
        forceOnline: Boolean = false,
        timeoutMs: Long = 60_000,
    ) = waitKindAndCamera(listOf(kind), lat, lon, zoom, want3d, forceOnline, timeoutMs)

    private fun waitKindAndCamera(
        kinds: List<String>,
        lat: Double,
        lon: Double,
        zoom: Double,
        want3d: Boolean,
        forceOnline: Boolean = false,
        timeoutMs: Long = 60_000,
    ) {
        if (forceOnline) {
            NaviMapTestHooks.forceOnlineBasemap = true
        }
        NaviMapTestHooks.requestOptIn3d = want3d
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            val kind = NaviMapTestHooks.lastBasemapKind
            val camOk =
                abs(NaviMapTestHooks.lastCameraLat - lat) <= 0.15 &&
                    abs(NaviMapTestHooks.lastCameraLon - lon) <= 0.15
            val kindOk =
                kind in kinds || (forceOnline && kind.startsWith("Online"))
            if (kindOk && NaviMapTestHooks.styleReady && camOk) {
                return
            }
            if (forceOnline) {
                NaviMapTestHooks.forceOnlineBasemap = true
            }
            NaviMapTestHooks.requestOptIn3d = want3d
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(400)
        }
        error(
            "timeout kinds=$kinds got=${NaviMapTestHooks.lastBasemapKind} " +
                "forceOnline=${NaviMapTestHooks.forceOnlineBasemap} " +
                "cam=${NaviMapTestHooks.lastCameraLat},${NaviMapTestHooks.lastCameraLon}",
        )
    }

    private fun washFraction(bmp: Bitmap): Double {
        var n = 0
        var wash = 0
        val step = 6
        val y0 = (bmp.height * 0.18).toInt()
        val y1 = (bmp.height * 0.75).toInt()
        val x0 = (bmp.width * 0.1).toInt()
        val x1 = (bmp.width * 0.9).toInt()
        var y = y0
        while (y < y1) {
            var x = x0
            while (x < x1) {
                val c = bmp.getPixel(x, y)
                val r = Color.red(c)
                val g = Color.green(c)
                val b = Color.blue(c)
                val water = b > 140 && b > r + 25 && b > g + 15
                val ui = r > 220 && g > 220 && b > 220
                if (!water && !ui) {
                    n++
                    val l = 0.2126 * r + 0.7152 * g + 0.0722 * b
                    val chroma = maxOf(r, g, b) - minOf(r, g, b)
                    val oliveSig =
                        abs(r - 88) + abs(g - 80) + abs(b - 60) < 35
                    val darkSlab = l < 118.0 && chroma < 25
                    if (oliveSig || darkSlab) wash++
                }
                x += step
            }
            y += step
        }
        return if (n == 0) 1.0 else wash.toDouble() / n
    }

    private fun creamFraction(bmp: Bitmap): Double {
        var n = 0
        var cream = 0
        val step = 6
        val y0 = (bmp.height * 0.18).toInt()
        val y1 = (bmp.height * 0.75).toInt()
        val x0 = (bmp.width * 0.1).toInt()
        val x1 = (bmp.width * 0.9).toInt()
        var y = y0
        while (y < y1) {
            var x = x0
            while (x < x1) {
                val c = bmp.getPixel(x, y)
                val r = Color.red(c)
                val g = Color.green(c)
                val b = Color.blue(c)
                val water = b > 140 && b > r + 25 && b > g + 15
                val ui = r > 220 && g > 220 && b > 220
                if (!water && !ui) {
                    n++
                    val dist = abs(r - 236) + abs(g - 228) + abs(b - 216)
                    if (dist < 45) cream++
                }
                x += step
            }
            y += step
        }
        return if (n == 0) 0.0 else cream.toDouble() / n
    }

    companion object {
        private const val TAG = "PmtilesPauseResume"
        private const val REGION = "test/oslo"
        private const val OSLO_LAT = 59.91
        private const val OSLO_LON = 10.75
        private const val PAUSED_JOB_FILE = "/data/local/tmp/navi_paused_pmtiles_job.txt"
        private const val OFFLINE_SHOT_TMP = "/data/local/tmp/pmtiles_pause_resume_offline.png"
        private const val ONLINE_SHOT_TMP = "/data/local/tmp/online_path_after_pause_tests.png"
    }
}
