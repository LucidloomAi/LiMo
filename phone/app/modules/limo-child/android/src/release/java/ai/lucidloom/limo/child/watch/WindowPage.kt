package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.ffi.GuardedPage

/**
 * Release builds (src/release). The class name of a window of Android's
 * Settings or the installer, as [WatchService] read it, matched against the
 * fixed list ([Guarded.PAGES]) and dropped: never logged, kept or sent.
 *
 * Debug builds have their own copy of this object (src/debug/watch), which
 * also hands the name to the development probe when a person at the PC has
 * switched it on. Nothing of the probe is in this one.
 */
internal object WindowPage {
  fun of(packageName: String?, className: String?): GuardedPage? = Guarded.page(packageName, className)
}
