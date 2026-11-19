package ai.lucidloom.limo.child.watch

import android.app.AppOpsManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.database.ContentObserver
import android.media.AudioManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.telecom.TelecomManager
import android.telephony.TelephonyManager

/**
 * Android's signals that something changed, registered while anyone listens
 * to the watch ([Watch.listen]) and gone when nobody does. Each is
 * registered and unregistered on its own, so one Android refuses leaves the
 * others working; then the host's once a second call still notices within a
 * second. Start and stop on the main thread; the callbacks come there too,
 * except the app ops one, which [Watch] carries to the main thread itself.
 *
 * | Signal | Android's call | Tells |
 * | --- | --- | --- |
 * | screen on, off, unlocked | receiver: `SCREEN_ON`, `SCREEN_OFF`, `USER_PRESENT` | [WatchChange.SCREEN] |
 * | a call (Android 12 and later) | `AudioManager.addOnModeChangedListener` | [WatchChange.CALL] |
 * | a call (Android 8 to 11) | `TelephonyManager.listen(LISTEN_CALL_STATE)`, no permission needed there | [WatchChange.CALL] |
 * | apps installed, removed, changed; the default dialler | receiver: `PACKAGE_*` with the `package` scheme, `ACTION_DEFAULT_DIALER_CHANGED` | [WatchChange.APPS] |
 * | keyboards | receiver: `INPUT_METHOD_CHANGED` | nothing to tell; the roles are asked again |
 * | usage access | `AppOpsManager.startWatchingMode` | [WatchChange.PERMISSIONS] |
 * | the accessibility setting | a content observer on `ENABLED_ACCESSIBILITY_SERVICES` | [WatchChange.PERMISSIONS] |
 *
 * The clock and the zone come by the manifest's [ClockReceiver], whether
 * anyone listens or not.
 */
internal class WatchSignals(private val context: Context, private val watch: Watch) {
  private var running = false
  private val registered = ArrayList<() -> Unit>()

  private val screen = object : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = watch.screenChanged()
  }

  private val packages = object : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = watch.appsChanged(tell = true)
  }

  private val dialler = object : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = watch.appsChanged(tell = true)
  }

  private val keyboards = object : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = watch.appsChanged(tell = false)
  }

  private val accessibilitySetting = object : ContentObserver(Handler(Looper.getMainLooper())) {
    override fun onChange(selfChange: Boolean) = watch.permissionsChanged()
  }

  fun start() {
    if (running) return
    running = true
    receiver("screen", screen, IntentFilter().apply {
      addAction(Intent.ACTION_SCREEN_ON)
      addAction(Intent.ACTION_SCREEN_OFF)
      addAction(Intent.ACTION_USER_PRESENT)
    })
    receiver("packages", packages, IntentFilter().apply {
      addAction(Intent.ACTION_PACKAGE_ADDED)
      addAction(Intent.ACTION_PACKAGE_REMOVED)
      addAction(Intent.ACTION_PACKAGE_CHANGED)
      addAction(Intent.ACTION_PACKAGE_REPLACED)
      addDataScheme("package")
    })
    receiver("default_dialler", dialler, IntentFilter(TelecomManager.ACTION_DEFAULT_DIALER_CHANGED))
    receiver("keyboards", keyboards, IntentFilter(Intent.ACTION_INPUT_METHOD_CHANGED))
    attempt("calls") { calls() }
    attempt("usage_access") { usageAccess() }
    attempt("accessibility_setting") { accessibilitySetting() }
  }

  fun stop() {
    if (!running) return
    running = false
    for (undo in registered.asReversed()) {
      try {
        undo()
      } catch (_: Exception) {
        // Already gone.
      }
    }
    registered.clear()
  }

  private fun receiver(what: String, receiver: BroadcastReceiver, filter: IntentFilter) = attempt(what) {
    // Only Android's own broadcasts are asked for, which no app can send.
    if (Build.VERSION.SDK_INT >= 33) {
      context.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
    } else {
      context.registerReceiver(receiver, filter)
    }
    registered.add { context.unregisterReceiver(receiver) }
  }

  // PhoneStateListener is deprecated from Android 12, where it is not used:
  // there it would need the phone permission. Before, it needs none.
  @Suppress("DEPRECATION")
  private fun calls() {
    if (Build.VERSION.SDK_INT >= 31) {
      val audio = requireNotNull(context.getSystemService(AudioManager::class.java))
      val listener = AudioManager.OnModeChangedListener { watch.callChanged() }
      audio.addOnModeChangedListener(context.mainExecutor, listener)
      registered.add { audio.removeOnModeChangedListener(listener) }
    } else {
      val telephony = requireNotNull(context.getSystemService(TelephonyManager::class.java))
      val listener = object : android.telephony.PhoneStateListener() {
        // Neither the state nor the number is read here: the watch reads
        // the state again itself, and a number is never read at all.
        @Deprecated("Deprecated in Android")
        override fun onCallStateChanged(state: Int, phoneNumber: String?) = watch.callChanged()
      }
      telephony.listen(listener, android.telephony.PhoneStateListener.LISTEN_CALL_STATE)
      registered.add { telephony.listen(listener, android.telephony.PhoneStateListener.LISTEN_NONE) }
    }
  }

  private fun usageAccess() {
    val ops = requireNotNull(context.getSystemService(AppOpsManager::class.java))
    val own = context.packageName
    val listener = AppOpsManager.OnOpChangedListener { _, pkg -> if (pkg == own) watch.permissionsChanged() }
    ops.startWatchingMode(AppOpsManager.OPSTR_GET_USAGE_STATS, own, listener)
    registered.add { ops.stopWatchingMode(listener) }
  }

  private fun accessibilitySetting() {
    val uri = Settings.Secure.getUriFor(Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES)
    context.contentResolver.registerContentObserver(uri, false, accessibilitySetting)
    registered.add { context.contentResolver.unregisterContentObserver(accessibilitySetting) }
  }

  private inline fun attempt(what: String, block: () -> Unit) {
    try {
      block()
    } catch (e: Exception) {
      AndroidWatchLog.line("signal $what unavailable error=${e.javaClass.simpleName}")
    } catch (e: LinkageError) {
      AndroidWatchLog.line("signal $what unavailable error=${e.javaClass.simpleName}")
    }
  }
}
