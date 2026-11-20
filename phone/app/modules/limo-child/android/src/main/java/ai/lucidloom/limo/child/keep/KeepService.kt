package ai.lucidloom.limo.child.keep

import ai.lucidloom.limo.child.host.Host
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log

/**
 * The foreground service that keeps LiMo's process alive while the phone
 * is paired, so that the engine's host (host/) goes on ticking, and that
 * shows the time left as its ongoing notification ([NotificationKeeper]).
 * It does no work of its own: the host runs on its own thread whether this
 * service runs or not.
 *
 * Its type is `specialUse` (Android 14 and later), with the subtype the
 * manifest gives in `PROPERTY_SPECIAL_USE_FGS_SUBTYPE`. `START_STICKY`:
 * Android starts it again when it stopped the process to free memory. With
 * no state kept (not paired) it stops at once.
 */
class KeepService : Service() {
  override fun onCreate() {
    super.onCreate()
    running = true
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    val keeper = Keeper.display(this)
    try {
      // Called at every start, as Android asks after startForegroundService.
      if (Build.VERSION.SDK_INT >= 34) {
        startForeground(NotificationKeeper.ID_TIME_LEFT, keeper.current(), ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
      } else {
        startForeground(NotificationKeeper.ID_TIME_LEFT, keeper.current())
      }
    } catch (e: Exception) {
      // Android refused the foreground (from Android 12, when started from
      // the background without an exemption). The host runs on regardless.
      Log.i(TAG, "keep foreground_refused error=${e.javaClass.simpleName}")
      stopSelf(startId)
      return START_NOT_STICKY
    }
    return try {
      val host = Host.of(this)
      host.start()
      if (host.paired) {
        START_STICKY
      } else {
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf(startId)
        START_NOT_STICKY
      }
    } catch (e: Exception) {
      Log.i(TAG, "keep failed what=service error=${e.javaClass.simpleName}")
      START_STICKY
    }
  }

  override fun onDestroy() {
    running = false
    super.onDestroy()
  }

  override fun onBind(intent: Intent?): IBinder? = null

  companion object {
    private const val TAG = "LiMo"

    /** The service is running in this process. */
    @Volatile
    @JvmStatic
    var running: Boolean = false
      private set
  }
}
