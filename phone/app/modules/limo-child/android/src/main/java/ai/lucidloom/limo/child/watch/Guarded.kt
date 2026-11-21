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
 * Four ways, from the surest to the least sure:
 *
 * - **By event** (needs no name): Android calls LiMo's device admin
 *   receiver (`onDisableRequested`) when someone asks to switch it off
 *   ([GuardWatch.adminOffAsked]).
 * - **By Android's answer** ([GuardReach]): the package manager, asked on
 *   this phone which activity opens the device admin's page and which the
 *   accessibility settings ([PageIntent]). The device admin's page is then
 *   known by that activity's package and class; accessibility settings in a
 *   package of their own are guarded whole.
 * - **By package** ([PACKAGES]): a settings app that exists only for
 *   something LiMo depends on, covered whole while it is in front.
 * - **By the page's name** ([PAGES]): a fixed list read in Android's source,
 *   beside Android's answer, for a phone that answers nothing.
 *
 * A page known by its class is told two ways: by the class of its window
 * (the accessibility event) and by the class of the activity that resumed
 * (Android's usage record). Either class name is read only for the packages
 * [GuardReach] names, only to be matched, and is never logged in a release
 * build, never kept and never sent.
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

  /** The fixed packages whose window class name is read: Settings and the installer. */
  val CLASS_NAME_PACKAGES: Set<String> = setOf(SETTINGS) + INSTALLERS

  /**
   * **By package.** Each is guarded whole while it is in front.
   *
   * - `com.samsung.accessibility`: One UI's accessibility settings, where
   *   LiMo's accessibility service is switched off. Seen on a phone: the
   *   Galaxy S20 Ultra (Android 13, One UI 5.1) on 20 November 2026, where
   *   setup's Accessibility step opened it and time was counted under it.
   *   Kept beside Android's answer ([GuardReach]), which names such a
   *   package on any maker, for a phone where the package manager cannot be
   *   asked. Android's own source keeps accessibility inside Settings
   *   instead, where no name tells LiMo's page apart (see the README).
   */
  val PACKAGES: Map<String, GuardedPage> = mapOf(
    "com.samsung.accessibility" to GuardedPage.ACCESSIBILITY,
  )

  /**
   * **By the page's name**, a package and the class name its window's
   * accessibility event and its resumed activity in Android's usage record
   * carry (`Activity` sets the event's class to its own). From Android's
   * source, not seen on a phone yet:
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

  /** The packages of [PAGES]: Settings. */
  val PAGE_PACKAGES: Set<String> = PAGES.keys.mapTo(LinkedHashSet()) { it.first }

  /** True only for Settings and the installer: by the fixed list alone, the class name of [packageName]'s window may be read. */
  fun readsClassName(packageName: String?): Boolean = packageName != null && packageName in CLASS_NAME_PACKAGES

  /** The guarded page a window of [packageName] with [className] is, from [PAGES] alone; null for anything else. */
  fun page(packageName: String?, className: String?): GuardedPage? {
    if (!readsClassName(packageName) || className == null) return null
    return PAGES[packageName to className]
  }
}

/** The intents the guard asks the package manager about, each for a page or for where one is. */
internal enum class PageIntent {
  /** `DevicePolicyManager.ACTION_ADD_DEVICE_ADMIN`: the device admin's page. */
  DEVICE_ADMIN,

  /** `Settings.ACTION_ACCESSIBILITY_SETTINGS`: the accessibility settings' own entry page. */
  ACCESSIBILITY,

  /** `Settings.ACTION_SETTINGS`: Android's Settings app itself, which is never guarded whole. */
  SETTINGS,
}

/**
 * An activity the package manager named for an intent, as it named it:
 * never anything read from a screen.
 */
internal data class NamedActivity(
  val packageName: String,
  /** The activity's class as the package manager has it (an alias's own name, for an alias). */
  val className: String,
  /** For an alias, the activity it opens: the class that activity's window and usage events carry. */
  val aliasTarget: String? = null,
  /** Preinstalled (a system app). Only these are believed: anyone may install an app that claims an intent. */
  val preinstalled: Boolean,
) {
  /** Its class and, for an alias, its target's, each with the package. */
  val classes: List<Pair<String, String>> get() = listOfNotNull(packageName to className, aliasTarget?.let { packageName to it })

  /** Preinstalled and not the framework's own (Android's chooser among them). */
  val believable: Boolean get() = preinstalled && !Overlays.isOverlay(packageName)
}

/** What the package manager answered for one intent. */
internal data class IntentAnswer(
  /** The activity it opens (`resolveActivity`): null for none, Android's chooser where several could and none is chosen. */
  val opens: NamedActivity?,
  /** Every activity that handles it (`queryIntentActivities`). */
  val handlers: List<NamedActivity>,
) {
  /** Every preinstalled activity Android named, the one it opens first. */
  fun everyPreinstalled(): List<NamedActivity> = (listOfNotNull(opens) + handlers).filter { it.believable }.distinct()

  /**
   * The preinstalled activity it opens; where it opens Android's chooser or
   * an app a person installed and chose, every preinstalled one that could.
   */
  fun opensPreinstalled(): List<NamedActivity> {
    val first = opens?.takeIf { it.believable }
    return if (first != null) listOf(first) else handlers.filter { it.believable }.distinct()
  }

  companion object {
    /** No activity at all. */
    val NONE = IntentAnswer(null, emptyList())
  }
}

/**
 * **How far the guard reaches on this phone**: the fixed lists of
 * [Guarded], and what the package manager answered for [PageIntent]. Held
 * in memory only, for the life of LiMo's process; asked again when apps
 * change. Pure.
 *
 * - [adminPages]: the device admin's page as Android named it, each a
 *   package and a class: every preinstalled activity that handles
 *   `ACTION_ADD_DEVICE_ADMIN`, and an alias's target too. One that also
 *   opens Settings' own first page or its accessibility settings is not
 *   taken: it holds many pages, and the guard must not cover all of them.
 * - [accessibilityPackages]: the packages Android named for the
 *   accessibility settings that are not Android's Settings (as One UI's),
 *   guarded whole. "Android's Settings" is `com.android.settings`, every
 *   preinstalled app that opens `ACTION_SETTINGS` and the package of the
 *   device admin's page; with no answer for `ACTION_SETTINGS` no package is
 *   taken, since a maker's whole Settings app must never be guarded.
 * - [accessibility]: where the accessibility settings open, as it is.
 */
internal data class GuardReach(
  val adminPages: Set<Pair<String, String>> = emptySet(),
  val accessibilityPackages: Set<String> = emptySet(),
  val accessibility: AccessibilityPlace = AccessibilityPlace.NOT_NAMED,
  /** The packages the accessibility settings open in, for the log: only package names. */
  val accessibilityIn: Set<String> = emptySet(),
) {
  /** The packages of the device admin's page as Android named it. */
  val adminPackages: Set<String> = adminPages.mapTo(LinkedHashSet()) { it.first }

  /**
   * True when the class name of a window of [packageName] may be read:
   * Settings, the installer, and the package of the device admin's page
   * Android named. Never for any other app.
   */
  fun readsClassName(packageName: String?): Boolean =
    packageName != null && (packageName in Guarded.CLASS_NAME_PACKAGES || packageName in adminPackages)

  /**
   * True when the class of a resumed activity of [packageName] may be read
   * from Android's usage record: Settings and the package of the device
   * admin's page Android named. Not the installer, which has no page here;
   * never any other app.
   */
  fun readsResumedClassName(packageName: String?): Boolean =
    packageName != null && (packageName in Guarded.PAGE_PACKAGES || packageName in adminPackages)

  /** The guarded page a window or a resumed activity of [packageName] with [className] is; null for anything else. */
  fun page(packageName: String?, className: String?): GuardedPage? {
    if (packageName == null || className == null || !readsClassName(packageName)) return null
    Guarded.page(packageName, className)?.let { return it }
    return if ((packageName to className) in adminPages) GuardedPage.DEVICE_ADMIN else null
  }

  /** The page a package guarded whole is: the fixed list's, then the accessibility settings Android named. */
  fun wholePackage(packageName: String?): GuardedPage? {
    if (packageName == null) return null
    return Guarded.PACKAGES[packageName] ?: if (packageName in accessibilityPackages) GuardedPage.ACCESSIBILITY else null
  }

  /** What the host may be told of it: never a class. */
  fun scope(): GuardScope = GuardScope(deviceAdminNamed = adminPages.isNotEmpty(), accessibility = accessibility)

  companion object {
    /** The fixed lists alone: before the package manager answered, or where it never could. */
    val FIXED = GuardReach()

    /** From the package manager's answers for each [PageIntent]; null for one never answered. */
    fun from(admin: IntentAnswer?, accessibility: IntentAnswer?, settings: IntentAnswer?): GuardReach {
      val adminActivities = admin?.everyPreinstalled().orEmpty()
      val entries = accessibility?.opensPreinstalled().orEmpty()
      val settingsActivities = settings?.everyPreinstalled().orEmpty()
      // A class that also opens Settings' first page or its accessibility
      // settings holds many pages: never taken for the device admin's page.
      val containers = (settingsActivities + accessibility?.everyPreinstalled().orEmpty()).flatMapTo(HashSet()) { it.classes }
      val adminPages = adminActivities.flatMap { it.classes }.filterTo(LinkedHashSet()) { it !in containers }
      val entryPackages = entries.mapTo(LinkedHashSet()) { it.packageName }
      val own = if (settings == null) {
        emptySet()
      } else {
        val notOwn = setOf(Guarded.SETTINGS) + settingsActivities.map { it.packageName } + adminActivities.map { it.packageName }
        entryPackages.filterTo(LinkedHashSet()) { it !in notOwn }
      }
      val place = when {
        own.isNotEmpty() -> AccessibilityPlace.OWN_PACKAGE
        entryPackages.isNotEmpty() && settings != null -> AccessibilityPlace.IN_SETTINGS
        else -> AccessibilityPlace.NOT_NAMED
      }
      return GuardReach(adminPages, own, place, entryPackages.takeIf { settings != null }.orEmpty())
    }
  }
}

/**
 * **Which guarded page is in front, the watch's part.** Pure: [WatchCore]
 * feeds it what Android said and asks it at every look.
 *
 * - **By Android's record**: when usage shows that the activity in front is
 *   a guarded page by its class ([GuardReach.page], for the packages it
 *   reads only), that page is guarded, whether or not any window or event
 *   told the service. So the page is guarded at the first look of a new
 *   process of LiMo's, and again whenever it comes back to the front.
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
 * - **So does the phone put down**: its screen off, or Android's lock screen
 *   up. The page waits under it, paused, and resumes when the phone is
 *   unlocked; that first resume is the same page coming back, guarded again
 *   whether or not its window tells anything. The engine closes a PIN's
 *   opening at that same moment, so a phone handed back with its screen off
 *   comes back guarded. Only the watch can keep this: Android's record tells
 *   it the same page coming back (its first resume after the phone was put
 *   down) from another page of the same app (a resume while in use), which
 *   the engine, seeing only the kind of page, cannot tell apart.
 * - **For a moment after it is raised** ([WatchTimes.CONFIRM_GRACE_MS]) it
 *   holds while usage shows nothing newer than the raise, since Android may
 *   write the page's own resume after its window told the service; an app
 *   that usage shows resumed after the raise ends it at once.
 * - **With usage access off**, the front is the package that last sent a
 *   window event, and another package's window ends it (after that moment).
 * - **A package guarded whole** ([GuardReach.wholePackage]) is guarded while
 *   usage shows it in front, or, with usage access off, while it was the
 *   last to send a window event.
 */
internal class GuardWatch(private val log: WatchLog) {
  private class Raised(
    val page: GuardedPage,
    val packageName: String,
    /** How it was raised, for the log: `page` (a window's class matched) or `event` (the admin's switch off asked). */
    val by: String,
    var sinceWallMs: Long,
    val sinceMs: Long,
    var aside: Boolean,
  )

  private var raised: Raised? = null
  private var said: Pair<GuardedPage, String>? = null

  /** A window event from [packageName]; [page] is what its class name matched, for the packages the guard reads only. */
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

  /**
   * True while a raised guard holds for [packageName], or the last answer
   * guarded a page of it: a window event of it may have left the page, so
   * usage is read at once.
   */
  fun holdsFor(packageName: String?): Boolean =
    packageName != null && (raised?.packageName == packageName || said?.second == packageName)

  /** True while a page or the event raised the guard: only then does [now] need to know whether the phone is put down. */
  val isRaised: Boolean get() = raised != null

  /**
   * The guarded page in front now, from what is in front ([pick]), when its
   * activity resumed ([frontSinceWallMs], null with usage access off), the
   * guarded page that activity is by its class in Android's record
   * ([frontPage], null for any other and with usage access off), the
   * package that last sent a window event ([lastWindow]), whether usage can
   * be read ([usageOn]), whether the phone is put down ([putDown]: its
   * screen off, or Android's lock screen up) and how far the guard reaches
   * ([reach]). Logs one line when the answer changes.
   */
  fun now(
    pick: Pick,
    frontSinceWallMs: Long?,
    frontPage: GuardedPage?,
    lastWindow: String?,
    usageOn: Boolean,
    putDown: Boolean,
    nowMs: Long,
    reach: GuardReach,
  ): GuardedPage? {
    val inFront = if (usageOn) pick.packageName ?: lastWindow else lastWindow
    val r = raised
    if (pick.kind == FrontKind.LIMO || pick.kind == FrontKind.DIALLER || pick.kind == FrontKind.EMERGENCY_DIALLER) {
      r?.aside = true
      return answer(null, null, null)
    }
    val shown = pick.packageName
    if (usageOn && frontPage != null && shown != null) {
      // Android's record says the activity in front is a guarded page, by
      // its class: guarded, whatever told the service or did not. A raised
      // guard of the same app follows it: set aside while the phone is put
      // down, and taken up again by a newer resume of the page, so that
      // another page after it ends it as below.
      var by = "usage"
      if (r != null) {
        if (r.packageName == shown) {
          if (putDown) {
            r.aside = true
          } else if (frontSinceWallMs != null && frontSinceWallMs > r.sinceWallMs) {
            r.sinceWallMs = frontSinceWallMs
            r.aside = false
          }
          if (r.page == frontPage) by = r.by
        } else {
          raised = null
        }
      }
      return answer(frontPage, shown, by)
    }
    if (r != null) {
      // Usage has said nothing newer than the raise yet: what it shows in
      // front may be from before the page came up.
      val since = frontSinceWallMs.takeIf { usageOn }
      val older = since == null || since <= r.sinceWallMs
      val same = inFront == r.packageName || (r.packageName.isEmpty() && inFront == null)
      if (putDown) {
        // The phone put down: the page waits under the dark screen or the
        // lock screen, and its next resume is the page coming back. What
        // comes up over the lock screen meanwhile (its camera) is not the
        // page, and does not end it.
        r.aside = true
        if (same) return answer(r.page, r.packageName, r.by)
      } else if (same) {
        if (since == null || since <= r.sinceWallMs) return answer(r.page, r.packageName, r.by)
        if (!r.aside) {
          // Another page of the same app resumed: the guard ends.
          raised = null
        } else {
          // The same app came back after LiMo's own screen, a dialler or
          // the phone put down: the page that was guarded, guarded again.
          r.sinceWallMs = since
          r.aside = false
          return answer(r.page, r.packageName, r.by)
        }
      } else if (older && nowMs - r.sinceMs < WatchTimes.CONFIRM_GRACE_MS) {
        // Another app is in front by what usage last read: for a moment
        // after the raise that may be from before the page came up, unless
        // usage shows it resumed after the raise.
        return answer(r.page, r.packageName, r.by)
      } else {
        raised = null
      }
    }
    val pkg = if (usageOn) pick.packageName else lastWindow
    return answer(reach.wholePackage(pkg), pkg, "package")
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
