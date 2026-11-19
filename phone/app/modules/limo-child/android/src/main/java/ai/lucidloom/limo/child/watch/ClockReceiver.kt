package ai.lucidloom.limo.child.watch

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Told by Android when the clock was set (`TIME_SET`, also sent when the 12
 * or 24 hour setting changes) or the time zone changed (`TIMEZONE_CHANGED`).
 * It tells the watch, which tells the host at once ([WatchChange.CLOCK],
 * [WatchChange.TIME_ZONE]); after the clock was set the watch also reads
 * Android's usage record again from the start, since Android moves its
 * stored events with the clock.
 *
 * Both are Android's own broadcasts, which no app can send, and the receiver
 * is not exported. Nothing here may fail out of it.
 */
class ClockReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    try {
      when (intent.action) {
        Intent.ACTION_TIME_CHANGED -> Watch.get(context).clockChanged(zone = false)
        Intent.ACTION_TIMEZONE_CHANGED -> Watch.get(context).clockChanged(zone = true)
      }
    } catch (e: Exception) {
      AndroidWatchLog.line("clock failed error=${e.javaClass.simpleName}")
    }
  }
}
