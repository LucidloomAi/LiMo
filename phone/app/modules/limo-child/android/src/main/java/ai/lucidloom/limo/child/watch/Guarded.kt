package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.ffi.FrontKind
import ai.lucidloom.limo.child.ffi.GuardedPage

/**
 * **What could switch LiMo off or remove it, and how each is recognised:
 * the one place these names are kept.** The engine covers what is named
 * here with the guard (the guardian PIN only); the watch only says when one
 * is in front. Every entry has its reason, and says whether it was seen on
 * a phone or read in Android's source only.
 *
 * Three ways, from the surest to the least sure:
 *
 * - **By event** (needs no name): Android calls LiMo's device admin
 *   receiver (`onDisableRequested`) when someone asks to switch it off
 *   ([GuardWatch.adminOffAsked]).
 * - **By package** ([PACKAGES]): a settings app that exists only for
 *   something LiMo depends on, covered whole while it is in front.
 * - **By the page's name** ([PAGES]): inside Android's Settings and the
 *   package installer only, the name of the window's class in the
 *   accessibility event. That class name is read for no other package
 *   ([CLASS_NAME_PACKAGES]), only to be matched here, and it is never
 *   logged in a release build, never kept and never sent.
 */
internal object Guarded {
  /** Android's Settings app: this package on Android's own source and on the makers known. */
  const val SETTINGS = "com.android.settings"

  /**
   * The package installer: Android's own source names it
   * `com.android.packageinstaller`, and phones with Google's apps carry
   * Google's build of it, `com.google.android.packageinstaller`. No page of
   * it is guarded yet: Android itself refuses to uninstall an app while it is
   * an active device admin, and the installer then sends a person to the
   * device admin's page, which is guarded. It is here so that the debug
   * build's probe can show its pages' names.
   */
  val INSTALLERS: Set<String> = setOf("com.android.packageinstaller", "com.google.android.packageinstaller")

  /** The only packages whose window class name is ever read: Settings and the installer. */
  val CLASS_NAME_PACKAGES: Set<String> = setOf(SETTINGS) + INSTALLERS

  /**
   * **By package.** Each is guarded whole while it is in front.
   *
   * - `com.samsung.accessibility`: One UI's accessibility settings, where
   *   LiMo's accessibility service is switched off. Seen on a phone: the
   *   Galaxy S20 Ultra (Android 13, One UI 5.1) on 20 November 2026, where
   *   setup's Accessibility step opened it and time was counted under it.
   *   Android's own source keeps accessibility inside Settings instead, where
   *   no name tells LiMo's page apart (see the README).
   */
  val PACKAGES: Map<String, GuardedPage> = mapOf(
    "com.samsung.accessibility" to GuardedPage.ACCESSIBILITY,
  )

  /**
   * **By the page's name**, a package and the class name its window's
   * accessibility event carries (`Activity` sets it to its own class). From
   * Android's source, not seen on a phone yet:
   *
   * - `com.android.settings.DeviceAdminAdd`: the device admin's page on
   *   Android 8.0 to 9 (packages/apps/Settings, `src/com/android/settings/`).
   * - `com.android.settings.applications.specialaccess.deviceadmin.DeviceAdminAdd`:
   *   the same page on Android 10 to 15.
   *
   * That page switches LiMo's device admin off, and, opened from App info's
   * Uninstall, switches it off and removes LiMo in one tap with no word to
   * LiMo's receiver first; so it is covered as it comes up. A class name
   * cannot tell which app's admin the page is for: another app's admin page
   * is covered too, while LiMo's protection is on.
   */
  val PAGES: Map<Pair<String, String>, GuardedPage> = mapOf(
    (SETTINGS to "com.android.settings.DeviceAdminAdd") to GuardedPage.DEVICE_ADMIN,
    (SETTINGS to "com.android.settings.applications.specialaccess.deviceadmin.DeviceAdminAdd") to GuardedPage.DEVICE_ADMIN,
  )

  /** True only for Settings and the installer: the class name of [packageName]'s window may be read. */
  fun readsClassName(packageName: String?): Boolean = packageName != null && packageName in CLASS_NAME_PACKAGES

  /** The guarded page a window of [packageName] with [className] is, from [PAGES]; null for anything else. */
  fun page(packageName: String?, className: String?): GuardedPage? {
    if (!readsClassName(packageName) || className == null) return null
    return PAGES[packageName to className]
  }
}

/**
 * **Which guarded page is in front, the watch's part.** Pure: [WatchCore]
 * feeds it what Android said and asks it at every look.
 *
 * - **A page or the event raises the guard** for the package in front then
 *   (Settings, for both). It holds while that package's activity in front is
 *   the one that was in front when it was raised, as Android's usage record
 *   tells: a dialog over the page (Android's own "Deactivate?" among them)
 *   resumes nothing, so it stays guarded; another page of the same app
 *   resumes, and the guard ends with it. Another app in front ends it too.
 * - **LiMo's own screens and the diallers set it aside**, never end it: the
 *   guard's PIN is typed in LiMo's own screen, and Emergency call opens the
 *   emergency dialler, and coming back to the same page must find it guarded
 *   again, whether or not that page tells anything again.
 * - **For a moment after it is raised** ([WatchTimes.CONFIRM_GRACE_MS]) it
 *   holds while usage shows nothing newer than the raise, since Android may
 *   write the page's own resume after its window told the service; an app
 *   that usage shows resumed after the raise ends it at once.
 * - **With usage access off**, the front is the package that last sent a
 *   window event, and another package's window ends it (after that moment).
 * - **A package guarded whole** ([Guarded.PACKAGES]) is guarded while usage
 *   shows it in front, or, with usage access off, while it was the last to
 *   send a window event.
 */
internal class GuardWatch(private val log: WatchLog) {
  private class Raised(
    val page: GuardedPage,
    val packageName: String,
    /** How it was raised, for the log: `page` (a class name matched) or `event` (the admin's switch off asked). */
    val by: String,
    var sinceWallMs: Long,
    val sinceMs: Long,
    var aside: Boolean,
  )

  private var raised: Raised? = null
  private var said: Pair<GuardedPage, String>? = null

  /** A window event from [packageName]; [page] is what its class name matched, for Settings and the installer only. */
  fun windowEvent(packageName: String?, page: GuardedPage?, wallMs: Long, nowMs: Long) {
    if (page == null || packageName == null) return
    raised = Raised(page, packageName, "page", wallMs, nowMs, aside = false)
  }

  /**
   * Android asked LiMo's device admin receiver whether it may be switched
   * off (someone tapped Deactivate on its page): the guard is raised for
   * [inFront], the package in front now (Settings), at once.
   */
  fun adminOffAsked(inFront: String?, wallMs: Long, nowMs: Long) {
    raised = Raised(GuardedPage.DEVICE_ADMIN, inFront.orEmpty(), "event", wallMs, nowMs, aside = false)
  }

  /** True while a raised guard holds for [packageName]: a window event of it may have left the page. */
  fun holdsFor(packageName: String?): Boolean = packageName != null && raised?.packageName == packageName

  /**
   * The guarded page in front now, from what is in front ([pick]), when its
   * activity resumed ([frontSinceWallMs], null with usage access off), the
   * package that last sent a window event ([lastWindow]) and whether usage
   * can be read ([usageOn]). Logs one line when the answer changes.
   */
  fun now(pick: Pick, frontSinceWallMs: Long?, lastWindow: String?, usageOn: Boolean, nowMs: Long): GuardedPage? {
    val inFront = if (usageOn) pick.packageName ?: lastWindow else lastWindow
    raised?.let { r ->
      // Usage has said nothing newer than the raise yet: what it shows in
      // front may be from before the page came up.
      val since = frontSinceWallMs.takeIf { usageOn }
      val older = since == null || since <= r.sinceWallMs
      when {
        pick.kind == FrontKind.LIMO || pick.kind == FrontKind.DIALLER || pick.kind == FrontKind.EMERGENCY_DIALLER -> {
          r.aside = true
          return answer(null, null, null)
        }
        inFront == r.packageName || (r.packageName.isEmpty() && inFront == null) -> {
          if (older) return answer(r.page, r.packageName, r.by)
          if (r.aside) {
            // The same app came back after LiMo's own screen or a dialler:
            // the page that was guarded, guarded again.
            r.sinceWallMs = since
            r.aside = false
            return answer(r.page, r.packageName, r.by)
          }
          raised = null
        }
        // Another app is in front by what usage last read: for a moment
        // after the raise that may be from before the page came up, unless
        // usage shows it resumed after the raise.
        older && nowMs - r.sinceMs < WatchTimes.CONFIRM_GRACE_MS -> return answer(r.page, r.packageName, r.by)
        else -> raised = null
      }
    }
    val pkg = if (usageOn) pick.packageName else lastWindow
    return answer(pkg?.let { Guarded.PACKAGES[it] }, pkg, "package")
  }

  /** [page] for [packageName], with one log line when it differs from the last answer: the page and the package, never a class name. */
  private fun answer(page: GuardedPage?, packageName: String?, by: String?): GuardedPage? {
    val now = page?.let { it to packageName.orEmpty() }
    if (now != said) {
      said = now
      log.line(if (page == null) "guard now none" else "guard now ${page.name.lowercase()} by=$by package=${packageName.orEmpty()}")
    }
    return page
  }
}
