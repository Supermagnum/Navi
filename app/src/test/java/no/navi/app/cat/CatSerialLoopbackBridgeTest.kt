package no.navi.app.cat

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.PipedInputStream
import java.io.PipedOutputStream
import java.net.Socket

class CatSerialLoopbackBridgeTest {
    @Test
    fun pathname_uses_loopback_port() {
        val serialIn = ByteArrayInputStream(ByteArray(0))
        val serialOut = ByteArrayOutputStream()
        CatSerialLoopbackBridge(serialIn, serialOut).use { bridge ->
            assertTrue(bridge.pathname.startsWith("127.0.0.1:"))
            assertTrue(bridge.port > 0)
        }
    }

    @Test
    fun pumps_bytes_tcp_to_serial() {
        val toSerial = PipedOutputStream()
        val serialIn = PipedInputStream(toSerial)
        val serialOut = ByteArrayOutputStream()
        CatSerialLoopbackBridge(serialIn, serialOut).use { bridge ->
            bridge.start()
            Socket("127.0.0.1", bridge.port).use { sock ->
                sock.getOutputStream().write("FA145725000;".toByteArray())
                sock.getOutputStream().flush()
                val deadline = System.currentTimeMillis() + 2000
                while (serialOut.size() < 12 && System.currentTimeMillis() < deadline) {
                    Thread.sleep(20)
                }
                assertEquals("FA145725000;", serialOut.toString("UTF-8"))
            }
        }
    }

    @Test
    fun remote_rigctld_emulator_default() {
        val r = CatRemoteRigctld("10.0.2.2")
        assertEquals("10.0.2.2:4532", r.endpoint())
    }
}
