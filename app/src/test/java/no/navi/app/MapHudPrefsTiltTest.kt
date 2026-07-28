package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Test

class MapHudPrefsTiltTest {
    @Test
    fun presetsStayWithinMapLibreMax() {
        for (p in MapHudPrefs.CAMERA_TILT_PRESETS) {
            assertEquals(
                "preset $p must be <= MapLibre max",
                true,
                p <= MapHudPrefs.MAPLIBRE_MAX_TILT_DEG + 1e-9,
            )
        }
        assertEquals(60.0, MapHudPrefs.MAPLIBRE_MAX_TILT_DEG, 0.0)
        assertEquals(60.0, MapHudPrefs.CAMERA_TILT_PRESETS.last(), 0.0)
    }

    @Test
    fun legacySixtyFiveSnapsToSixty() {
        // Old installs may still have 65 stored; MapLibre clamps to 60.
        assertEquals(60.0, MapHudPrefs.snapTilt(65.0), 0.0)
        assertEquals(60.0, MapHudPrefs.snapTilt(62.0), 0.0)
        assertEquals(45.0, MapHudPrefs.snapTilt(50.0), 0.0)
        assertEquals(0.0, MapHudPrefs.snapTilt(0.0), 0.0)
    }
}
