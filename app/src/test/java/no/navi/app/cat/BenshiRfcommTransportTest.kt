package no.navi.app.cat

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.io.PipedInputStream
import java.io.PipedOutputStream
import java.util.concurrent.atomic.AtomicInteger

/**
 * Injectable-socket tests only — no real Bluetooth devices.
 */
class BenshiRfcommTransportTest {
    @Test
    fun spp_uuid_is_standard_serial_port_profile() {
        assertEquals(
            "00001101-0000-1000-8000-00805f9b34fb",
            BenshiRfcommTransport.SPP_UUID.toString(),
        )
    }

    @Test
    fun audio_uuid_documented_and_distinct_from_spp() {
        assertFalse(
            BenshiRfcommTransport.AUDIO_RFCOMM_UUID_DO_NOT_OPEN == BenshiRfcommTransport.SPP_UUID,
        )
    }

    @Test
    fun gaia_round_trip_extract() {
        // group/cmd + 1-byte payload (matches navi-cat gaia_encode layout)
        val body = byteArrayOf(0x00, 0x02, 0x00, 0x04, 0x00)
        val framed = BenshiRfcommTransport.gaiaEncode(body)
        val acc = ArrayList<Byte>()
        for (b in framed) acc.add(b)
        val got = BenshiRfcommTransport.tryExtractGaia(acc)
        assertArrayEquals(body, got)
    }

    @Test
    fun transact_one_at_a_time_with_injected_socket() {
        val peerToHost = PipedOutputStream()
        val hostFromPeer = PipedInputStream(peerToHost)
        val hostToPeer = ByteArrayOutputStream()
        val opens = AtomicInteger(0)
        val reconnects = AtomicInteger(0)

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

                override fun onReconnected(notificationTypes: List<Byte>) {
                    reconnects.incrementAndGet()
                }
            }

        BenshiRfcommTransport(factory, commandTimeoutMs = 500).use { transport ->
            transport.connect()
            transport.noteNotificationRegistration(14)
            // Peer replies after host writes: echo a framed ACK in a side thread.
            Thread {
                val deadline = System.currentTimeMillis() + 2000
                while (hostToPeer.size() < 4 && System.currentTimeMillis() < deadline) {
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
        }
    }
}
