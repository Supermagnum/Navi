package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.loadAvoidFerries
import uniffi.navi.loadUseNetworkedCabins
import uniffi.navi.loadUseUnlockedCabins
import uniffi.navi.saveAvoidFerries
import uniffi.navi.saveUseNetworkedCabins
import uniffi.navi.saveUseUnlockedCabins

/**
 * Process-death persistence for avoid_ferries (and cabin keys for regression).
 *
 * Host orchestration (real kill, not Compose navigation):
 * 1. Run [seed_persistsAvoidFerriesAndCabins]
 * 2. `adb shell am force-stop no.navi.app`
 * 3. Run [load_afterProcessKill_readsPersistedPrefs]
 *
 * The seed/load split is intentional: a single process cannot prove survival
 * across force-stop.
 */
@RunWith(AndroidJUnit4::class)
class AvoidFerriesPersistInstrumentedTest {
    private fun dataDir(): String =
        NaviAppData.resolve(InstrumentationRegistry.getInstrumentation().targetContext).absolutePath

    @Test
    fun seed_persistsAvoidFerriesAndCabins() {
        val dir = dataDir()
        assertTrue(saveAvoidFerries(dir, true))
        assertTrue(saveUseNetworkedCabins(dir, true))
        assertTrue(saveUseUnlockedCabins(dir, true))
        assertTrue(loadAvoidFerries(dir))
        assertTrue(loadUseNetworkedCabins(dir))
        assertTrue(loadUseUnlockedCabins(dir))
    }

    @Test
    fun load_afterProcessKill_readsPersistedPrefs() {
        val dir = dataDir()
        // Must be run in a NEW process after force-stop following seed_*.
        assertTrue(
            "avoid_ferries must survive process death (ConfigStore app_config)",
            loadAvoidFerries(dir),
        )
        assertTrue(
            "use_networked_cabins must survive process death",
            loadUseNetworkedCabins(dir),
        )
        assertTrue(
            "use_unlocked_cabins must survive process death",
            loadUseUnlockedCabins(dir),
        )
    }

    @Test
    fun clear_resetsAvoidFerriesWithoutClobberingCabins() {
        val dir = dataDir()
        // Re-seed cabins first: debug dump intents reset cabin defaults mid-suite.
        assertTrue(saveUseNetworkedCabins(dir, true))
        assertTrue(saveUseUnlockedCabins(dir, true))
        assertTrue(saveAvoidFerries(dir, true))
        assertTrue(saveAvoidFerries(dir, false))
        assertFalse(loadAvoidFerries(dir))
        // Distinct keys: clearing avoid_ferries must not wipe cabin prefs.
        assertTrue(loadUseNetworkedCabins(dir))
        assertTrue(loadUseUnlockedCabins(dir))
    }
}
