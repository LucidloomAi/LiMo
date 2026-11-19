package ai.lucidloom.limo.child.watch

import android.accessibilityservice.AccessibilityService
import android.view.accessibility.AccessibilityEvent

/**
 * The accessibility service. A stub: it receives window state events and
 * does nothing with them.
 *
 * Folder watch/ will hold the facts the engine is given: which app is in
 * front (from these events, by package name only), usage access, the screen,
 * the lock screen, calls, and clock changes. The covers of show/ are drawn as
 * this service's overlay windows.
 *
 * What it may ask Android for is fixed in res/xml/limo_watch_service.xml.
 */
class WatchService : AccessibilityService() {
  override fun onAccessibilityEvent(event: AccessibilityEvent?) = Unit

  override fun onInterrupt() = Unit
}
