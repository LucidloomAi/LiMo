package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.ffi.GuardedPage

/**
 * Release builds (src/release). A class name of a package the guard reads
 * ([GuardReach]: Settings, the installer, and the package Android named for
 * the device admin's page), matched against the guarded pages
 * ([GuardReach.page]) and dropped: never logged, kept or sent. Two callers,
 * each after asking the reach first: [WatchService] for a window's class,
 * and [AndroidPhone] for the class of a resumed activity in Android's usage
 * record.
 *
 * Debug builds have their own copy of this object (src/debug/watch), which
 * also hands the name to the development probe when a person at the PC has
 * switched it on. Nothing of the probe is in this one.
 */
internal object WindowPage {
  /** A window's class name, from the accessibility event. */
  fun of(reach: GuardReach, packageName: String?, className: String?): GuardedPage? = reach.page(packageName, className)

  /** A resumed activity's class name, from Android's usage record. */
  fun resumed(reach: GuardReach, packageName: String, className: String?): GuardedPage? = reach.page(packageName, className)
}
