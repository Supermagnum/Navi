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
    fun isWifiOrEthernet(context: Context): Boolean {
        val cm =
            context.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
                ?: return false
        val network = cm.activeNetwork ?: return false
        val caps = cm.getNetworkCapabilities(network) ?: return false
        return caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) ||
            caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET)
    }
}
