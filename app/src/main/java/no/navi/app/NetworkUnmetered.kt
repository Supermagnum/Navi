package no.navi.app

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities

/**
 * Unmetered Wi‑Fi / Ethernet check shared by DATEX and long-trip pack downloads.
 *
 * Ordinary Tools region downloads must not call this — long-trip mode only.
 */
object NetworkUnmetered {
    /**
     * Host-test override. When non-null, [isWifiOrEthernet] returns this value and
     * never touches [Context] (so JVM unit tests can exercise the long-trip gate).
     */
    @Volatile
    internal var forceForTests: Boolean? = null

    fun isWifiOrEthernet(context: Context): Boolean {
        forceForTests?.let { return it }
        val cm =
            context.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
                ?: return false
        val network = cm.activeNetwork ?: return false
        val caps = cm.getNetworkCapabilities(network) ?: return false
        return caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) ||
            caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET)
    }
}
