package no.navi.app.cat

import java.io.InputStream
import java.io.OutputStream
import java.net.ServerSocket
import java.net.Socket
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.concurrent.thread

/**
 * Bridges a byte stream (USB serial or Bluetooth SPP) to a loopback TCP port
 * so Hamlib can open `127.0.0.1:<port>` as [RIG_PORT_NETWORK].
 *
 * Unit tests inject fake streams; production wires UsbSerialPort / BluetoothSocket.
 */
class CatSerialLoopbackBridge(
    private val serialIn: InputStream,
    private val serialOut: OutputStream,
    preferPort: Int = 0,
) : AutoCloseable {
    private val server = ServerSocket(preferPort)
    val port: Int get() = server.localPort
    private val running = AtomicBoolean(true)
    private var client: Socket? = null

    val pathname: String get() = "127.0.0.1:$port"

    fun start() {
        thread(name = "cat-loopback-accept", isDaemon = true) {
            while (running.get()) {
                try {
                    val c = server.accept()
                    client?.close()
                    client = c
                    pump(c.getInputStream(), serialOut)
                    pump(serialIn, c.getOutputStream())
                } catch (_: Exception) {
                    if (!running.get()) break
                }
            }
        }
    }

    private fun pump(from: InputStream, to: OutputStream) {
        thread(name = "cat-loopback-pump", isDaemon = true) {
            val buf = ByteArray(4096)
            try {
                while (running.get()) {
                    val n = from.read(buf)
                    if (n < 0) break
                    if (n > 0) {
                        to.write(buf, 0, n)
                        to.flush()
                    }
                }
            } catch (_: Exception) {
                // disconnect / close
            }
        }
    }

    override fun close() {
        running.set(false)
        try {
            client?.close()
        } catch (_: Exception) {
        }
        try {
            server.close()
        } catch (_: Exception) {
        }
    }
}

/** Remote NET rigctl endpoint (emulator uses 10.0.2.2:4532 for host rigctld). */
data class CatRemoteRigctld(
    val host: String,
    val port: Int = 4532,
) {
    fun endpoint(): String = "$host:$port"
}
