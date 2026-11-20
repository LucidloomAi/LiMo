package ai.lucidloom.limo.child.keep

import ai.lucidloom.limo.child.host.Host
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log

/**
 * Told when the phone has restarted (`BOOT_COMPLETED`) and when LiMo has
 * been updated (`MY_PACKAGE_REPLACED`). Starts the keeper: the engine's
 * host loads its state, so a lock that was on is on again at the first
 * tick, and the foreground service starts (Android lets an app start one
 * from these two broadcasts, also from the background, and on Android 15 a
 * `specialUse` service among them).
 *
 * The broadcast is held until the host's first tick is done, so Android
 * does not stop the process before it.
 */
class RestartReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    val why = when (intent.action) {
      Intent.ACTION_BOOT_COMPLETED -> "boot"
      Intent.ACTION_MY_PACKAGE_REPLACED -> "update"
      else -> return
    }
    val pending = goAsync()
    try {
      Keeper.start(context, why)
      Host.of(context).wake { pending.finish() }
    } catch (e: Exception) {
      Log.i("LiMo", "keep failed what=restart why=$why error=${e.javaClass.simpleName}")
      pending.finish()
    }
  }
}
