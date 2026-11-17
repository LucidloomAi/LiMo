package ai.lucidloom.limo.spike05

import android.util.Log
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Every log line goes to logcat with the tag [TAG], and the newest ones are
 * kept in memory for the screen. Safe to call from any thread.
 */
object SpikeLog {

    const val TAG = "LiMoSpike05"
    private const val KEEP = 60

    private val recent = ArrayDeque<String>()

    /** How many lines were ever added, so the screen knows when to redraw. */
    @Volatile
    var added = 0L
        private set

    fun i(msg: String) {
        Log.i(TAG, msg)
        remember(msg)
    }

    fun w(msg: String) {
        Log.w(TAG, msg)
        remember(msg)
    }

    /** Newest first. */
    fun recentLines(): List<String> = synchronized(recent) { recent.toList() }

    private fun remember(msg: String) {
        val stamp = SimpleDateFormat("HH:mm:ss", Locale.US).format(Date())
        synchronized(recent) {
            recent.addFirst("$stamp $msg")
            while (recent.size > KEEP) recent.removeLast()
            added += 1
        }
    }
}
