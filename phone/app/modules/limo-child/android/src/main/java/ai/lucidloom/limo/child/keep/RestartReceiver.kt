package ai.lucidloom.limo.child.keep

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Told when the phone has restarted and when LiMo has been updated. A stub:
 * it does nothing. Later it starts the engine again, which loads its state,
 * so a lock that was on is on again at once.
 */
class RestartReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) = Unit
}
