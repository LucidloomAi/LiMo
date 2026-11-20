package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.keep.Keeper
import android.accessibilityservice.AccessibilityService
import android.content.Intent
import android.view.accessibility.AccessibilityEvent

/**
 * The accessibility service. It is told when a window comes to the front,
 * and reads of each event its package name, and, **only for Android's
 * Settings and the package installer** ([Guarded.CLASS_NAME_PACKAGES]), the
 * name of the window's class, only to match it against a fixed list of pages
 * that could switch LiMo off or remove it ([Guarded.PAGES]). That class name
 * is never read for any other app, never logged in a release build, never
 * kept and never sent. Never the event's text, its content description, its
 * source or any node of the screen; and Android gives it none, since its
 * settings (res/xml/limo_watch_service.xml) ask for window state changes only
 * and no window content. That is the promise the setup screens make: LiMo is
 * told an app's name, never what is on the screen.
 *
 * A window event only proposes an app; Android's usage events confirm it
 * (see [FrontWatch]). The covers of show/ are drawn as this service's
 * overlay windows: [current] is the connected service for that.
 *
 * Nothing here may stop the service: every call into the watch is guarded.
 */
class WatchService : AccessibilityService() {
  override fun onServiceConnected() {
    super.onServiceConnected()
    current = this
    guard("service") { Watch.get(this).serviceConnected(true) }
    // Android starts LiMo's process to connect this service (after a
    // restart, or a guardian switching it on): the engine's host and the
    // keeper start with it, when the phone is paired.
    guard("keep") { Keeper.start(this, "accessibility") }
  }

  override fun onAccessibilityEvent(event: AccessibilityEvent?) {
    guard("window_event") {
      if (event == null || event.eventType != AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED) return@guard
      val packageName = event.packageName?.toString()
      // The class name: for Settings and the installer only, matched and dropped (WindowPage).
      val page = if (Guarded.readsClassName(packageName)) WindowPage.of(packageName, event.className?.toString()) else null
      Watch.get(this).windowEvent(packageName, page)
    }
  }

  override fun onInterrupt() = Unit

  override fun onUnbind(intent: Intent?): Boolean {
    stopped()
    return super.onUnbind(intent)
  }

  override fun onDestroy() {
    stopped()
    super.onDestroy()
  }

  private fun stopped() {
    if (current !== this) return
    current = null
    guard("service") { Watch.get(this).serviceConnected(false) }
  }

  private inline fun guard(what: String, block: () -> Unit) {
    try {
      block()
    } catch (e: Exception) {
      AndroidWatchLog.line("$what failed error=${e.javaClass.simpleName}")
    }
  }

  companion object {
    /**
     * The connected service, for show/ to add its covers as this service's
     * overlay windows; null while Android has not connected it. Read it on
     * the main thread.
     */
    @Volatile
    @JvmStatic
    var current: WatchService? = null
      private set
  }
}
