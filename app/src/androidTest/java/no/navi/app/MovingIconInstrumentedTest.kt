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
    // Same street-level zoom that ZoomPoiScreenshotTest proved renders tiles (z16).
    // 16.5 was in-range but early PixelCopy frames were often still black; z16 +
    // waiting for styleReady matches the working multi-zoom test path.
    private val zoom = 16.0

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

        // Reset hooks before launch so we do not clear styleReady after the style
        // callback already fired.
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.tracksEpoch = 0
        NaviMapTestHooks.tracksAppliedEpoch = 0
        NaviMapTestHooks.lastTrackIds = emptyList()
        NaviMapTestHooks.lastTrackFeatureCount = 0
        NaviMapTestHooks.lastTrackImagesReady = 0
        NaviMapTestHooks.snapshotRequestId = 0
        NaviMapTestHooks.lastSnapshotId = 0
        NaviMapTestHooks.lastSnapshotPng = null
        NaviMapTestHooks.pendingTracks = null
        NaviMapTestHooks.pendingCamera = Triple(centerLat, centerLon, zoom)

        activityRule.launchActivity(null)
        assertTrue(activityRule.activity.isFinishing.not())

        // Wait for MapLibre style (camera/tracks before styleReady are easy to drop).
        val styleDeadline = System.currentTimeMillis() + 20_000
        while (System.currentTimeMillis() < styleDeadline) {
            if (NaviMapTestHooks.styleReady) break
            Thread.sleep(200)
        }
        assertTrue("MapLibre style not ready", NaviMapTestHooks.styleReady)
        // Re-pin camera after style load and give tiles a moment (ZoomPoi uses ~4.5s).
        NaviMapTestHooks.pendingCamera = Triple(centerLat, centerLon, zoom)
        Thread.sleep(4_500)

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
            val featDeadline = System.currentTimeMillis() + 8_000
            while (System.currentTimeMillis() < featDeadline) {
                if (
                    NaviMapTestHooks.lastTrackFeatureCount == snap.size &&
                    NaviMapTestHooks.tracksAppliedEpoch >= epochWait &&
                    NaviMapTestHooks.lastTrackOverlayCount == snap.size &&
                    NaviMapTestHooks.lastTrackImagesReady >= snap.size
                ) {
                    break
                }
                Thread.sleep(100)
            }
            assertEquals(
                "style feature count",
                snap.size,
                NaviMapTestHooks.lastTrackFeatureCount,
            )
            assertTrue(
                "tracks not yet on MapLibre style (appliedEpoch=${NaviMapTestHooks.tracksAppliedEpoch})",
                NaviMapTestHooks.tracksAppliedEpoch >= epochWait,
            )
            assertEquals(
                "visible overlay markers",
                snap.size,
                NaviMapTestHooks.lastTrackOverlayCount,
            )
            assertTrue(
                "track icons not registered (${NaviMapTestHooks.lastTrackImagesReady}/${snap.size})",
                NaviMapTestHooks.lastTrackImagesReady >= snap.size,
            )
            // Let the GL thread paint symbols after geojson/image registration.
            Thread.sleep(1_500)
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
     * Capture via UiAutomation screencap (same path as ZoomPoiScreenshotTest).
     * MapLibre PixelCopy/snapshot often returns a blank buffer on this Automotive emulator.
     */
    private fun mapSnapshot(name: String): File {
        // Nudge the map idle/snapshot hooks, but do not trust tiny/blank hook PNGs.
        val req = NaviMapTestHooks.snapshotRequestId + 1
        NaviMapTestHooks.lastSnapshotPng = null
        NaviMapTestHooks.snapshotRequestId = req
        Thread.sleep(800)

        fun shell(cmd: String) {
            val pfd = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
            java.io.FileInputStream(pfd.fileDescriptor).use { input ->
                val buf = ByteArray(4096)
                while (input.read(buf) >= 0) {
                }
            }
            pfd.close()
        }

        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertTrue("UiAutomation screenshot null for $name", shot != null)
        val outStream = java.io.ByteArrayOutputStream()
        shot!!.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, outStream)
        val png = outStream.toByteArray()
        assertTrue("snapshot empty for $name", png.isNotEmpty())

        // Reject near-black frames (style/tiles not painted yet).
        var nonBlack = 0
        val step = (shot.width * shot.height / 2000).coerceAtLeast(1)
        var i = 0
        while (i < shot.width * shot.height) {
            val c = shot.getPixel(i % shot.width, i / shot.width)
            val r = (c shr 16) and 0xff
            val g = (c shr 8) and 0xff
            val b = c and 0xff
            if (r > 20 || g > 20 || b > 20) nonBlack++
            i += step
        }
        assertTrue(
            "$name looks blank/black (nonBlack samples=$nonBlack) — map not ready",
            nonBlack > 30,
        )

        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dir = NaviAppData.resolve(ctx)
        val out = File(dir, name)
        out.writeBytes(png)
        assertTrue("$name too small", out.length() > 20_000)

        shell("screencap -p /data/local/tmp/$name")
        shell("ls -la /data/local/tmp/$name")
        android.util.Log.i(
            "MovingIconTest",
            "mapSnapshot $name bytes=${out.length()} path=${out.absolutePath} " +
                "features=${NaviMapTestHooks.lastTrackFeatureCount} " +
                "images=${NaviMapTestHooks.lastTrackImagesReady}",
        )
        return out
    }
}
