package no.navi.app

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.hardware.usb.UsbAccessory
import android.hardware.usb.UsbManager
import android.os.Build
import android.os.ParcelFileDescriptor
import java.io.FileOutputStream
import java.nio.charset.StandardCharsets
import java.util.concurrent.atomic.AtomicReference

/**
 * Host-owned USB accessory (AOA) writer for instrument-cluster NDJSON.
 *
 * When the sink is `usb_ndjson`, [ClusterPluginRuntime] drains lines and calls
 * [writeLines]. Permission dialogs stay in the host; guests never see USB.
 */
object ClusterUsbExporter {
    private const val ACTION_USB_PERMISSION = "no.navi.app.USB_CLUSTER_PERMISSION"

    private val accessory = AtomicReference<UsbAccessory?>(null)
    private val pfd = AtomicReference<ParcelFileDescriptor?>(null)
    private val out = AtomicReference<FileOutputStream?>(null)
    private val status = AtomicReference("USB: idle")

    fun statusLine(): String = status.get()

    fun attach(context: Context) {
        val app = context.applicationContext
        val filter =
            IntentFilter().apply {
                addAction(UsbManager.ACTION_USB_ACCESSORY_ATTACHED)
                addAction(UsbManager.ACTION_USB_ACCESSORY_DETACHED)
                addAction(ACTION_USB_PERMISSION)
            }
        if (Build.VERSION.SDK_INT >= 33) {
            app.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            @Suppress("UnspecifiedRegisterReceiverFlag")
            app.registerReceiver(receiver, filter)
        }
        // Try any already-attached accessory.
        openFirstAccessory(app)
    }

    fun detach(context: Context) {
        try {
            context.applicationContext.unregisterReceiver(receiver)
        } catch (_: Exception) {
        }
        close()
    }

    fun writeLines(lines: List<String>): Int {
        if (lines.isEmpty()) return 0
        val stream = out.get() ?: return 0
        var written = 0
        try {
            for (line in lines) {
                val bytes = (line + "\n").toByteArray(StandardCharsets.UTF_8)
                stream.write(bytes)
                written += 1
            }
            stream.flush()
            status.set("USB: wrote $written line(s)")
        } catch (e: Exception) {
            status.set("USB write failed: ${e.message}")
            close()
        }
        return written
    }

    private fun openFirstAccessory(context: Context) {
        val usb = context.getSystemService(Context.USB_SERVICE) as? UsbManager ?: return
        val list = usb.accessoryList ?: return
        for (acc in list) {
            requestOrOpen(context, usb, acc)
            break
        }
    }

    private fun requestOrOpen(
        context: Context,
        usb: UsbManager,
        acc: UsbAccessory,
    ) {
        if (usb.hasPermission(acc)) {
            openAccessory(usb, acc)
            return
        }
        val flags =
            if (Build.VERSION.SDK_INT >= 31) {
                PendingIntent.FLAG_MUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
            } else {
                PendingIntent.FLAG_UPDATE_CURRENT
            }
        val pi =
            PendingIntent.getBroadcast(
                context,
                0,
                Intent(ACTION_USB_PERMISSION),
                flags,
            )
        status.set("USB: requesting permission")
        usb.requestPermission(acc, pi)
    }

    private fun openAccessory(
        usb: UsbManager,
        acc: UsbAccessory,
    ) {
        close()
        val fd =
            try {
                usb.openAccessory(acc)
            } catch (e: Exception) {
                status.set("USB open failed: ${e.message}")
                return
            }
        if (fd == null) {
            status.set("USB open failed: null fd")
            return
        }
        accessory.set(acc)
        pfd.set(fd)
        out.set(FileOutputStream(fd.fileDescriptor))
        status.set("USB: connected ${acc.model ?: acc.manufacturer ?: "accessory"}")
    }

    private fun close() {
        try {
            out.getAndSet(null)?.close()
        } catch (_: Exception) {
        }
        try {
            pfd.getAndSet(null)?.close()
        } catch (_: Exception) {
        }
        accessory.set(null)
        status.set("USB: idle")
    }

    private val receiver =
        object : BroadcastReceiver() {
            override fun onReceive(
                context: Context,
                intent: Intent,
            ) {
                val usb = context.getSystemService(Context.USB_SERVICE) as? UsbManager ?: return
                when (intent.action) {
                    UsbManager.ACTION_USB_ACCESSORY_ATTACHED -> {
                        val acc =
                            if (Build.VERSION.SDK_INT >= 33) {
                                intent.getParcelableExtra(
                                    UsbManager.EXTRA_ACCESSORY,
                                    UsbAccessory::class.java,
                                )
                            } else {
                                @Suppress("DEPRECATION")
                                intent.getParcelableExtra(UsbManager.EXTRA_ACCESSORY)
                            }
                        if (acc != null) requestOrOpen(context, usb, acc)
                    }
                    UsbManager.ACTION_USB_ACCESSORY_DETACHED -> close()
                    ACTION_USB_PERMISSION -> {
                        val granted = intent.getBooleanExtra(UsbManager.EXTRA_PERMISSION_GRANTED, false)
                        val acc =
                            if (Build.VERSION.SDK_INT >= 33) {
                                intent.getParcelableExtra(
                                    UsbManager.EXTRA_ACCESSORY,
                                    UsbAccessory::class.java,
                                )
                            } else {
                                @Suppress("DEPRECATION")
                                intent.getParcelableExtra(UsbManager.EXTRA_ACCESSORY)
                            }
                        if (granted && acc != null) {
                            openAccessory(usb, acc)
                        } else {
                            status.set("USB: permission denied")
                        }
                    }
                }
            }
        }
}
