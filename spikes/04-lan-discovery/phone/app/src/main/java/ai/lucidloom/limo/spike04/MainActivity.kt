package ai.lucidloom.limo.spike04

import android.app.Activity
import android.content.Intent
import android.graphics.Typeface
import android.net.nsd.NsdServiceInfo
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.text.InputType
import android.util.Log
import android.util.TypedValue
import android.view.View
import android.view.WindowInsets
import android.view.WindowManager
import android.view.inputmethod.InputMethodManager
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import java.net.InetAddress
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException

/**
 * The one screen. Plain framework views built in code.
 *
 * On start it searches for _limo-spike._tcp with NsdManager for 30 s,
 * resolves each instance, and sends each a HELLO. After the first hub is
 * found it repeats the HELLO every 5 s, 12 times (60 s).
 *
 * Intent extras, for the direct path with no discovery (see README.md):
 *   --es host 192.168.68.200 --ei port 47810
 * With these extras the automatic search is skipped; "Search again" still works.
 *
 * Every outcome writes one log line with the tag LiMoSpike04.
 */
class MainActivity : Activity(), Discovery.Events {

    private val main = Handler(Looper.getMainLooper())
    private val worker: ExecutorService = Executors.newSingleThreadExecutor()
    private val clock = SimpleDateFormat("HH:mm:ss.SSS", Locale.ROOT)

    private lateinit var discovery: Discovery
    private lateinit var wifi: WifiWatch

    private lateinit var wifiText: TextView
    private lateinit var searchText: TextView
    private lateinit var hubsText: TextView
    private lateinit var repeatText: TextView
    private lateinit var hostField: EditText
    private lateinit var portField: EditText
    private lateinit var directText: TextView
    private lateinit var logText: TextView

    private val logLines = ArrayDeque<String>()
    private var visible = false

    /** A direct HELLO from the extras waits for the first Wi-Fi report, so its socket can be bound to Wi-Fi. */
    private var directWaitsForWifi = false

    // the search

    private class Hub(val name: String, val foundMs: Long) {
        var resolvedMs = -1L
        var host = ""
        var port = 0
        var id = "?"
        var status = "resolving..."
    }

    private var search = 0
    private var searchStart = 0L
    private var searchActive = false
    private val hubs = LinkedHashMap<String, Hub>()
    private var repeatStartedThisSearch = false

    // the stability repeat

    private class Repeat(val address: InetAddress, val port: Int, val startMs: Long) {
        val tally = RepeatTally()
        var sent = 0
    }

    private var repeat: Repeat? = null
    private var repeatSummary = "starts after the first hub is found"

    private val searchTimeout = Runnable { stopSearch("time_up") }
    private val ticker = object : Runnable {
        override fun run() {
            refresh()
            main.postDelayed(this, 1000L)
        }
    }

    // lifecycle

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(buildUi())
        discovery = Discovery(this, main, this)
        wifi = WifiWatch(this, main) { onWifi(it) }
        log("app start android=${Build.VERSION.SDK_INT} model=${Lines.quote(Build.MODEL ?: "?")}")
        wifi.start()
        if (applyExtras(intent)) {
            log(Lines.searchSkipped("direct_extras"))
        } else {
            startSearch()
        }
        refresh()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        applyExtras(intent)
    }

    override fun onStart() {
        super.onStart()
        visible = true
        main.removeCallbacks(ticker)
        main.post(ticker)
    }

    override fun onStop() {
        visible = false
        main.removeCallbacks(ticker)
        // NsdManager's advice: do not keep searching in the background.
        stopSearch("app_hidden")
        stopRepeat("app_hidden")
        super.onStop()
    }

    override fun onDestroy() {
        discovery.stop()
        wifi.stop()
        worker.shutdownNow()
        main.removeCallbacksAndMessages(null)
        super.onDestroy()
    }

    // intent extras: the direct path

    /** True if the extras asked for a direct HELLO (sent now, or as soon as the Wi-Fi state is known). */
    private fun applyExtras(intent: Intent?): Boolean {
        if (intent == null) return false
        if ((intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY) != 0) return false
        if (!intent.hasExtra("host")) return false
        val host = intent.getStringExtra("host") ?: ""
        val port = when {
            !intent.hasExtra("port") -> Protocol.DEFAULT_PORT.toString()
            intent.getIntExtra("port", -1) != -1 -> intent.getIntExtra("port", -1).toString()
            else -> intent.getStringExtra("port") ?: ""
        }
        log("intent host=${Lines.quote(host)} port=${Lines.quote(port)}")
        // Applied once. Do not apply again if the activity is recreated.
        intent.removeExtra("host")
        intent.removeExtra("port")
        hostField.setText(host)
        portField.setText(port)
        if (wifi.state.known) {
            sendDirect()
        } else {
            // Just started: the Wi-Fi report comes within 1.5 s (see WifiWatch).
            directWaitsForWifi = true
            directText.text = "Waiting for the Wi-Fi state, then sending HELLO..."
        }
        return true
    }

    private fun sendDirect() {
        val rawHost = hostField.text.toString().trim()
        val host = Protocol.checkHost(rawHost)
        val port = Protocol.parsePort(portField.text.toString())
        if (host == null || port == null) {
            val why = if (host == null) "the address is empty or has spaces in it"
            else "the port must be a whole number from 1 to 65535"
            log(Lines.helloFailed(rawHost.ifEmpty { "none" }, port ?: 0, "direct", "none", why))
            directText.text = "Not sent: $why."
            return
        }
        hideKeyboard()
        directText.text = "Sending HELLO to $host port $port..."
        val net = wifi.state.network
        submit {
            val address = try {
                net?.getByName(host) ?: InetAddress.getByName(host)
            } catch (e: Exception) {
                val error = HelloClient.words(e, "connect")
                main.post {
                    log(Lines.helloFailed(host, port, "direct", if (net != null) "wifi" else "default", error))
                    directText.text = "Failed: $error"
                }
                return@submit
            }
            val outcome = HelloClient.send(address, port, net)
            main.post {
                logOutcome(address, port, "direct", outcome)
                directText.text = describe(outcome)
            }
        }
    }

    // the search

    private fun startSearch() {
        stopSearch("search_again")
        stopRepeat("search_again")
        hubs.clear()
        repeatStartedThisSearch = false
        repeatSummary = "starts after the first hub is found"
        log(Lines.searchStarted())
        searchStart = SystemClock.elapsedRealtime()
        search = discovery.start()
        discovery.dropStale(search)
        searchActive = true
        main.removeCallbacks(searchTimeout)
        main.postDelayed(searchTimeout, Protocol.SEARCH_MS)
        keepScreenOnWhileBusy()
        refresh()
    }

    private fun stopSearch(reason: String) {
        main.removeCallbacks(searchTimeout)
        if (!searchActive) return
        searchActive = false
        discovery.stop()
        log(Lines.searchStopped(hubs.size, sinceSearch(), reason))
        keepScreenOnWhileBusy()
        refresh()
    }

    private fun sinceSearch(): Long = SystemClock.elapsedRealtime() - searchStart

    override fun onSearchRunning(search: Int) {
        if (search == this.search) log(Lines.searchRunning(sinceSearch()))
    }

    override fun onSearchFailed(search: Int, error: String) {
        log(Lines.searchFailed(error))
        if (search == this.search && searchActive) {
            searchActive = false
            main.removeCallbacks(searchTimeout)
            keepScreenOnWhileBusy()
            refresh()
        }
    }

    override fun onSearchEnded(search: Int) {
        // A stop the app asked for was logged when it was asked for. This is
        // a stop nobody asked for.
        if (search == this.search && searchActive) {
            searchActive = false
            main.removeCallbacks(searchTimeout)
            log(Lines.searchStopped(hubs.size, sinceSearch(), "system"))
            keepScreenOnWhileBusy()
            refresh()
        }
    }

    override fun onStopFailed(search: Int, error: String) {
        log("search stop failed error=${Lines.quote(error)}")
    }

    override fun onFound(search: Int, info: NsdServiceInfo) {
        val name = info.serviceName ?: "?"
        if (search != this.search) {
            log("found ignored name=${Lines.quote(name)} reason=old_search")
            return
        }
        val after = sinceSearch()
        if (hubs.containsKey(name)) {
            log(Lines.foundAgain(name, after))
            return
        }
        hubs[name] = Hub(name, after)
        log(Lines.found(name, after))
        discovery.resolve(search, info)
        refresh()
    }

    override fun onLost(search: Int, name: String) {
        if (search != this.search) return
        log(Lines.lost(name, sinceSearch()))
        hubs[name]?.status = "lost (the PC withdrew it, or it timed out)"
        refresh()
    }

    override fun onResolved(search: Int, name: String, addresses: List<InetAddress>, port: Int, id: String) {
        if (search != this.search) {
            log("resolved ignored name=${Lines.quote(name)} reason=old_search")
            return
        }
        val hub = hubs.getOrPut(name) { Hub(name, -1L) }
        val address = Protocol.pickAddress(addresses)
        hub.resolvedMs = sinceSearch()
        hub.host = address?.hostAddress ?: "none"
        hub.port = port
        hub.id = id
        log(Lines.resolved(name, hub.host, port, id, hub.resolvedMs))
        if (addresses.size > 1) {
            log("resolved addresses name=${Lines.quote(name)} all=${addresses.joinToString(",") { it.hostAddress ?: "?" }}")
        }
        if (address == null) {
            val error = "Android resolved the service but gave no address"
            log(Lines.helloFailed("none", port, "found", "none", error))
            hub.status = error
            refresh()
            return
        }
        hub.status = "sending HELLO..."
        refresh()
        sendHello(address, port, "found") { outcome ->
            hub.status = describe(outcome)
            if (search == this.search && !repeatStartedThisSearch && repeat == null && visible) {
                startRepeat(address, port)
            }
            refresh()
        }
    }

    override fun onResolveFailed(search: Int, name: String, error: String) {
        log(Lines.resolveFailed(name, error))
        if (search == this.search) {
            hubs[name]?.status = "resolve failed: $error"
            refresh()
        }
    }

    // HELLO

    private fun submit(task: () -> Unit) {
        try {
            worker.execute(task)
        } catch (e: RejectedExecutionException) {
            log("hello not sent: the app is closing")
        }
    }

    private fun sendHello(address: InetAddress, port: Int, run: String, then: (HelloClient.Outcome) -> Unit) {
        val net = wifi.state.network
        submit {
            val outcome = HelloClient.send(address, port, net)
            main.post {
                logOutcome(address, port, run, outcome)
                then(outcome)
            }
        }
    }

    private fun logOutcome(address: InetAddress, port: Int, run: String, o: HelloClient.Outcome) {
        val host = address.hostAddress ?: address.toString()
        if (o.ok) {
            log(Lines.helloOk(host, port, run, o.net, Protocol.msOneDecimal(o.rttNanos), Protocol.msOneDecimal(o.connectNanos), o.reply))
        } else {
            log(Lines.helloFailed(host, port, run, o.net, o.error))
        }
    }

    private fun describe(o: HelloClient.Outcome): String =
        if (o.ok) "HELLO ok in ${Protocol.msOneDecimal(o.rttNanos)} ms (connect ${Protocol.msOneDecimal(o.connectNanos)} ms): ${o.reply}"
        else "HELLO failed: ${o.error}"

    // the stability repeat

    private fun startRepeat(address: InetAddress, port: Int) {
        val r = Repeat(address, port, SystemClock.elapsedRealtime())
        repeat = r
        repeatStartedThisSearch = true
        log(Lines.repeatStarted(address.hostAddress ?: "?", port))
        keepScreenOnWhileBusy()
        scheduleNext(r)
    }

    private fun scheduleNext(r: Repeat) {
        val n = r.sent + 1
        val due = r.startMs + n * Protocol.REPEAT_EVERY_MS
        val wait = (due - SystemClock.elapsedRealtime()).coerceAtLeast(0L)
        main.postDelayed({
            if (repeat === r) {
                r.sent = n
                sendHello(r.address, r.port, "repeat_$n") { outcome -> onRepeatResult(r, outcome) }
            }
        }, wait)
    }

    private fun onRepeatResult(r: Repeat, o: HelloClient.Outcome) {
        if (repeat !== r) return
        r.tally.record(o.ok, o.rttNanos)
        if (r.tally.done) {
            log(r.tally.doneLine())
            repeatSummary = "done: ok ${r.tally.ok}, failed ${r.tally.failed}, slowest ${r.tally.slowestMs()} ms"
            repeat = null
            keepScreenOnWhileBusy()
        } else {
            scheduleNext(r)
        }
        refresh()
    }

    private fun stopRepeat(reason: String) {
        val r = repeat ?: return
        repeat = null
        log(r.tally.stoppedLine(reason))
        repeatSummary = "stopped ($reason) after ${r.tally.count}: ok ${r.tally.ok}, failed ${r.tally.failed}"
        keepScreenOnWhileBusy()
    }

    // Wi-Fi

    private fun onWifi(state: WifiWatch.State) {
        log(Lines.wifi(state.connected, state.addresses))
        if (directWaitsForWifi) {
            directWaitsForWifi = false
            sendDirect()
        }
        refresh()
    }

    // log and screen

    private fun log(line: String) {
        Log.i(TAG, line)
        logLines.addFirst("${clock.format(Date())} $line")
        while (logLines.size > MAX_SHOWN_LINES) logLines.removeLast()
        if (::logText.isInitialized) logText.text = logLines.joinToString("\n")
    }

    /** The screen stays on while a search or the repeat runs, so a short screen timeout cannot cut them off. */
    private fun keepScreenOnWhileBusy() {
        if (searchActive || repeat != null) {
            window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        } else {
            window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        }
    }

    private fun refresh() {
        if (!::wifiText.isInitialized || !::wifi.isInitialized) return
        val w = wifi.state
        wifiText.text = when {
            !w.known -> "Wi-Fi: checking..."
            w.connected -> "Wi-Fi: connected. This phone: ${w.addresses.joinToString(", ").ifEmpty { "no address yet" }}"
            else -> "Wi-Fi: not connected. The phone must be on the same Wi-Fi as the PC."
        }
        searchText.text = when {
            searchActive -> "Search: running, ${sinceSearch() / 1000} s of ${Protocol.SEARCH_MS / 1000}. Found ${hubs.size}."
            search == 0 -> "Search: not started. Tap \"Search again\"."
            else -> "Search: stopped. Found ${hubs.size}."
        }
        hubsText.text = if (hubs.isEmpty()) {
            "No hub found yet."
        } else {
            hubs.values.joinToString("\n\n") { h ->
                val where = if (h.resolvedMs >= 0) "${h.host} port ${h.port}, id ${h.id}" else "not resolved yet"
                val times = "found after ${h.foundMs} ms" +
                    if (h.resolvedMs >= 0) ", resolved after ${h.resolvedMs} ms" else ""
                "${h.name}\n$where\n$times\n${h.status}"
            }
        }
        val r = repeat
        repeatText.text = if (r != null) {
            "Repeat: ${r.tally.count} of ${r.tally.total} done, ok ${r.tally.ok}, failed ${r.tally.failed}, slowest ${r.tally.slowestMs()} ms"
        } else {
            "Repeat: $repeatSummary"
        }
    }

    private fun hideKeyboard() {
        val imm = getSystemService(InputMethodManager::class.java)
        imm?.hideSoftInputFromWindow(hostField.windowToken, 0)
    }

    // views

    private fun buildUi(): View {
        val scroll = ScrollView(this)
        val col = LinearLayout(this)
        col.orientation = LinearLayout.VERTICAL
        val pad = dp(16)
        col.setPadding(pad, pad, pad, pad)
        scroll.addView(col)

        col.addView(heading("LiMo spike 04: find the PC"))
        wifiText = body()
        col.addView(wifiText)

        col.addView(heading("Search"))
        col.addView(button("Search again") { startSearch() })
        searchText = body()
        col.addView(searchText)
        hubsText = body()
        hubsText.setTypeface(Typeface.MONOSPACE, Typeface.NORMAL)
        col.addView(hubsText)
        repeatText = body()
        col.addView(repeatText)

        col.addView(heading("Direct HELLO (no discovery)"))
        hostField = EditText(this)
        hostField.hint = "PC address, for example 192.168.68.200"
        hostField.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
        hostField.isSingleLine = true
        col.addView(hostField)
        portField = EditText(this)
        portField.hint = "Port"
        portField.inputType = InputType.TYPE_CLASS_NUMBER
        portField.isSingleLine = true
        portField.setText(Protocol.DEFAULT_PORT.toString())
        col.addView(portField)
        col.addView(button("Send HELLO to this address") { sendDirect() })
        directText = body()
        directText.text = "Not sent yet."
        col.addView(directText)

        col.addView(heading("Log (newest first)"))
        logText = body()
        logText.setTypeface(Typeface.MONOSPACE, Typeface.NORMAL)
        logText.setTextSize(TypedValue.COMPLEX_UNIT_SP, 11f)
        logText.setTextIsSelectable(true)
        col.addView(logText)

        // Android 15 and later draw the app edge to edge: keep the content
        // clear of the status bar, the navigation bar and the keyboard.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            scroll.setOnApplyWindowInsetsListener { v, insets ->
                val bars = insets.getInsets(
                    WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout() or WindowInsets.Type.ime()
                )
                v.setPadding(bars.left, bars.top, bars.right, bars.bottom)
                insets
            }
        }
        return scroll
    }

    private fun heading(text: String): TextView {
        val t = TextView(this)
        t.text = text
        t.setTextSize(TypedValue.COMPLEX_UNIT_SP, 20f)
        t.setTypeface(t.typeface, Typeface.BOLD)
        t.setPadding(0, dp(16), 0, dp(8))
        return t
    }

    private fun body(): TextView {
        val t = TextView(this)
        t.setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
        t.setPadding(0, dp(4), 0, dp(4))
        return t
    }

    private fun button(text: String, onClick: () -> Unit): Button {
        val b = Button(this)
        b.text = text
        b.isAllCaps = false
        b.setOnClickListener { onClick() }
        return b
    }

    private fun dp(v: Int): Int =
        TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v.toFloat(), resources.displayMetrics).toInt()

    companion object {
        const val TAG = "LiMoSpike04"
        const val MAX_SHOWN_LINES = 80
    }
}
