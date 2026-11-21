package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.ffi.GuardedPage
import ai.lucidloom.limo.child.ffi.PhoneFacts
import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log
import java.util.concurrent.CopyOnWriteArrayList

/**
 * The watch: [PhoneWatch] for the engine's host, fed by the accessibility
 * service ([WatchService]), the clock receiver ([ClockReceiver]) and, while
 * anyone listens, Android's own signals ([WatchSignals]). One per process.
 *
 * Every decision is [WatchCore]'s, called under one lock. This class only
 * carries: it reads through [AndroidPhone], tells listeners on the main
 * thread, and keeps the settling of a window event going with a handler on
 * the main thread. No thread of its own.
 */
class Watch private constructor(context: Context) : PhoneWatch {
  private val app: Context = context.applicationContext ?: context
  private val phone = AndroidPhone(app)
  private val lock = Any()
  private val core = WatchCore(phone, AndroidWatchLog)
  private val main = Handler(Looper.getMainLooper())
  private val listeners = CopyOnWriteArrayList<WatchListener>()
  private val signals = WatchSignals(app, this)

  @Volatile
  private var phoneSeen: Boolean? = null

  private val recheck = Runnable {
    guard("recheck") {
      val changed = synchronized(lock) { core.recheck() }
      scheduleRecheck()
      if (changed) announce(WatchChange.FRONT)
    }
  }

  // PhoneWatch

  override fun factsNow(inReach: Boolean, prodded: Boolean): PhoneFacts = look(inReach, prodded).facts

  override fun look(inReach: Boolean, prodded: Boolean): WatchLook {
    val look = synchronized(lock) {
      try {
        core.look(inReach, prodded)
      } catch (e: Exception) {
        AndroidWatchLog.line("look failed error=${e.javaClass.simpleName}")
        core.lastLook(inReach, prodded)
      }
    }
    phoneRead(look.permissions.phone)
    return look
  }

  override fun listen(listener: WatchListener): AutoCloseable {
    listeners.add(listener)
    syncSignals()
    return AutoCloseable {
      if (listeners.remove(listener)) syncSignals()
    }
  }

  override fun launchableApps(): List<LaunchableApp> {
    // Loading every label is slow, so it is done outside the lock: window
    // events go on being settled meanwhile.
    val raw = try {
      phone.launchableApps()
    } catch (e: Exception) {
      AndroidWatchLog.line("fact launchable_apps unreadable error=${e.javaClass.simpleName}")
      return emptyList()
    }
    return synchronized(lock) { core.shapeApps(raw) }
  }

  override fun permissions(): LimoPermissions = synchronized(lock) { core.permissions() }.also { phoneRead(it.phone) }

  override fun limoScreenShown(shown: Boolean) {
    guard("limo_screen") {
      val changed = synchronized(lock) { core.limoScreen(shown) }
      if (changed) announce(WatchChange.FRONT)
    }
  }

  override fun emergencyDiallerOpening() {
    guard("emergency_opening") { synchronized(lock) { core.emergencyOpening() } }
  }

  override fun deviceAdminOffAsked() {
    guard("admin_off") {
      val changed = synchronized(lock) { core.adminOffAsked() }
      if (changed) announce(WatchChange.FRONT)
    }
  }

  // From the service, the receivers and the signals. Each is guarded, so no
  // failure here can stop the service or a receiver.

  /**
   * How far the guard reaches now, for the service: which packages' window
   * class names it may read, and what they are matched against. The package
   * manager is asked at the first need in this process and again when apps
   * changed. Never throws: a refused answer keeps the last good one, and with
   * none the fixed lists hold.
   */
  internal fun reach(): GuardReach = synchronized(lock) { core.reachNow() }

  /** The package manager's last answers for the guard's intents, for the debug build's probe (dev/PageProbe). */
  internal fun pagesAnswered(): Map<PageIntent, IntentAnswer> = synchronized(lock) { core.pagesAnswered() }

  internal fun windowEvent(packageName: String?, page: GuardedPage?) = guard("window_event") {
    val changed = synchronized(lock) { core.windowEvent(packageName, page) }
    scheduleRecheck()
    if (changed) announce(WatchChange.FRONT)
  }

  internal fun serviceConnected(running: Boolean) = guard("service") {
    synchronized(lock) { core.serviceConnected(running) }
    announce(WatchChange.PERMISSIONS)
  }

  internal fun clockChanged(zone: Boolean) = guard("clock") {
    val changed = synchronized(lock) { if (zone) false else core.clockSet() }
    announce(if (zone) WatchChange.TIME_ZONE else WatchChange.CLOCK)
    if (changed) announce(WatchChange.FRONT)
  }

  internal fun callChanged() = guard("call") {
    val changed = synchronized(lock) { core.callChanged() }
    announce(WatchChange.CALL)
    if (changed) announce(WatchChange.FRONT)
  }

  internal fun screenChanged() = guard("screen") {
    // The guard learns of the phone put down at once, whether or not the
    // host looks then; the host's tick at this signal sees the rest.
    synchronized(lock) { core.screenChanged() }
    announce(WatchChange.SCREEN)
  }

  internal fun appsChanged(tell: Boolean) = guard("apps") {
    synchronized(lock) { core.appsChanged() }
    if (tell) announce(WatchChange.APPS)
  }

  internal fun permissionsChanged() = guard("permissions") {
    synchronized(lock) { core.permissionsChanged() }
    announce(WatchChange.PERMISSIONS)
  }

  /**
   * The Phone permission as just read. When it reads otherwise than before
   * (setup grants it while LiMo runs, and Android tells nothing of it),
   * Android's own call listener follows it, on the main thread.
   */
  private fun phoneRead(granted: Boolean) {
    if (phoneSeen == granted) return
    phoneSeen = granted
    main.post { guard("signals") { signals.phoneGranted(granted) } }
  }

  private fun scheduleRecheck() {
    val delay = synchronized(lock) { core.nextLookInMs() }
    main.removeCallbacks(recheck)
    if (delay != null) main.postDelayed(recheck, delay.coerceAtLeast(0L))
  }

  /** Starts Android's signals while anyone listens, and stops them when nobody does. On the main thread. */
  private fun syncSignals() {
    main.post {
      guard("signals") {
        if (listeners.isEmpty()) signals.stop() else signals.start()
      }
    }
  }

  private fun announce(what: WatchChange) {
    if (Looper.myLooper() == Looper.getMainLooper()) deliver(what) else main.post { deliver(what) }
  }

  private fun deliver(what: WatchChange) {
    for (listener in listeners) {
      try {
        listener.changed(what)
      } catch (e: Exception) {
        AndroidWatchLog.line("listener failed change=${what.name.lowercase()} error=${e.javaClass.simpleName}")
      }
    }
  }

  private inline fun guard(what: String, block: () -> Unit) {
    try {
      block()
    } catch (e: Exception) {
      AndroidWatchLog.line("$what failed error=${e.javaClass.simpleName}")
    } catch (e: LinkageError) {
      AndroidWatchLog.line("$what failed error=${e.javaClass.simpleName}")
    }
  }

  companion object {
    @Volatile
    private var instance: Watch? = null

    /** The watch of this process, made on first use. Any thread. */
    @JvmStatic
    fun of(context: Context): PhoneWatch = get(context)

    internal fun get(context: Context): Watch =
      instance ?: synchronized(this) { instance ?: Watch(context).also { instance = it } }
  }
}

/** The watch's log: Android's log, tag `LiMoWatch`. */
internal object AndroidWatchLog : WatchLog {
  private const val TAG = "LiMoWatch"

  override fun line(text: String) {
    Log.i(TAG, text)
  }
}
