package ai.lucidloom.limo.spike04

import android.content.Context
import android.net.ConnectivityManager
import android.net.LinkProperties
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.os.Handler
import java.net.Inet4Address

/**
 * Follows the Wi-Fi network: whether there is one, its Network object (the
 * HELLO socket is bound to it), and the phone's own addresses on it. Needs
 * only ACCESS_NETWORK_STATE. Callbacks come on the main thread.
 */
class WifiWatch(context: Context, private val main: Handler, private val onChange: (State) -> Unit) {

    data class State(val known: Boolean, val network: Network?, val addresses: List<String>) {
        val connected: Boolean get() = network != null
    }

    private val cm: ConnectivityManager = context.getSystemService(ConnectivityManager::class.java)
    var state = State(known = false, network = null, addresses = emptyList())
        private set

    private val callback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            set(network, cm.getLinkProperties(network))
        }

        override fun onLinkPropertiesChanged(network: Network, linkProperties: LinkProperties) {
            set(network, linkProperties)
        }

        override fun onLost(network: Network) {
            if (state.network == network) {
                state = State(true, null, emptyList())
                onChange(state)
            }
        }
    }

    private val noWifiYet = Runnable {
        if (!state.known) {
            state = State(true, null, emptyList())
            onChange(state)
        }
    }

    fun start() {
        val request = NetworkRequest.Builder()
            .addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
            .removeCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            .build()
        cm.registerNetworkCallback(request, callback, main)
        // If Wi-Fi is up, onAvailable comes almost at once. If nothing came
        // by then, there is no Wi-Fi.
        main.postDelayed(noWifiYet, 1500L)
    }

    fun stop() {
        main.removeCallbacks(noWifiYet)
        try {
            cm.unregisterNetworkCallback(callback)
        } catch (notRegistered: IllegalArgumentException) {
            // nothing to undo
        }
    }

    private fun set(network: Network, lp: LinkProperties?) {
        val all = lp?.linkAddresses.orEmpty()
        val v4 = all.filter { it.address is Inet4Address }
        val shown = (v4.ifEmpty { all }).map { "${it.address.hostAddress}/${it.prefixLength}" }
        val next = State(true, network, shown)
        if (next != state) {
            state = next
            onChange(state)
        }
    }
}
