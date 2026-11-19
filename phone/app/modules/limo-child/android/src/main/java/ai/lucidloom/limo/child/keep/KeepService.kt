package ai.lucidloom.limo.child.keep

import android.app.Service
import android.content.Intent
import android.os.IBinder

/**
 * The foreground service that will host the engine and show the time left
 * notification. A stub: nothing starts it, and if something did, it would
 * stop at once.
 *
 * Folder keep/ will hold what keeps LiMo running and in place: this
 * service, its alarms, starting again after a restart or an update, and the
 * device admin of the base protection.
 */
class KeepService : Service() {
  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    stopSelf(startId)
    return START_NOT_STICKY
  }

  override fun onBind(intent: Intent?): IBinder? = null
}
