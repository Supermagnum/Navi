package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.CampingCallKind
import uniffi.navi.campingPluginCapabilitySourcesJson
import uniffi.navi.campingPluginConfigure
import uniffi.navi.campingPluginInstallGuest
import uniffi.navi.campingPluginIsEnabled
import uniffi.navi.campingPluginRunIsolationGuest
import uniffi.navi.campingPluginSetEnabled
import uniffi.navi.initNativeLogging
import java.io.File
import java.util.TimeZone

/**
 * Phase 5a: wasmtime sandbox on the emulator with deliberately misbehaving guests.
 * Fuel / timeout / memory / trap must fail closed (no process crash).
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
class CampingSandboxInstrumentedTest {
    private lateinit var filesDir: File
    private lateinit var dataDir: File

    @Before
    fun setUp() {
        initNativeLogging()
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        filesDir = ctx.filesDir
        dataDir = File(filesDir, "navi-data").also { it.mkdirs() }
        campingPluginConfigure(
            filesDir.absolutePath,
            dataDir.absolutePath,
            TimeZone.getDefault().id,
        )
        installAssetGuest("busy_loop")
        installAssetGuest("trap_guest")
        installAssetGuest("memory_bomb")
        installAssetGuest("right_to_roam_camping")
        campingPluginSetEnabled(false)
    }

    private fun installAssetGuest(name: String) {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val am = ctx.assets
        val manifest =
            am.open("plugins/$name/plugin.json").use { it.readBytes().decodeToString() }
        val wasm = am.open("plugins/$name/plugin.wasm").use { it.readBytes() }
        val status = campingPluginInstallGuest(name, manifest, wasm)
        assertTrue("install $name: $status", status.startsWith("OK"))
    }

    @Test
    fun enableDefaultsOff_andDisableDeletesNightStore() {
        assertFalse(campingPluginIsEnabled())
        val kv = File(filesDir, "plugin_kv/camping_night.json")
        kv.parentFile?.mkdirs()
        kv.writeText("""{"rtr_night_active:no":"cell:1:1"}""")
        assertTrue(kv.isFile)
        val on = campingPluginSetEnabled(true)
        assertTrue(on, on.startsWith("OK"))
        assertTrue(campingPluginIsEnabled())
        val off = campingPluginSetEnabled(false)
        assertTrue(off, off.contains("disabled"))
        assertFalse(campingPluginIsEnabled())
        assertFalse("night store must be deleted on disable", kv.isFile)
    }

    @Test
    fun busyLoop_fuelOrTimeout_noCrash() {
        val r = campingPluginRunIsolationGuest("busy_loop")
        assertTrue(
            "busy_loop kind=${r.kind} msg=${r.message}",
            r.kind == CampingCallKind.FUEL_EXHAUSTED || r.kind == CampingCallKind.TIMEOUT,
        )
        assertTrue(r.elapsedMs < 5_000uL)
    }

    @Test
    fun trapGuest_classified_noCrash() {
        val r = campingPluginRunIsolationGuest("trap_guest")
        assertEquals(CampingCallKind.TRAP, r.kind)
        assertTrue(r.message.contains("trap", ignoreCase = true))
    }

    @Test
    fun memoryBomb_memoryExceeded_noCrash() {
        val r = campingPluginRunIsolationGuest("memory_bomb")
        assertEquals(
            "memory_bomb kind=${r.kind} msg=${r.message}",
            CampingCallKind.MEMORY_EXCEEDED,
            r.kind,
        )
        assertTrue(r.elapsedMs < 5_000uL)
    }

    @Test
    fun uiPath_isolationGuests_viaRunSuggest() {
        campingPluginSetEnabled(true)
        val campingDir = File(filesDir, "plugins/right_to_roam_camping")
        for (name in listOf("busy_loop", "trap_guest", "memory_bomb")) {
            val src = File(filesDir, "plugins/$name")
            src.copyRecursively(campingDir, overwrite = true)
            campingPluginConfigure(
                filesDir.absolutePath,
                dataDir.absolutePath,
                TimeZone.getDefault().id,
            )
            campingPluginSetEnabled(true)
            val r =
                kotlinx.coroutines.runBlocking {
                    CampingPluginApi.runSuggest("{}")
                }
            when (name) {
                "busy_loop" ->
                    assertTrue(
                        "busy_loop via UI path kind=${r.kind} ${r.message}",
                        r.kind == CampingCallKind.FUEL_EXHAUSTED || r.kind == CampingCallKind.TIMEOUT,
                    )
                "trap_guest" ->
                    assertEquals("trap via UI path ${r.message}", CampingCallKind.TRAP, r.kind)
                "memory_bomb" ->
                    assertEquals(
                        "memory_bomb via UI path ${r.message}",
                        CampingCallKind.MEMORY_EXCEEDED,
                        r.kind,
                    )
            }
        }
        installAssetGuest("right_to_roam_camping")
        campingPluginConfigure(
            filesDir.absolutePath,
            dataDir.absolutePath,
            TimeZone.getDefault().id,
        )
        campingPluginSetEnabled(false)
    }

    @Test
    fun capabilitySourcesDocumented() {
        val json = campingPluginCapabilitySourcesJson()
        assertTrue(json.contains("clock_read"))
        assertTrue(json.contains("route_destination_read"))
        assertTrue(json.contains("PluginEnableStore"))
    }
}
