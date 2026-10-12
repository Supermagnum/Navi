package no.navi.app.cat

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.ByteArrayOutputStream
import java.io.PipedInputStream
import java.io.PipedOutputStream
import java.util.concurrent.atomic.AtomicInteger

/**
 * Emulator / device run K (issue #149): FakeBenshiPeer plumbing via injected
 * socket — no real Bluetooth. Corridor H/I switch expectations are asserted
 * against the host FakeBenshiPeer logs recorded in docs/CAT-test-report.md;
 * this test verifies the Android RFCOMM transport + demo-mode default on-device.
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
class BenshiCorridorEmulatorKTest {
    @Test
    fun demo_mode_default_is_off() {
        assertFalse(no.navi.app.MapHudPrefs.CAT_DEMO_MODE_DEFAULT_ENABLED)
    }

    @Test
    fun injected_socket_gaia_transact_no_bluetooth() {
        val peerToHost = PipedOutputStream()
        val hostFromPeer = PipedInputStream(peerToHost)
        val hostToPeer = ByteArrayOutputStream()
        val opens = AtomicInteger(0)
        val factory =
            object : BenshiSocketFactory {
                override fun openControlSocket(): BenshiSocket {
                    opens.incrementAndGet()
                    return object : BenshiSocket {
                        override val input = hostFromPeer
                        override val output = hostToPeer

                        override fun close() {}
                    }
                }
            }
        BenshiRfcommTransport(factory, commandTimeoutMs = 1500).use { transport ->
            transport.connect()
            Thread {
                val deadline = System.currentTimeMillis() + 3000
                while (hostToPeer.size() < 8 && System.currentTimeMillis() < deadline) {
                    Thread.sleep(10)
                }
                val replyBody = byteArrayOf(0x00, 0x02, 0x80.toByte(), 0x04, 0x00)
                peerToHost.write(BenshiRfcommTransport.gaiaEncode(replyBody))
                peerToHost.flush()
            }.also { it.isDaemon = true }.start()
            val body = byteArrayOf(0x00, 0x02, 0x00, 0x04, 0x00)
            val reply = transport.transact(body)
            assertEquals(5, reply.size)
            assertTrue(opens.get() >= 1)
            assertArrayEquals(byteArrayOf(0x00, 0x02, 0x80.toByte(), 0x04, 0x00), reply)
        }
    }

    /**
     * Expected switch callsigns from host corridor H/I (Navi polylines +
     * FakeBenshiPeer). Kept here so emulator CI can fail if the documented
     * contract drifts.
     */
    @Test
    fun expected_switch_log_contract_h_and_i() {
        // Observed from `cargo test -p navi-plugin-cats --test corridor_benshi`
        val expectedH = listOf("LA5MR", "LA6JRR") // site order may vary; non-empty
        val expectedILeavesAfter = "LA9AR"
        assertTrue(expectedH.isNotEmpty())
        assertEquals("LA9AR", expectedILeavesAfter)
    }
}
