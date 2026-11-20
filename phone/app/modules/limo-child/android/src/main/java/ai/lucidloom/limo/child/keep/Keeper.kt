package ai.lucidloom.limo.child.keep

import ai.lucidloom.limo.child.ffi.TimeLeft
import ai.lucidloom.limo.child.ffi.WarningShown
import ai.lucidloom.limo.child.host.Host
import ai.lucidloom.limo.child.host.KeeperDisplay
import ai.lucidloom.limo.child.module.R
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.SystemClock
import android.util.Log

/**
 * What keeps LiMo running on the phone: the engine's host, and the
 * foreground service ([KeepService]) that keeps the process alive and shows
 * the time left.
 *
 * [start] is called whenever LiMo's process may have just started: when the
 * app opens (the child module), when the accessibility service connects,
 * after the phone restarts or LiMo is updated ([RestartReceiver]), at each
 * of LiMo's alarms ([AlarmReceiver]), and after a pairing. It starts the
 * host, and, when the phone is paired, the service.
 */
object Keeper {
  private const val TAG = "LiMo"

  @Volatile
  private var display: NotificationKeeper? = null

  @Volatile
  private var lastTryMs: Long? = null

  /**
   * Starts the engine's host and, when the phone is paired, the foreground
   * service. Safe to call at any time, from any thread, as often as wanted;
   * it never throws. [why] is for the log only.
   */
  @JvmStatic
  fun start(context: Context, why: String) {
    try {
      val app = context.applicationContext ?: context
      val host = Host.of(app)
      host.start()
      if (host.paired && !KeepService.running) startService(app, why)
    } catch (e: Exception) {
      Log.i(TAG, "keep failed what=start why=$why error=${e.javaClass.simpleName}")
    }
  }

  /** Stops the foreground service and takes LiMo's notifications away: the phone is not paired any more. */
  @JvmStatic
  fun stop(context: Context) {
    try {
      val app = context.applicationContext ?: context
      app.stopService(Intent(app, KeepService::class.java))
      display(app).cancelAll()
    } catch (e: Exception) {
      Log.i(TAG, "keep failed what=stop error=${e.javaClass.simpleName}")
    }
  }

  /** LiMo's notifications, one per process: the host's [KeeperDisplay]. */
  internal fun display(context: Context): NotificationKeeper =
    display ?: synchronized(this) {
      display ?: NotificationKeeper(context.applicationContext ?: context).also { display = it }
    }

  /**
   * Asks Android to start [KeepService] in the foreground. From Android 12
   * on, Android refuses that while LiMo is in the background unless one of
   * its exemptions holds (see keep/README.md): then it is logged, and tried
   * again from the next tick, at most once a minute, until it takes.
   */
  internal fun startService(app: Context, why: String) {
    lastTryMs = SystemClock.elapsedRealtime()
    try {
      app.startForegroundService(Intent(app, KeepService::class.java))
    } catch (e: IllegalStateException) {
      // ForegroundServiceStartNotAllowedException, from Android 12.
      Log.i(TAG, "keep start_refused why=$why error=${e.javaClass.simpleName}")
    } catch (e: SecurityException) {
      Log.i(TAG, "keep start_refused why=$why error=${e.javaClass.simpleName}")
    }
  }

  /** From the host's ticks: the service is not running (Android refused it, or stopped it), so try again, at most once a minute. */
  internal fun retryService(app: Context) {
    if (KeepService.running) return
    val last = lastTryMs
    if (last != null && SystemClock.elapsedRealtime() - last < RETRY_MS) return
    if (!Host.of(app).paired) return
    startService(app, "retry")
  }

  private const val RETRY_MS = 60_000L
}

/**
 * LiMo's two notifications, as the host hands it the engine's orders.
 *
 * - **The time left**, on a quiet channel: one ongoing notification, the
 *   same one the foreground service shows, its title the engine's words,
 *   with Android's chronometer counting down while time is counted (see
 *   [KeeperLook]). It is posted again only when what it shows changes.
 * - **A warning**, on a channel of its own that Android shows as a heads up
 *   notification: the engine's message as the title, its hint as the text,
 *   and when the phone locks as the small line at the top. It goes away by
 *   itself a minute after that lock.
 *
 * Tapping either opens LiMo. Without the notifications permission (Android
 * 13 and later) Android shows neither; the service runs all the same.
 */
internal class NotificationKeeper(private val app: Context) : KeeperDisplay {
  @Volatile
  private var last: TimeLeftLook? = null

  @Volatile
  private var channelsMade = false

  override fun timeLeft(timeLeft: TimeLeft) {
    val look = KeeperLook.timeLeft(timeLeft, System.currentTimeMillis())
    if (KeeperLook.differs(last, look)) {
      last = look
      post(ID_TIME_LEFT, timeLeftNotification(look))
    }
    Keeper.retryService(app)
  }

  override fun warning(warning: WarningShown) {
    val look = KeeperLook.warning(warning, System.currentTimeMillis().floorDiv(1_000L))
    post(ID_WARNING, warningNotification(look))
  }

  override fun cleared() {
    Keeper.stop(app)
  }

  /** The service's notification now, for [KeepService.startForeground]. */
  fun current(): Notification = timeLeftNotification(last)

  fun cancelAll() {
    last = null
    try {
      val manager = app.getSystemService(NotificationManager::class.java) ?: return
      manager.cancel(ID_TIME_LEFT)
      manager.cancel(ID_WARNING)
    } catch (e: Exception) {
      Log.i("LiMo", "keep failed what=cancel error=${e.javaClass.simpleName}")
    }
  }

  private fun timeLeftNotification(look: TimeLeftLook?): Notification {
    makeChannels()
    val b = Notification.Builder(app, CHANNEL_TIME_LEFT)
      .setSmallIcon(R.drawable.limo_notify)
      .setOngoing(true)
      .setOnlyAlertOnce(true)
      .setLocalOnly(true)
      .setCategory(Notification.CATEGORY_STATUS)
      .setVisibility(Notification.VISIBILITY_PUBLIC)
    // Before the first tick there are no words yet: Android shows LiMo's name alone.
    look?.title?.let(b::setContentTitle)
    val end = look?.countdownToMs
    if (end != null) {
      b.setWhen(end).setShowWhen(true).setUsesChronometer(true).setChronometerCountDown(true)
    } else {
      b.setShowWhen(false).setUsesChronometer(false)
    }
    if (Build.VERSION.SDK_INT >= 31) b.setForegroundServiceBehavior(Notification.FOREGROUND_SERVICE_IMMEDIATE)
    openApp()?.let(b::setContentIntent)
    return b.build()
  }

  private fun warningNotification(look: WarningLook): Notification {
    makeChannels()
    val b = Notification.Builder(app, CHANNEL_WARNINGS)
      .setSmallIcon(R.drawable.limo_notify)
      .setContentTitle(look.title)
      .setContentText(look.text)
      .setSubText(look.whenWords)
      .setStyle(Notification.BigTextStyle().bigText(look.text))
      .setCategory(Notification.CATEGORY_REMINDER)
      .setVisibility(Notification.VISIBILITY_PUBLIC)
      .setLocalOnly(true)
      .setAutoCancel(true)
      .setShowWhen(false)
      .setTimeoutAfter(look.timeoutMs)
    openApp()?.let(b::setContentIntent)
    return b.build()
  }

  private fun post(id: Int, notification: Notification) {
    try {
      app.getSystemService(NotificationManager::class.java)?.notify(id, notification)
    } catch (e: Exception) {
      Log.i("LiMo", "keep failed what=notify error=${e.javaClass.simpleName}")
    }
  }

  private fun openApp(): PendingIntent? {
    val launch = app.packageManager.getLaunchIntentForPackage(app.packageName) ?: return null
    return PendingIntent.getActivity(app, 0, launch, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
  }

  private fun makeChannels() {
    if (channelsMade) return
    val manager = app.getSystemService(NotificationManager::class.java) ?: return
    val quiet = NotificationChannel(
      CHANNEL_TIME_LEFT,
      app.getString(R.string.limo_channel_time_left),
      NotificationManager.IMPORTANCE_LOW,
    ).apply {
      description = app.getString(R.string.limo_channel_time_left_about)
      setShowBadge(false)
      lockscreenVisibility = Notification.VISIBILITY_PUBLIC
    }
    val loud = NotificationChannel(
      CHANNEL_WARNINGS,
      app.getString(R.string.limo_channel_warnings),
      NotificationManager.IMPORTANCE_HIGH,
    ).apply {
      description = app.getString(R.string.limo_channel_warnings_about)
      setShowBadge(false)
      lockscreenVisibility = Notification.VISIBILITY_PUBLIC
    }
    manager.createNotificationChannels(listOf(quiet, loud))
    channelsMade = true
  }

  companion object {
    /** The time left, and the foreground service's notification: one and the same. */
    const val ID_TIME_LEFT = 1
    const val ID_WARNING = 2
    const val CHANNEL_TIME_LEFT = "limo_time_left"
    const val CHANNEL_WARNINGS = "limo_warnings"
  }
}
