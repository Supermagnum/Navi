package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiTrackStore
import uniffi.navi.displayRangeMaxKm
import uniffi.navi.displayRangeMinKm
import uniffi.navi.haversineKm
import uniffi.navi.offsetLatLonM
import uniffi.navi.stationTimeoutMaxS
import java.io.File

/**
 * Moving-icon position updates: upsert-by-id moves markers in place (no duplicates),
 * symbols stick across updates, timeout expiry removes stale stations.
 */
@RunWith(AndroidJUnit4::class)
class MovingIconInstrumentedTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private val centerLat = 60.722823
    private val centerLon = 10.613182
    // AA landscape chrome shortens the map viewport; z≈17.5 clips a 300 m radius
    // vertically. 16.5 keeps ~300 m readable with room for four independent moves.
    private val zoom = 16.5

    private data class IconSpec(
        val id: String,
        val symbolKey: String,
        val table: String,
        val code: String,
        val east0: Double,
        val north0: Double,
        val east1: Double,
        val north1: Double,
        val east2: Double,
        val north2: Double,
    )

    // Offsets stay well inside the 300 m radius so icons remain on-screen after chrome.
    private val icons = listOf(
        IconSpec("CAR-9", "aprs_car", "/", ">", -90.0, 70.0, 80.0, 90.0, 30.0, -100.0),
        IconSpec("HUMAN-7", "aprs_human", "/", "[", 100.0, 50.0, -70.0, 110.0, -110.0, -30.0),
        IconSpec("HOUSE-1", "aprs_house", "/", "-", -70.0, -80.0, 90.0, -60.0, -40.0, 95.0),
        IconSpec("DIGI-0", "aprs_digi", "/", "#", 60.0, -95.0, -90.0, -70.0, 105.0, 40.0),
    )

    @Test
    fun movingIcons_updateInPlace_noDuplicates_timeoutExpires() {
        // Range / timeout policy checks (hardcoded APRS rules).
        assertEquals(3600uL, stationTimeoutMaxS())
        assertEquals(50.0, displayRangeMinKm(), 0.001)
        assertEquals(150.0, displayRangeMaxKm(), 0.001)

        // 300 m radius setup is well within the 50–150 km display window.
        val edge = offsetLatLonM(centerLat, centerLon, 300.0, 0.0)
        val dKm = haversineKm(centerLat, centerLon, edge[0], edge[1])
        assertTrue("300 m edge must be << 50 km (got $dKm km)", dKm < 1.0)
        assertTrue("300 m within display max", dKm <= displayRangeMaxKm())

        activityRule.launchActivity(null)
        assertTrue(activityRule.activity.isFinishing.not())

        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.tracksEpoch = 0
        NaviMapTestHooks.lastTrackIds = emptyList()
        NaviMapTestHooks.lastTrackFeatureCount = 0
        NaviMapTestHooks.snapshotRequestId = 0
        NaviMapTestHooks.lastSnapshotId = 0
        NaviMapTestHooks.lastSnapshotPng = null
        NaviMapTestHooks.pendingCamera = Triple(centerLat, centerLon, zoom)
        Thread.sleep(5_000)

        // Short timeout for expiry simulation; still ≤ 3600.
        val store = FfiTrackStore(timeoutS = 2u, rangeKm = 150.0)
        assertEquals(2uL, store.timeoutS())
        assertEquals(150.0, store.rangeKm(), 0.001)

        fun pushFromStore(epochWait: Int) {
            val snap = store.visible(centerLat, centerLon)
            NaviMapTestHooks.pendingTracks = snap.map {
                TrackMarker(
                    id = it.id,
                    lat = it.lat,
                    lon = it.lon,
                    symbolKey = it.symbolKey,
                    label = it.id,
                )
            }
            val deadline = System.currentTimeMillis() + 8_000
            while (System.currentTimeMillis() < deadline) {
                if (NaviMapTestHooks.tracksEpoch >= epochWait) break
                Thread.sleep(200)
            }
            assertTrue(
                "tracksEpoch expected >= $epochWait, got ${NaviMapTestHooks.tracksEpoch}",
                NaviMapTestHooks.tracksEpoch >= epochWait,
            )
            val ids = NaviMapTestHooks.lastTrackIds
            assertEquals("no duplicate marker ids", ids.size, ids.toSet().size)
            // Re-pin camera after each track push so zoom/center cannot drift.
            NaviMapTestHooks.pendingCamera = Triple(centerLat, centerLon, zoom)
            val featDeadline = System.currentTimeMillis() + 5_000
            while (System.currentTimeMillis() < featDeadline) {
                if (NaviMapTestHooks.lastTrackFeatureCount == snap.size) break
                Thread.sleep(100)
            }
            assertEquals(
                "style feature count",
                snap.size,
                NaviMapTestHooks.lastTrackFeatureCount,
            )
        }

        fun pos(east: Double, north: Double): Pair<Double, Double> {
            val ll = offsetLatLonM(centerLat, centerLon, east, north)
            return ll[0] to ll[1]
        }

        // --- Step 1: initial positions ---
        var t = 1_000uL
        for (icon in icons) {
            val (lat, lon) = pos(icon.east0, icon.north0)
            val outcome = store.upsert(
                id = icon.id,
                lat = lat,
                lon = lon,
                symbolTable = icon.table,
                symbolCode = icon.code,
                symbolKey = icon.symbolKey,
                lastHeardUnix = t,
                comment = "start",
            )
            assertEquals("${icon.id} first upsert", "created", outcome)
        }
        assertEquals(4, store.len().toInt())
        pushFromStore(1)
        Thread.sleep(2_000)
        val beforeShot = mapSnapshot("navi_moving_before.png")

        // Per-icon initial checks
        for (icon in icons) {
            val s = store.all().first { it.id == icon.id }
            assertEquals(icon.symbolKey, s.symbolKey)
            val (elat, elon) = pos(icon.east0, icon.north0)
            assertTrue(icon.id, kotlin.math.abs(s.lat - elat) < 1e-6)
            assertTrue(icon.id, kotlin.math.abs(s.lon - elon) < 1e-6)
        }

        // --- Step 2: position updates (in place) ---
        t = 1_010uL
        for (icon in icons) {
            val (lat, lon) = pos(icon.east1, icon.north1)
            val outcome = store.upsert(
                id = icon.id,
                lat = lat,
                lon = lon,
                symbolTable = icon.table,
                symbolCode = icon.code,
                symbolKey = icon.symbolKey,
                lastHeardUnix = t,
                comment = "move1",
            )
            assertEquals("${icon.id} must update not create", "updated", outcome)
        }
        assertEquals("still exactly 4 stations after update", 4, store.len().toInt())
        pushFromStore(2)
        Thread.sleep(2_000)
        val afterShot = mapSnapshot("navi_moving_after.png")
        assertFalse(
            "before/after MapLibre snapshots must differ (markers moved on map)",
            beforeShot.readBytes().contentEquals(afterShot.readBytes()),
        )

        for (icon in icons) {
            val s = store.all().first { it.id == icon.id }
            assertEquals("${icon.id} symbol retained", icon.symbolKey, s.symbolKey)
            val (elat, elon) = pos(icon.east1, icon.north1)
            assertTrue("${icon.id} moved", kotlin.math.abs(s.lat - elat) < 1e-6)
            assertTrue("${icon.id} moved", kotlin.math.abs(s.lon - elon) < 1e-6)
        }

        // --- Step 3: second update (repeated moves) ---
        t = 1_020uL
        for (icon in icons) {
            val (lat, lon) = pos(icon.east2, icon.north2)
            assertEquals(
                "updated",
                store.upsert(
                    id = icon.id,
                    lat = lat,
                    lon = lon,
                    symbolTable = icon.table,
                    symbolCode = icon.code,
                    symbolKey = icon.symbolKey,
                    lastHeardUnix = t,
                    comment = "move2",
                ),
            )
        }
        assertEquals(4, store.len().toInt())
        pushFromStore(3)
        Thread.sleep(2_000)
        val after2Shot = mapSnapshot("navi_moving_after2.png")
        assertFalse(
            "after/after2 MapLibre snapshots must differ (second move)",
            afterShot.readBytes().contentEquals(after2Shot.readBytes()),
        )

        for (icon in icons) {
            val s = store.all().first { it.id == icon.id }
            assertEquals(icon.symbolKey, s.symbolKey)
            val (elat, elon) = pos(icon.east2, icon.north2)
            assertTrue(kotlin.math.abs(s.lat - elat) < 1e-6)
            assertTrue(kotlin.math.abs(s.lon - elon) < 1e-6)
        }

        // --- Timeout: leave DIGI-0 stale ---
        t = 1_030uL
        for (icon in icons.filter { it.id != "DIGI-0" }) {
            val s = store.all().first { it.id == icon.id }
            store.upsert(
                id = icon.id,
                lat = s.lat,
                lon = s.lon,
                symbolTable = icon.table,
                symbolCode = icon.code,
                symbolKey = icon.symbolKey,
                lastHeardUnix = t,
                comment = "keep",
            )
        }
        // DIGI-0 last heard still 1020; now 1024 → age 4 > timeout 2
        val removed = store.expire(1_024uL)
        assertTrue("DIGI-0 must expire", removed.contains("DIGI-0"))
        assertFalse(store.all().any { it.id == "DIGI-0" })
        assertEquals(3, store.len().toInt())
        pushFromStore(4)
        assertEquals(3, NaviMapTestHooks.lastTrackFeatureCount)
        Thread.sleep(2_000)
        val timeoutShot = mapSnapshot("navi_moving_after_timeout.png")
        assertFalse(
            "timeout MapLibre snapshot must differ (DIGI-0 removed)",
            after2Shot.readBytes().contentEquals(timeoutShot.readBytes()),
        )
        assertFalse(NaviMapTestHooks.lastTrackIds.contains("DIGI-0"))
        assertEquals(3, NaviMapTestHooks.lastTrackIds.size)
    }

    /**
     * Capture a map screenshot after MapLibre reports idle (via snapshot hooks).
     * Uses UiAutomation + screencap so export works across Android Automotive user 10;
     * MapLibre map.snapshot() returns blank buffers on this emulator.
     */
    private fun mapSnapshot(name: String): File {
        val req = NaviMapTestHooks.snapshotRequestId + 1
        NaviMapTestHooks.lastSnapshotPng = null
        NaviMapTestHooks.snapshotRequestId = req
        val deadline = System.currentTimeMillis() + 12_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastSnapshotId >= req && NaviMapTestHooks.lastSnapshotPng != null) {
                break
            }
            Thread.sleep(200)
        }
        // Prefer hook PNG only when it belongs to this request and looks real.
        var png: ByteArray? = null
        if (NaviMapTestHooks.lastSnapshotId >= req) {
            val hooked = NaviMapTestHooks.lastSnapshotPng
            if (hooked != null && hooked.size >= 20_000) {
                png = hooked
            }
        }
        if (png == null) {
            val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
            assertTrue("UiAutomation screenshot null for $name", shot != null)
            val outStream = java.io.ByteArrayOutputStream()
            shot!!.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, outStream)
            png = outStream.toByteArray()
        }
        assertTrue("snapshot empty for $name", png != null && png!!.isNotEmpty())
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dir = (ctx.getExternalFilesDir(null) ?: ctx.filesDir).also { it.mkdirs() }
        val out = File(dir, name)
        out.writeBytes(png!!)
        assertTrue("$name too small", out.length() > 5_000)

        fun shell(cmd: String) {
            val pfd = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
            java.io.FileInputStream(pfd.fileDescriptor).use { input ->
                val buf = ByteArray(4096)
                while (input.read(buf) >= 0) {
                }
            }
            pfd.close()
        }
        // Device screencap is the reliable pull path (same as ZoomPoi / Corridor tests).
        shell("screencap -p /data/local/tmp/$name")
        shell("ls -la /data/local/tmp/$name")
        // Also try copying the exact PNG bytes we asserted on.
        try {
            val b64 = android.util.Base64.encodeToString(png, android.util.Base64.NO_WRAP)
            val b64Path = "/data/local/tmp/${name}.hook.b64"
            shell("rm -f $b64Path /data/local/tmp/${name}.hook.png")
            for (chunk in b64.chunked(6000)) {
                shell("printf '%s' '$chunk' >> $b64Path")
            }
            shell("base64 -d $b64Path > /data/local/tmp/${name}.hook.png")
            shell("rm -f $b64Path")
        } catch (_: Throwable) {
        }
        android.util.Log.i(
            "MovingIconTest",
            "mapSnapshot $name bytes=${out.length()} path=${out.absolutePath}",
        )
        return out
    }
}
