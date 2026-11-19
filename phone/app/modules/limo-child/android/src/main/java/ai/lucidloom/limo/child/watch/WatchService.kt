package ai.lucidloom.limo.child.watch

import android.accessibilityservice.AccessibilityService
import android.content.Intent
import android.view.accessibility.AccessibilityEvent

/**
 * The accessibility service. It is told when a window comes to the front,
 * and reads one thing of each event: the package name. Never the event's
 * text, its content description, its class name, its source or any node of
 * the screen; and Android gives it none, since its settings
 * (res/xml/limo_watch_service.xml) ask for window state changes only and no
 * window content. That is the promise the setup screens make: LiMo is told
 * an app's name, never what is on the screen.
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
  }

  override fun onAccessibilityEvent(event: AccessibilityEvent?) {
    guard("window_event") {
      if (event == null || event.eventType != AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED) return@guard
      // The only field read.
      val packageName = event.packageName?.toString()
      Watch.get(this).windowEvent(packageName)
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
