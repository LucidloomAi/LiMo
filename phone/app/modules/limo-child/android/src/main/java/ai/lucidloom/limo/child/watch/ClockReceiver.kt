package ai.lucidloom.limo.child.watch

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Told when the clock or the time zone is changed. A stub: it does nothing.
 * Later it hands the change to the engine as a fact.
 */
class ClockReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) = Unit
}
