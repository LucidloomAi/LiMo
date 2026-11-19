package ai.lucidloom.limo.child.keep

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * LiMo's own alarms arrive here, from pending intents only LiMo makes. A
 * stub: it does nothing. Later an alarm wakes the engine at the time it
 * asked to be asked again.
 */
class AlarmReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) = Unit
}
