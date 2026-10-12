package no.navi.app.cat

import java.io.Closeable
import java.io.InputStream
import java.io.OutputStream
import java.util.UUID
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.concurrent.thread

/**
 * Benshi control RFCOMM transport (SPP UUID, GAIA framing helpers).
 *
 * Production opens a BluetoothSocket; unit tests inject [BenshiSocketFactory]
 * streams. Never opens the audio RFCOMM UUID. One-at-a-time command queue with
 * timeouts, retries, and reconnect that re-registers notifications.
 *
 * No contact with real Bluetooth devices from tests — inject fakes only.
 */
class BenshiRfcommTransport(
    private val socketFactory: BenshiSocketFactory,
    private val commandTimeoutMs: Long = 800,
    private val maxRetries: Int = 2,
) : Closeable {
    private val running = AtomicBoolean(false)
    private var socket: BenshiSocket? = null
    private var readerThread: Thread? = null
    private val rxFrames = ArrayBlockingQueue<ByteArray>(64)
    private val queueLock = Any()
    private val notificationTypes = mutableListOf<Byte>()

    fun connect() {
        if (running.get() && socket != null) return
        val s = socketFactory.openControlSocket()
        socket = s
        running.set(true)
        readerThread =
            thread(name = "benshi-rfcomm-rx", isDaemon = true) {
                val buf = ByteArray(4096)
                val acc = ArrayList<Byte>()
                try {
                    while (running.get()) {
                        val n = s.input.read(buf)
                        if (n < 0) break
                        for (i in 0 until n) acc.add(buf[i])
                        drainGaiaFrames(acc)
                    }
                } catch (_: Exception) {
                    // drop; reconnect path re-opens
                }
            }
    }

    /**
     * Send one GAIA body (unframed), wait for a reply frame. Retries on timeout.
     * Serialized: only one in-flight command.
     */
    fun transact(body: ByteArray): ByteArray {
        synchronized(queueLock) {
            var lastErr: Exception? = null
            repeat(maxRetries + 1) { attempt ->
                try {
                    ensureConnected()
                    val frame = gaiaEncode(body)
                    rxFrames.clear()
                    socket!!.output.write(frame)
                    socket!!.output.flush()
                    val reply = rxFrames.poll(commandTimeoutMs, TimeUnit.MILLISECONDS)
                    if (reply != null) return reply
                    lastErr =
                        java.io.IOException("benshi RFCOMM timeout after ${commandTimeoutMs}ms")
                    reconnect()
                } catch (e: Exception) {
                    lastErr = e
                    reconnect()
                }
                if (attempt < maxRetries) Thread.sleep(50)
            }
            throw lastErr ?: java.io.IOException("benshi RFCOMM failed")
        }
    }

    /** Remember notification types so reconnect can re-register them. */
    fun noteNotificationRegistration(type: Byte) {
        synchronized(notificationTypes) {
            if (!notificationTypes.contains(type)) notificationTypes.add(type)
        }
    }

    fun reconnect() {
        closeSocketOnly()
        connect()
        val regs = synchronized(notificationTypes) { notificationTypes.toList() }
        socketFactory.onReconnected(regs)
    }

    private fun ensureConnected() {
        if (!running.get() || socket == null) connect()
    }

    private fun drainGaiaFrames(acc: ArrayList<Byte>) {
        while (true) {
            val frame = tryExtractGaia(acc) ?: return
            rxFrames.offer(frame)
        }
    }

    override fun close() {
        running.set(false)
        closeSocketOnly()
        readerThread = null
    }

    private fun closeSocketOnly() {
        try {
            socket?.close()
        } catch (_: Exception) {
        }
        socket = null
        running.set(false)
    }

    companion object {
        /** Bluetooth Serial Port Profile UUID (control). */
        val SPP_UUID: UUID = UUID.fromString("00001101-0000-1000-8000-00805F9B34FB")

        /**
         * Audio RFCOMM UUID — must never be opened by Navi CAT.
         * Documented so static analysis / reviews can grep for accidental use.
         */
        val AUDIO_RFCOMM_UUID_DO_NOT_OPEN: UUID =
            UUID.fromString("39144315-32FA-40DB-85ED-FBFEBA2D86E6")

        private const val GAIA_SOF_BYTE: Byte = 0xFF.toByte()
        private const val GAIA_VER: Byte = 0x01

        /** Match `navi-cat` `gaia_encode`: FF 01 00 <payload_len> + body. */
        fun gaiaEncode(body: ByteArray): ByteArray {
            require(body.size >= 4) { "command body too short" }
            val payloadLen = body.size - 4
            require(payloadLen <= 0xFF) { "payload too long for GAIA len byte" }
            val out = ByteArray(4 + body.size)
            out[0] = GAIA_SOF_BYTE
            out[1] = GAIA_VER
            out[2] = 0x00
            out[3] = payloadLen.toByte()
            System.arraycopy(body, 0, out, 4, body.size)
            return out
        }

        fun tryExtractGaia(acc: ArrayList<Byte>): ByteArray? {
            if (acc.size < 8) return null
            var sof = -1
            for (i in 0 until acc.size - 1) {
                if (acc[i] == GAIA_SOF_BYTE && acc[i + 1] == GAIA_VER) {
                    sof = i
                    break
                }
            }
            if (sof < 0) {
                acc.clear()
                return null
            }
            if (sof > 0) {
                repeat(sof) { acc.removeAt(0) }
            }
            if (acc.size < 8) return null
            val hasCs = (acc[2].toInt() and 1) != 0
            val payloadLen = acc[3].toInt() and 0xFF
            val total = payloadLen + 8 + if (hasCs) 1 else 0
            if (acc.size < total) return null
            val body = ByteArray(4 + payloadLen)
            for (i in body.indices) body[i] = acc[4 + i]
            repeat(total) { acc.removeAt(0) }
            return body
        }
    }
}

/** Injectable socket (production BluetoothSocket wrapper or test fake). */
interface BenshiSocket : Closeable {
    val input: InputStream
    val output: OutputStream
}

interface BenshiSocketFactory {
    fun openControlSocket(): BenshiSocket

    /** Called after reconnect so the host can re-REGISTER_NOTIFICATION. */
    fun onReconnected(notificationTypes: List<Byte>) {}
}
