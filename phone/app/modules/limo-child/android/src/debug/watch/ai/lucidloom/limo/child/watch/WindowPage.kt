package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.dev.PageProbe
import ai.lucidloom.limo.child.ffi.GuardedPage

/**
 * Debug builds only (src/debug/watch, added to the debug source set in the
 * module's build.gradle; a release build has src/release's copy, with no
 * probe). The class name of a window of Android's Settings or the installer,
 * as [WatchService] read it: handed to the development probe
 * ([PageProbe.window]), which logs it only while a person at the PC has
 * switched it on, then matched against the fixed list as a release build
 * does. The probe sees exactly what a release build reads, nothing more.
 */
internal object WindowPage {
  fun of(packageName: String?, className: String?): GuardedPage? {
    PageProbe.window(packageName, className)
    return Guarded.page(packageName, className)
  }
}
