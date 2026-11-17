package ai.lucidloom.limo.spike04

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import android.os.Handler
import android.os.SystemClock
import java.net.InetAddress

/**
 * Android's own NsdManager, with its known quirks handled:
 *
 * - A listener object may not be reused while it is registered. Every search
 *   gets a new DiscoveryListener, and every resolve a new ResolveListener.
 * - Before Android 14, only one resolve may run at a time; a second one fails
 *   with code 3 (already active). Resolves are queued and run one by one. If
 *   code 3 comes back anyway, the resolve is tried again after 1 s, at most 3
 *   times in all.
 * - A resolve that never calls back would stall the queue, so after 10 s it
 *   is given up (reported as a time out) and the queue moves on.
 * - Callbacks arrive on NsdManager's own thread on older versions. Each one
 *   is posted to the main thread, which owns all state.
 * - Each search has a number. Callbacks from an earlier search still arrive
 *   after "Search again"; the activity uses the number to tell them apart.
 *
 * Every method must be called on the main thread.
 */
class Discovery(context: Context, private val main: Handler, private val events: Events) {

    interface Events {
        fun onSearchRunning(search: Int)
        fun onSearchFailed(search: Int, error: String)
        fun onSearchEnded(search: Int)
        fun onStopFailed(search: Int, error: String)
        fun onFound(search: Int, info: NsdServiceInfo)
        fun onLost(search: Int, name: String)
        fun onResolved(search: Int, name: String, addresses: List<InetAddress>, port: Int, id: String)
        fun onResolveFailed(search: Int, name: String, error: String)
    }

    private val nsd: NsdManager = context.getSystemService(NsdManager::class.java)
    private var search = 0
    private var listener: NsdManager.DiscoveryListener? = null

    private class Pending(val search: Int, val info: NsdServiceInfo, val tries: Int)

    private val queue = ArrayDeque<Pending>()
    private var inFlight: Pending? = null
    private var inFlightToken = 0
    private var holdUntil = 0L

    val running: Boolean get() = listener != null

    /** Starts a new search (stopping the one before) and returns its number. */
    fun start(): Int {
        stop()
        val s = ++search
        val l = object : NsdManager.DiscoveryListener {
            override fun onDiscoveryStarted(serviceType: String) {
                main.post { events.onSearchRunning(s) }
            }

            override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {
                main.post {
                    if (listener === this) listener = null
                    events.onSearchFailed(s, Protocol.nsdError(errorCode))
                }
            }

            override fun onDiscoveryStopped(serviceType: String) {
                main.post { events.onSearchEnded(s) }
            }

            override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) {
                main.post { events.onStopFailed(s, Protocol.nsdError(errorCode)) }
            }

            override fun onServiceFound(serviceInfo: NsdServiceInfo) {
                main.post { events.onFound(s, serviceInfo) }
            }

            override fun onServiceLost(serviceInfo: NsdServiceInfo) {
                val name = serviceInfo.serviceName ?: "?"
                main.post { events.onLost(s, name) }
            }
        }
        listener = l
        try {
            nsd.discoverServices(Protocol.SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, l)
        } catch (e: RuntimeException) {
            listener = null
            main.post { events.onSearchFailed(s, "${e.javaClass.simpleName}: ${e.message}") }
        }
        return s
    }

    /** Stops the running search, if any. Its onSearchEnded follows. */
    fun stop() {
        val l = listener ?: return
        listener = null
        try {
            nsd.stopServiceDiscovery(l)
        } catch (e: RuntimeException) {
            val s = search
            main.post { events.onStopFailed(s, "${e.javaClass.simpleName}: ${e.message}") }
        }
    }

    /** Queues a resolve for a service found by search number [s]. */
    fun resolve(s: Int, info: NsdServiceInfo) {
        queue.addLast(Pending(s, info, 1))
        pump()
    }

    /** Drops queued resolves of earlier searches, reporting each. */
    fun dropStale(current: Int) {
        val keep = queue.filter { it.search == current }
        val gone = queue.filter { it.search != current }
        queue.clear()
        queue.addAll(keep)
        for (p in gone) events.onResolveFailed(p.search, p.info.serviceName ?: "?", "skipped: a new search started")
    }

    private fun pump() {
        if (inFlight != null) return
        if (SystemClock.elapsedRealtime() < holdUntil) return
        val next = queue.removeFirstOrNull() ?: return
        inFlight = next
        val token = ++inFlightToken
        val name = next.info.serviceName ?: "?"

        val rl = object : NsdManager.ResolveListener {
            override fun onResolveFailed(serviceInfo: NsdServiceInfo, errorCode: Int) {
                main.post { resolveFailed(token, next, errorCode) }
            }

            override fun onServiceResolved(serviceInfo: NsdServiceInfo) {
                main.post { resolved(token, next, serviceInfo) }
            }
        }
        try {
            // Deprecated in Android 14 in favour of registerServiceInfoCallback.
            // The test phone runs Android 13, where this is the only way.
            @Suppress("DEPRECATION")
            nsd.resolveService(next.info, rl)
        } catch (e: RuntimeException) {
            inFlight = null
            events.onResolveFailed(next.search, name, "${e.javaClass.simpleName}: ${e.message}")
            pump()
            return
        }
        main.postDelayed({
            if (inFlightToken == token && inFlight === next) {
                inFlight = null
                events.onResolveFailed(next.search, name, "timed out: no answer from Android within ${RESOLVE_TIMEOUT_MS / 1000} s")
                pump()
            }
        }, RESOLVE_TIMEOUT_MS)
    }

    private fun resolveFailed(token: Int, p: Pending, code: Int) {
        if (token != inFlightToken || inFlight !== p) return // already given up
        inFlight = null
        val name = p.info.serviceName ?: "?"
        if (code == 3 && p.tries < 3) {
            events.onResolveFailed(p.search, name, "${Protocol.nsdError(code)}, trying again in 1 s")
            queue.addFirst(Pending(p.search, p.info, p.tries + 1))
            holdUntil = SystemClock.elapsedRealtime() + 1000L
            main.postDelayed({ pump() }, 1000L)
        } else {
            events.onResolveFailed(p.search, name, Protocol.nsdError(code))
            pump()
        }
    }

    private fun resolved(token: Int, p: Pending, info: NsdServiceInfo) {
        if (token != inFlightToken || inFlight !== p) return // already given up
        inFlight = null
        val addresses: List<InetAddress> = if (Build.VERSION.SDK_INT >= 34) {
            info.hostAddresses
        } else {
            @Suppress("DEPRECATION")
            listOfNotNull(info.host)
        }
        val id = Protocol.txtId(info.attributes)
        events.onResolved(p.search, info.serviceName ?: "?", addresses, info.port, id)
        pump()
    }

    companion object {
        const val RESOLVE_TIMEOUT_MS = 10_000L
    }
}
