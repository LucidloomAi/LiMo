package ai.lucidloom.limo.spike07

import android.app.Activity
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.util.Log
import android.util.TypedValue
import android.widget.ScrollView
import android.widget.TextView

/**
 * The test app's one screen: what it is, what the phone is, whether USB
 * debugging is on (as any app can read it), what the guarded door last
 * took, and how the last loopback run went. Started with the int extra
 * `loopback_port`, it runs the loopback client to that port first.
 */
class MainActivity : Activity() {
    private lateinit var text: TextView
    private val refresh: () -> Unit = { runOnUiThread { show() } }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        text = TextView(this).apply {
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 16f)
            val pad = (16 * resources.displayMetrics.density).toInt()
            setPadding(pad, pad, pad, pad)
        }
        setContentView(ScrollView(this).apply { addView(text) })
        if (savedInstanceState == null) handle(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handle(intent)
    }

    override fun onResume() {
        super.onResume()
        SpikeState.listen(refresh)
        show()
    }

    override fun onPause() {
        SpikeState.unlisten(refresh)
        super.onPause()
    }

    private fun handle(intent: Intent?) {
        val port = intent?.getIntExtra(EXTRA_PORT, 0) ?: 0
        if (port == 0) return
        Log.i(CableDoor.TAG, "loopback start port=$port")
        Thread({
            val result = Loopback.run(port)
            Log.i(CableDoor.TAG, "loopback ${if (result.ok) "ok" else "failed"} port=$port bytes=${result.bytes} ms=${result.ms} why=${result.why}")
            SpikeState.recordLoopback(result)
        }, "limo-spike07-loopback").start()
    }

    private fun usbDebugging(): String =
        when (Settings.Global.getInt(contentResolver, Settings.Global.ADB_ENABLED, -1)) {
            1 -> "on"
            0 -> "off"
            else -> "not readable"
        }

    private fun show() {
        val s = SpikeState.snapshot()
        val door = when (s.secret) {
            "taken" -> "took ${s.bytes} bytes, check ${s.check}, written by user id ${s.callerUid} (2000 is the adb shell)"
            "refused" -> "refused a write (${s.why}), by user id ${s.callerUid}"
            else -> "nothing written yet"
        }
        val loopback = when (s.loopback) {
            "ok" -> "every byte came back: ${s.loopbackBytes} bytes in ${s.loopbackMs} ms"
            "failed" -> "failed (${s.loopbackWhy}) after ${s.loopbackBytes} bytes"
            else -> "not run yet"
        }
        text.text = listOf(
            "LiMo spike 07",
            "",
            "A test app for LiMo's cable proof. It does nothing else, keeps nothing on the phone, and can be removed at any time.",
            "",
            "This phone: ${Build.MANUFACTURER} ${Build.MODEL}, Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT}).",
            "USB debugging: ${usbDebugging()}.",
            "",
            "The guarded door: $door.",
            "Writes seen since the app started: ${s.takes}.",
            "",
            "The loopback through the cable: $loopback.",
            "Runs since the app started: ${s.loopbacks}.",
        ).joinToString("\n")
    }

    companion object {
        const val EXTRA_PORT = "loopback_port"
    }
}
