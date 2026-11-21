package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.dev.PageProbe
import ai.lucidloom.limo.child.ffi.GuardedPage

/**
 * Debug builds only (src/debug/watch, added to the debug source set in the
 * module's build.gradle; a release build has src/release's copy, with no
 * probe). A class name of a package the guard reads ([GuardReach]), as
 * [WatchService] read it from a window or [AndroidPhone] from a resumed
 * activity in Android's usage record: handed to the development probe
 * ([PageProbe.window], [PageProbe.resumed]), which logs it only while a
 * person at the PC has switched it on, then matched as a release build
 * does. The probe sees exactly what a release build reads, nothing more.
 */
internal object WindowPage {
  /** A window's class name, from the accessibility event. */
  fun of(reach: GuardReach, packageName: String?, className: String?): GuardedPage? {
    PageProbe.window(reach, packageName, className)
    return reach.page(packageName, className)
  }

  /** A resumed activity's class name, from Android's usage record. */
  fun resumed(reach: GuardReach, packageName: String, className: String?): GuardedPage? {
    PageProbe.resumed(reach, packageName, className)
    return reach.page(packageName, className)
  }
}
