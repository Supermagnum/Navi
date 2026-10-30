package no.navi.app.cat

import android.Manifest
import android.annotation.SuppressLint
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.content.ContextCompat
import uniffi.navi.BenshiBytePipe
import uniffi.navi.BenshiPipeException
import java.io.IOException

/** A device bonded in Android Bluetooth settings. */
data class BenshiBondedDevice(
    val name: String,
    val address: String,
    /** Cached SDP record lists the SPP control UUID. */
    val advertisesSpp: Boolean,
)

/** Real-radio Benshi path over a bonded device's SPP control RFCOMM channel. */
object BenshiBluetooth {
    /**
     * Runtime permissions needed for bonded-device RFCOMM. API 30 and below use
     * install-time `BLUETOOTH`; no scan permission because only bonded devices are used.
     */
    fun runtimePermissions(sdkInt: Int = Build.VERSION.SDK_INT): Array<String> =
        if (sdkInt >= Build.VERSION_CODES.S) {
            arrayOf(Manifest.permission.BLUETOOTH_CONNECT)
        } else {
            emptyArray()
        }

    fun hasPermission(context: Context): Boolean =
        runtimePermissions().all {
            ContextCompat.checkSelfPermission(context, it) == PackageManager.PERMISSION_GRANTED
        }

    /** Null when Bluetooth is missing or switched off. */
    @SuppressLint("MissingPermission")
    fun bondedDevices(context: Context): List<BenshiBondedDevice>? {
        if (!hasPermission(context)) return null
        val adapter =
            context.getSystemService(BluetoothManager::class.java)?.adapter ?: return null
        if (!adapter.isEnabled) return null
        return adapter.bondedDevices
            .orEmpty()
            .map { d ->
                BenshiBondedDevice(
                    name = d.name.orEmpty().ifBlank { d.address },
                    address = d.address,
                    advertisesSpp =
                        d.uuids.orEmpty().any { it.uuid == BenshiRfcommTransport.SPP_UUID },
                )
            }.sortedWith(compareByDescending<BenshiBondedDevice> { it.advertisesSpp }.thenBy { it.name })
    }

    /** Bonded device for [address]; throws [IOException] with a user-facing reason. */
    @SuppressLint("MissingPermission")
    fun bondedDevice(
        context: Context,
        address: String,
    ): BluetoothDevice {
        if (!hasPermission(context)) throw IOException("Bluetooth permission denied")
        val adapter =
            context.getSystemService(BluetoothManager::class.java)?.adapter
                ?: throw IOException("no Bluetooth adapter")
        if (!adapter.isEnabled) throw IOException("Bluetooth is off")
        return adapter.bondedDevices.orEmpty().firstOrNull { it.address.equals(address, ignoreCase = true) }
            ?: throw IOException("$address is not paired")
    }

    /** Open the SPP control socket on [device] and wrap it as a native pipe. */
    fun openPipe(device: BluetoothDevice): BenshiRfcommPipe {
        val transport = BenshiRfcommTransport(BluetoothBenshiSocketFactory(device))
        transport.connect()
        return BenshiRfcommPipe(transport)
    }
}

/** SPP control UUID only; the audio RFCOMM UUID is never opened. */
class BluetoothBenshiSocketFactory(
    private val device: BluetoothDevice,
) : BenshiSocketFactory {
    @SuppressLint("MissingPermission")
    override fun openControlSocket(): BenshiSocket {
        val s = device.createRfcommSocketToServiceRecord(BenshiRfcommTransport.SPP_UUID)
        try {
            s.connect()
        } catch (e: IOException) {
            runCatching { s.close() }
            throw e
        }
        return object : BenshiSocket {
            override val input = s.inputStream
            override val output = s.outputStream

            override fun close() = s.close()
        }
    }
}

/** Raw byte pipe for `catPluginConnectBenshi`; GAIA framing is done in `navi-cat`. */
class BenshiRfcommPipe(
    private val transport: BenshiRfcommTransport,
) : BenshiBytePipe {
    override fun writeBytes(bytes: ByteArray) {
        try {
            transport.writeRaw(bytes)
        } catch (e: IOException) {
            throw BenshiPipeException.Io(e.message ?: "RFCOMM write failed")
        }
    }

    override fun readBytes(timeoutMs: UInt): ByteArray =
        try {
            transport.readRaw(timeoutMs.toLong())
        } catch (e: IOException) {
            throw BenshiPipeException.Io(e.message ?: "RFCOMM read failed")
        }

    override fun shutdown() = transport.close()
}
