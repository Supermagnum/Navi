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
 * Unit tests inject fake streams; production wires UsbSerialPort / BluetoothSocket
 * opened at [baudRate] before constructing the bridge.
 */
class CatSerialLoopbackBridge(
    private val serialIn: InputStream,
    private val serialOut: OutputStream,
    preferPort: Int = 0,
    val baudRate: Int = DEFAULT_BAUD,
) : AutoCloseable {
    private val server = ServerSocket(preferPort)
    val port: Int get() = server.localPort
    private val running = AtomicBoolean(true)
    private var client: Socket? = null

    val pathname: String get() = "127.0.0.1:$port"

    init {
        require(baudRate > 0) { "baudRate must be positive" }
    }

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

    private fun pump(
        from: InputStream,
        to: OutputStream,
    ) {
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

    companion object {
        const val DEFAULT_BAUD = 9600

        /** Common CAT serial baud rates (USB/BT → loopback). */
        val BAUD_RATES: List<Int> = listOf(4800, 9600, 19200, 38400, 57600, 115200)

        fun normalizeBaud(baud: Int): Int {
            if (baud in BAUD_RATES) return baud
            // Snap to nearest supported rate for prefs migration / typos.
            return BAUD_RATES.minByOrNull { kotlin.math.abs(it - baud) } ?: DEFAULT_BAUD
        }

        /**
         * Production open path: apply [baudRate] on the serial port, then bridge.
         * Callers pass already-opened streams from UsbSerialPort / BluetoothSocket
         * configured with [openParams].
         */
        fun fromSerialStreams(
            serialIn: InputStream,
            serialOut: OutputStream,
            openParams: CatSerialOpenParams,
            preferPort: Int = 0,
        ): CatSerialLoopbackBridge {
            val baud = normalizeBaud(openParams.baudRate)
            return CatSerialLoopbackBridge(serialIn, serialOut, preferPort, baud)
        }
    }
}

/** Params used when opening USB/BT serial before [CatSerialLoopbackBridge]. */
data class CatSerialOpenParams(
    val baudRate: Int = CatSerialLoopbackBridge.DEFAULT_BAUD,
    /** Hamlib model id for onboard FFI (radio) or documentation of remote daemon model. */
    val rigModel: Int = 2,
)

/** Curated Hamlib models shown in the CAT sheet (number + name). */
data class CatRigModelPreset(
    val model: Int,
    val name: String,
)

fun catRigModelPresets(): List<CatRigModelPreset> =
    listOf(
        CatRigModelPreset(2, "NET rigctl (remote TCP)"),
        CatRigModelPreset(1, "Dummy (CI only)"),
        CatRigModelPreset(1036, "Yaesu FT-891"),
        CatRigModelPreset(1022, "Yaesu FT-857"),
        CatRigModelPreset(1020, "Yaesu FT-817"),
        CatRigModelPreset(1041, "Yaesu FT-818"),
        CatRigModelPreset(1035, "Yaesu FT-991"),
        CatRigModelPreset(2014, "Kenwood TS-2000"),
        CatRigModelPreset(2034, "Kenwood TM-D710"),
        CatRigModelPreset(3085, "Icom IC-705"),
        CatRigModelPreset(3073, "Icom IC-7300"),
    )

fun catRigModelLabel(model: Int): String =
    catRigModelPresets().firstOrNull { it.model == model }?.let { "${it.model} ${it.name}" }
        ?: "$model (custom)"

/** Remote NET rigctl endpoint (emulator uses 10.0.2.2:4532 for host rigctld). */
data class CatRemoteRigctld(
    val host: String,
    val port: Int = 4532,
) {
    fun endpoint(): String = "$host:$port"
}
