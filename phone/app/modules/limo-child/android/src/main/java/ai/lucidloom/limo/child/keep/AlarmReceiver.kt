package ai.lucidloom.limo.child.keep

import ai.lucidloom.limo.child.host.Host
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log

/**
 * LiMo's own alarm, from the one pending intent the host makes
 * (`host/AlarmWakeups`): it wakes the host while the screen is off, at the
 * engine's next check, and at most fifteen minutes apart in any case. If
 * LiMo's process had been stopped, the alarm starts it again, and with it
 * the keeper.
 *
 * The broadcast is held until the tick is done.
 */
class AlarmReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    if (intent.action != ACTION_WAKE) return
    val pending = goAsync()
    try {
      Keeper.start(context, "alarm")
      Host.of(context).wake { pending.finish() }
    } catch (e: Exception) {
      Log.i("LiMo", "keep failed what=alarm error=${e.javaClass.simpleName}")
      pending.finish()
    }
  }

  companion object {
    /** The action of LiMo's alarm. Not exported: only LiMo's own pending intent reaches this receiver. */
    const val ACTION_WAKE = "ai.lucidloom.limo.child.keep.WAKE"
  }
}
