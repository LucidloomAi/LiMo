package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.ffi.FrontKind
import ai.lucidloom.limo.child.ffi.GuardedPage
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** A camera that opens over Android's lock screen. */
private const val CAMERA = "org.example.camera"

/** Settings' one activity for most of its pages (App info, a service's own page, Wi-Fi), as Android's source names it. */
private const val SUB_SETTINGS = "com.android.settings.SubSettings"

/** The device admin's page under a maker's own name, which no fixed list holds. */
private const val MAKER_PAGE = "org.example.maker.AdminPage"

/** A preinstalled activity, as the package manager names it. */
internal fun preinstalled(pkg: String, cls: String, target: String? = null) = NamedActivity(pkg, cls, target, preinstalled = true)

/** An activity of an app a person installed, as the package manager names it. */
internal fun installed(pkg: String, cls: String) = NamedActivity(pkg, cls, null, preinstalled = false)

/** The package manager's answer for an intent only these activities handle, the first one opening it. */
internal fun answering(vararg activities: NamedActivity) = IntentAnswer(activities.firstOrNull(), activities.toList())

/** A phone whose package manager names [page], in Settings, for the device admin's page. */
private fun makerPhone(page: String = MAKER_PAGE) =
  TestPhone().apply { answers[PageIntent.DEVICE_ADMIN] = answering(preinstalled(Guarded.SETTINGS, page)) }

/**
 * The guard's PIN, typed on LiMo's own screen: [pkg]'s activity pauses,
 * LiMo's resumes, then LiMo's closes and the same activity of [pkg] resumes
 * again. No window event at all.
 */
private fun Rig.pinOnLimosScreen(pkg: String) {
  phone.pass(500)
  phone.paused(pkg)
  phone.pass(10)
  phone.resumed(LIMO)
  core.limoScreen(true)
  phone.pass(6_000)
  core.limoScreen(false)
  phone.paused(LIMO)
  phone.pass(10)
  phone.resumed(pkg)
  phone.stopped(LIMO)
  phone.pass(20)
}

/**
 * The watch's part of the guard: which page that could switch LiMo off or
 * remove it is in front, by the page's name inside Settings (its window's,
 * and its activity's in Android's usage record), by what the package
 * manager names for the device admin's page and the accessibility settings,
 * by Android's event when its device admin is asked to switch off, and by a
 * package guarded whole; through real orders of window events and Android's
 * usage record, as on a phone.
 */
class GuardTest {
  private val settings = Guarded.SETTINGS

  /** The device admin's page, as Android 10 to 15 name it. */
  private val adminPage = "com.android.settings.applications.specialaccess.deviceadmin.DeviceAdminAdd"

  /** One UI's accessibility settings. */
  private val oneUiAccessibility = "com.samsung.accessibility"

  /** A window of Settings with [className], as the service hands it: matched, the name dropped. */
  private fun Rig.settingsWindow(className: String?): Boolean = window(settings, className)

  /** Another page of Settings comes up: its activity of [className] resumes, then its window says so. */
  private fun Rig.settingsPage(className: String): Boolean {
    phone.pass(10)
    phone.paused(settings)
    phone.pass(30)
    phone.resumed(settings, activity = className)
    phone.pass(20)
    return settingsWindow(className)
  }

  private fun Rig.guarded(): GuardedPage? = facts().guarded

  /** The three ways a guard is raised. */
  private enum class Raised { BY_PAGE, BY_EVENT, BY_PACKAGE }

  /** Brings up a page that is guarded as [how] says, from the home screen; returns the package it is in and the page. */
  private fun Rig.raise(how: Raised): Pair<String, GuardedPage> {
    open(LAUNCHER)
    return when (how) {
      Raised.BY_PAGE -> {
        open(settings)
        settingsPage(adminPage)
        settings to GuardedPage.DEVICE_ADMIN
      }
      Raised.BY_EVENT -> {
        // A device admin's page whose name is not on the list (another maker's), then Deactivate.
        open(settings)
        settingsPage(MAKER_PAGE)
        phone.pass(2_000)
        core.adminOffAsked()
        settings to GuardedPage.DEVICE_ADMIN
      }
      Raised.BY_PACKAGE -> {
        open(oneUiAccessibility)
        oneUiAccessibility to GuardedPage.ACCESSIBILITY
      }
    }
  }

  /**
   * The ways a page leaves the front and comes back as the same page, each
   * as Android does it, the host looking at every change of the screen as
   * its tick at the screen's signal does, and **no window event from the page
   * on its way back**.
   */
  private enum class Away(val play: Rig.(String) -> Unit) {
    /** The screen off, Android's lock screen locking a few seconds later, then on, the lock screen, unlocked. */
    SCREEN_OFF_AND_LOCK_SCREEN({ pkg ->
      phone.pass(1_000)
      phone.paused(pkg)
      phone.pass(5)
      phone.screenOn = false
      phone.stopped(pkg)
      look()
      phone.pass(5_000)
      phone.keyguard = true
      look()
      phone.pass(60_000)
      phone.screenOn = true
      look()
      phone.pass(3_000)
      phone.keyguard = false
      phone.pass(40)
      phone.resumed(pkg)
      phone.pass(20)
    }),

    /** The screen off and on again on a phone with no screen lock: no lock screen at all. */
    SCREEN_OFF_ONLY({ pkg ->
      phone.pass(1_000)
      phone.paused(pkg)
      phone.pass(5)
      phone.screenOn = false
      phone.stopped(pkg)
      look()
      phone.pass(30_000)
      phone.screenOn = true
      phone.pass(40)
      phone.resumed(pkg)
      phone.pass(20)
    }),

    /** Android's lock screen coming up while the screen stays on, then unlocked. */
    LOCK_SCREEN_ONLY({ pkg ->
      phone.pass(1_000)
      phone.keyguard = true
      phone.paused(pkg)
      phone.stopped(pkg)
      look()
      phone.pass(10_000)
      phone.keyguard = false
      phone.pass(40)
      phone.resumed(pkg)
      phone.pass(20)
    }),

    /** The screen off, then on over the lock screen, the lock screen's camera opened and closed there, then unlocked. */
    LOCK_SCREEN_CAMERA({ pkg ->
      phone.pass(1_000)
      phone.paused(pkg)
      phone.pass(5)
      phone.screenOn = false
      phone.keyguard = true
      phone.stopped(pkg)
      look()
      phone.pass(20_000)
      phone.screenOn = true
      look()
      phone.pass(1_000)
      phone.resumed(CAMERA)
      phone.pass(20)
      core.windowEvent(CAMERA)
      look()
      phone.pass(10_000)
      phone.paused(CAMERA)
      phone.stopped(CAMERA)
      look()
      phone.pass(2_000)
      phone.keyguard = false
      phone.pass(40)
      phone.resumed(pkg)
      phone.pass(20)
    }),

    /** LiMo's own screen over it and away again (the guard's PIN, or any of LiMo's screens). */
    LIMOS_SCREEN({ pkg -> pinOnLimosScreen(pkg) }),

    /** A call: its screen over the page while it rings and is on, then the call ends and the page comes back. */
    A_CALL({ pkg ->
      phone.pass(1_000)
      phone.audioMode = 1
      core.callChanged()
      phone.paused(pkg)
      phone.pass(30)
      phone.resumed(PHONE_APP)
      phone.pass(20)
      core.windowEvent(PHONE_APP)
      phone.pass(3_000)
      phone.audioMode = 2
      core.callChanged()
      phone.pass(60_000)
      phone.audioMode = 0
      core.callChanged()
      phone.paused(PHONE_APP)
      phone.pass(30)
      phone.resumed(pkg)
      phone.stopped(PHONE_APP)
      phone.pass(20)
    }),
  }

  @Test
  fun aGuardedPageThatComesBackIsGuardedAgainWhateverTookItAwayWithNoWindowEvent() {
    val failed = ArrayList<String>()
    for (how in Raised.values()) {
      for (away in Away.values()) {
        val rig = Rig()
        val (pkg, page) = rig.raise(how)
        assertEquals("$how", page, rig.guarded())
        // A guardian typed the PIN on LiMo's own screen and came back to the
        // page (the engine keeps it open; the watch still says what it is).
        rig.pinOnLimosScreen(pkg)
        assertEquals("$how after the PIN", page, rig.guarded())

        away.play(rig, pkg)
        val back = rig.guarded()
        if (back != page) failed += "$how $away: $back"

        // Then another page of Settings, or another app, ends it as before.
        if (pkg == settings) rig.settingsPage(SUB_SETTINGS) else rig.open(GAME)
        rig.phone.pass(WatchTimes.CONFIRM_GRACE_MS)
        assertNull("$how $away then elsewhere", rig.guarded())
      }
    }
    assertTrue("not guarded again: $failed", failed.isEmpty())
  }

  @Test
  fun androidsScreenSignalAloneKeepsTheGuardWhenNothingLooksWhileTheScreenIsOff() {
    for (how in listOf(Raised.BY_PAGE, Raised.BY_EVENT)) {
      val rig = Rig()
      val (pkg, page) = rig.raise(how)
      rig.pinOnLimosScreen(pkg)
      // The screen goes off: Android's signal tells the watch, and nothing
      // else looks until the phone is picked up.
      rig.phone.pass(1_000)
      rig.phone.paused(pkg)
      rig.phone.screenOn = false
      rig.phone.stopped(pkg)
      assertFalse("$how: still the page, under the dark screen", rig.core.screenChanged())
      rig.phone.pass(60_000)
      rig.phone.screenOn = true
      rig.phone.pass(40)
      rig.phone.resumed(pkg)
      rig.phone.pass(20)
      assertEquals("$how", page, rig.guarded())
    }
    // With no guard raised, a window event reads nothing of the screen: it costs no more than before.
    val plain = Rig()
    plain.open(LAUNCHER)
    plain.look()
    plain.phone.failing += listOf("interactive", "keyguard_locked")
    plain.open(GAME)
    plain.core.screenChanged()
    assertFalse(plain.log.lines.any { it.startsWith("fact interactive") || it.startsWith("fact keyguard_locked") })
  }

  /**
   * App info's Uninstall, for an active device admin, opens the device
   * admin's page in its removing form, whose one tap switches the admin off
   * and removes LiMo, and Android asks LiMo's receiver nothing on that way.
   * On a phone whose maker named that page otherwise, no fixed list knows
   * it; the package manager's answer does.
   */
  @Test
  fun aDeviceAdminPageNamedOnNoListIsGuardedWhenAppInfosUninstallOpensItWithNoEventAtAll() {
    val withAlias = TestPhone().apply {
      answers[PageIntent.DEVICE_ADMIN] = answering(preinstalled(settings, "org.example.maker.AdminAlias", target = MAKER_PAGE))
    }
    for ((name, phone) in listOf("named" to makerPhone(), "named by an alias" to withAlias, "not asked" to TestPhone())) {
      val rig = Rig(phone)
      rig.open(LAUNCHER)
      rig.open(settings, activity = "com.android.settings.Settings")
      // Apps, LiMo: App info, a page of Settings' shared activity.
      rig.open(settings, activity = SUB_SETTINGS)
      assertNull(name, rig.guarded())
      // Uninstall: the device admin's page resumes. No window event reaches
      // LiMo (the service missed it), and Android asks LiMo's receiver nothing.
      rig.phone.pass(2_000)
      rig.open(settings, activity = MAKER_PAGE, window = false)
      if (name == "not asked") {
        // Without Android's answer, a name on no list is not guarded.
        assertNull(name, rig.guarded())
        continue
      }
      assertEquals(name, GuardedPage.DEVICE_ADMIN, rig.guarded())
      assertTrue(name, rig.log.lines.contains("guard pages device_admin=named package=com.android.settings"))
      assertTrue(name, rig.log.lines.contains("guard now device_admin by=usage package=com.android.settings"))
      // Back to App info: another page, the guard ends.
      rig.open(settings, activity = SUB_SETTINGS)
      assertNull(name, rig.guarded())
      assertFalse(name, rig.log.lines.any { it.contains("AdminPage") || it.contains("AdminAlias") || it.contains("SubSettings") })
    }

    // With usage access off, the page's own window says it, by the same answer.
    val off = Rig(makerPhone())
    off.phone.usageOn = false
    off.look()
    off.window(settings, SUB_SETTINGS)
    assertNull(off.guarded())
    off.window(settings, MAKER_PAGE)
    assertEquals(GuardedPage.DEVICE_ADMIN, off.guarded())
  }

  @Test
  fun aDeviceAdminPageInAMakersOwnPackageIsGuardedAndOnlyThatPackagesClassesAreRead() {
    val security = "org.example.maker.security"
    val page = "org.example.maker.security.AdminPage"
    val phone = TestPhone().apply { answers[PageIntent.DEVICE_ADMIN] = answering(preinstalled(security, page)) }
    val rig = Rig(phone)
    rig.open(LAUNCHER)
    rig.open(GAME, activity = "org.example.game.Main")
    rig.open(security, activity = "org.example.maker.security.Home")
    assertNull(rig.guarded())
    rig.open(security, activity = page, window = false)
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    assertTrue(rig.log.lines.contains("guard pages device_admin=named package=$security"))
    rig.open(GAME, activity = page)
    rig.phone.pass(WatchTimes.CONFIRM_GRACE_MS)
    assertNull(rig.guarded())
    // Its window by the same name, with usage access off.
    rig.phone.usageOn = false
    rig.core.permissionsChanged()
    rig.look()
    rig.window(security, page)
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    // The classes read from the record: that package's and Settings', never the game's or the launcher's.
    assertTrue(phone.classReads.contains(security))
    assertTrue(phone.classReads.toString(), phone.classReads.all { it == security || it == settings })
  }

  /**
   * A guard raised by Android's question alone used to end at Home, and
   * back in Settings the question was still open, unguarded. Where
   * the package manager names the page, it is guarded again on the way back
   * by Android's record, with no window event; where Android names nothing
   * and the name is on no list, it is still open, as the README says.
   */
  @Test
  fun aGuardRaisedAtDeactivateHoldsOverAndroidsQuestionAfterHomeAndBack() {
    for (named in listOf(true, false)) {
      val phone = if (named) makerPhone() else TestPhone()
      val rig = Rig(phone)
      rig.open(LAUNCHER)
      rig.open(settings, activity = SUB_SETTINGS)
      rig.open(settings, activity = MAKER_PAGE)
      assertEquals("named=$named", if (named) GuardedPage.DEVICE_ADMIN else null, rig.guarded())
      // Deactivate: Android asks LiMo's receiver, then shows its question,
      // a dialog over the page that resumes nothing.
      phone.pass(2_000)
      rig.core.adminOffAsked()
      phone.pass(300)
      rig.settingsWindow("android.app.AlertDialog")
      phone.pass(5_000)
      assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
      // Home: the page and its question wait in Settings' task.
      rig.open(LAUNCHER)
      assertNull(rig.guarded())
      // Back through the recent apps: the same page resumes with its
      // question still up, and no window event reaches LiMo.
      phone.pass(4_000)
      rig.open(settings, activity = MAKER_PAGE, window = false)
      val back = rig.guarded()
      if (named) assertEquals(GuardedPage.DEVICE_ADMIN, back) else assertNull("left open where Android names nothing", back)
    }
  }

  @Test
  fun limosProcessStartingAgainWithAGuardedPageInFrontGuardsItAtItsFirstLook() {
    for (how in Raised.values()) {
      for (named in listOf(false, true)) {
        val phone = if (named) makerPhone() else TestPhone()
        val (_, page) = Rig(phone).raise(how)
        phone.pass(5_000)
        // LiMo's process ends and starts again with the page still in front,
        // which tells nothing: a new watch on the same phone.
        val again = Rig(phone)
        val first = again.guarded()
        if (how == Raised.BY_EVENT && !named) {
          // A page known by Android's question alone, its name on no list
          // and Android naming nothing: nothing of the raise outlived the
          // process. Guarded again only when Deactivate asks once more.
          assertNull("$how", first)
          phone.pass(2_000)
          again.core.adminOffAsked()
          assertEquals("$how", page, again.guarded())
          continue
        }
        // Android's record says which package is in front, and which page it
        // is by its class: on the fixed list, or as Android named it.
        assertEquals("$how named=$named", page, first)
      }
    }
  }

  @Test
  fun theDeviceAdminsPageIsGuardedByItsNameAndADialogOverItKeepsItGuarded() {
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.open(settings)
    assertNull(rig.guarded())
    rig.settingsPage(SUB_SETTINGS)
    assertNull(rig.guarded())

    // LiMo's row under Device admin apps: the device admin's page.
    assertTrue(rig.settingsPage(adminPage))
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())

    // Android's own "Deactivate?" over it: a window of Settings with a
    // dialog's class, and nothing resumed. Still guarded, minutes later.
    rig.phone.pass(2_000)
    rig.settingsWindow("android.app.AlertDialog")
    rig.phone.pass(120_000)
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())

    // Back to the list: another page of Settings resumed. Not guarded.
    rig.phone.pass(1_000)
    rig.phone.resumed(settings, activity = "com.android.settings.Settings\$DeviceAdminSettingsActivity")
    rig.phone.pass(20)
    assertTrue(rig.settingsWindow("com.android.settings.Settings\$DeviceAdminSettingsActivity"))
    assertNull(rig.guarded())

    // Android 8.0 to 9 name the same page otherwise: guarded too.
    rig.settingsPage("com.android.settings.DeviceAdminAdd")
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    // Home: not guarded.
    rig.open(LAUNCHER)
    assertNull(rig.guarded())

    // The log names the page and the package, never a class name.
    assertTrue(rig.log.lines.contains("guard now device_admin by=page package=com.android.settings"))
    assertTrue(rig.log.lines.contains("guard now none"))
    for (word in listOf("DeviceAdminAdd", "AlertDialog", "SubSettings", "DeviceAdminSettings")) {
      assertFalse(word, rig.log.lines.any { it.contains(word) })
    }
  }

  @Test
  fun aPageWhoseOwnResumeAndroidWritesLateIsGuardedAtOnceAndALateWindowOverHomeRaisesNothing() {
    val rig = Rig()
    rig.open(LAUNCHER)
    // The device admin's page straight from a notification, Settings' resume
    // written after its window told the service: guarded at once all the same.
    rig.phone.pass(30)
    rig.phone.resumed(settings, late = true, activity = adminPage)
    rig.phone.pass(20)
    assertTrue(rig.settingsWindow(adminPage))
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    rig.phone.pass(300)
    rig.phone.catchUp()
    rig.core.recheck()
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    rig.phone.pass(10_000)
    assertEquals("Settings' resume came before the page's window", GuardedPage.DEVICE_ADMIN, rig.guarded())

    // Home: Settings pauses, the home screen resumes, Settings stops. A late
    // window of that page after it raises nothing at all: usage, read at the
    // window, shows Settings left just before.
    rig.phone.pass(10)
    rig.phone.paused(settings)
    rig.phone.pass(30)
    rig.phone.resumed(LAUNCHER)
    rig.phone.stopped(settings)
    rig.phone.pass(20)
    rig.core.windowEvent(LAUNCHER)
    assertNull(rig.guarded())
    rig.phone.pass(50)
    assertFalse("nothing to tell", rig.settingsWindow(adminPage))
    assertNull(rig.guarded())
    rig.settle()
    assertNull(rig.guarded())
  }

  @Test
  fun limosOwnScreenAndTheDiallersSetTheGuardAsideAndTheSamePageIsGuardedAgainOnItsReturn() {
    val phone = TestPhone().apply { emergency = setOf(SOS) }
    val rig = Rig(phone)
    rig.open(LAUNCHER)
    rig.open(settings)
    rig.settingsPage(adminPage)
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())

    // The guard's PIN, on LiMo's own screen: never guarded, never covered,
    // though Android's record still shows the page until LiMo's resume is read.
    phone.pass(500)
    phone.paused(settings)
    phone.pass(10)
    rig.core.limoScreen(true)
    assertNull(rig.guarded())
    phone.resumed(LIMO)
    assertNull(rig.guarded())
    // Back, with no PIN: the same page resumes again, with or without a
    // window event, and is guarded again.
    phone.pass(3_000)
    rig.core.limoScreen(false)
    phone.paused(LIMO)
    phone.pass(10)
    phone.resumed(settings)
    phone.pass(20)
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())

    // Emergency call from the guard: the emergency dialler, then back.
    phone.pass(2_000)
    rig.core.emergencyOpening()
    phone.pass(40)
    phone.paused(settings)
    phone.resumed(SOS)
    rig.core.windowEvent(SOS)
    assertNull(rig.guarded())
    phone.pass(5_000)
    phone.paused(SOS)
    phone.resumed(settings, activity = adminPage)
    phone.pass(20)
    rig.settingsWindow(adminPage)
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())

    // Then another page of Settings: the guard ends.
    phone.pass(2_000)
    rig.settingsPage(SUB_SETTINGS)
    assertNull(rig.guarded())
  }

  @Test
  fun androidAskingTheDeviceAdminToSwitchOffRaisesTheGuardOverItsQuestion() {
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.open(settings)
    // A device admin's page whose name is not on the list (another maker's).
    rig.settingsPage(MAKER_PAGE)
    assertNull(rig.guarded())
    rig.phone.pass(4_000)

    // Deactivate: Android asks LiMo's receiver, and the guard is up at once.
    assertTrue(rig.core.adminOffAsked())
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    assertTrue(rig.log.lines.contains("guard now device_admin by=event package=com.android.settings"))
    // Android's question over the page resumes nothing: guarded until it is answered.
    rig.phone.pass(500)
    rig.settingsWindow("android.app.AlertDialog")
    rig.phone.pass(60_000)
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    // Home: the guard ends.
    rig.open(LAUNCHER)
    assertNull(rig.guarded())
    // Asked with the home screen in front (it cannot be, but if it were): only for the home screen, until something else comes up.
    assertTrue(rig.core.adminOffAsked())
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    rig.open(GAME)
    rig.phone.pass(WatchTimes.CONFIRM_GRACE_MS)
    assertNull(rig.guarded())
  }

  @Test
  fun aPackageGuardedWholeIsGuardedWhileUsageShowsItAndWithUsageOffWhileItLastSaidSo() {
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.open(oneUiAccessibility)
    assertEquals(GuardedPage.ACCESSIBILITY, rig.guarded())
    assertTrue(rig.log.lines.contains("guard now accessibility by=package package=com.samsung.accessibility"))
    rig.open(GAME)
    assertNull(rig.guarded())
    // A late window of it over the game, never confirmed: not guarded.
    rig.core.windowEvent(oneUiAccessibility)
    assertNull(rig.guarded())
    rig.settle()

    // Usage access off: the last window event says what is in front.
    rig.phone.usageOn = false
    rig.core.permissionsChanged()
    rig.look()
    rig.core.windowEvent(oneUiAccessibility)
    assertEquals(GuardedPage.ACCESSIBILITY, rig.guarded())
    rig.core.windowEvent(GAME)
    assertNull(rig.guarded())
    // And a page of Settings named on the list, until another app's window.
    rig.settingsWindow(adminPage)
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    rig.settingsWindow("android.app.AlertDialog")
    rig.phone.pass(30_000)
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    rig.core.windowEvent(GAME)
    assertNull(rig.guarded())
  }

  /**
   * The Galaxy, 21 November 2026: setup's Accessibility step opened One
   * UI's accessibility settings, whose first window came 2.2 seconds before
   * Android's usage record showed them resumed (a cold start), and the page
   * was bare all that while. Now the window itself raises the guard.
   */
  @Test
  fun aPackageGuardedWholeIsGuardedAtItsWindowWhileUsageStillShowsLimosSetupScreen() {
    val rig = Rig()
    rig.open(LAUNCHER)
    // LiMo's own setup screen, in front by usage and by LiMo's own word.
    rig.phone.pass(10)
    rig.phone.paused(LAUNCHER)
    rig.phone.pass(30)
    rig.phone.resumed(LIMO)
    rig.core.limoScreen(true)
    rig.phone.pass(20)
    rig.phone.stopped(LAUNCHER)
    assertNull(rig.guarded())
    // The Accessibility step: LiMo's screen pauses and says so, and the
    // settings' first window comes at once.
    rig.phone.pass(3_000)
    rig.core.limoScreen(false)
    rig.phone.paused(LIMO)
    rig.phone.pass(30)
    assertTrue("told at the window", rig.core.windowEvent(oneUiAccessibility))
    assertEquals(GuardedPage.ACCESSIBILITY, rig.guarded())
    assertTrue(rig.log.lines.contains("guard now accessibility by=window package=com.samsung.accessibility"))
    // The rechecks give up on the front; the guard holds, past a second.
    rig.settle()
    assertTrue(rig.log.lines.any { it.startsWith("front window_event package=com.samsung.accessibility not_confirmed front=$LIMO") })
    rig.phone.pass(1_000)
    assertEquals(GuardedPage.ACCESSIBILITY, rig.guarded())
    // Usage shows the settings resumed, 2.2 seconds after their window: guarded throughout, by the package now.
    rig.phone.pass(10)
    rig.phone.resumed(oneUiAccessibility)
    rig.phone.stopped(LIMO)
    rig.phone.pass(600)
    assertEquals(GuardedPage.ACCESSIBILITY, rig.guarded())
    assertEquals(FrontKind.APP, rig.front().kind)
    assertEquals(1, rig.log.lines.count { it.startsWith("guard now accessibility") })
    assertEquals(0, rig.log.lines.count { it == "guard now none" })
  }

  @Test
  fun aPageByItsClassIsGuardedAtItsWindowAndHeldPastASecondUntilItResumes() {
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.open(GAME)
    // The device admin's page straight from a notification over the game,
    // Settings started cold: its window first, its activity resumed two
    // seconds later.
    rig.phone.pass(10)
    rig.phone.paused(GAME)
    rig.phone.pass(30)
    assertTrue(rig.settingsWindow(adminPage))
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    rig.settle()
    rig.phone.pass(500)
    assertEquals("past a second, usage still showing the game", GuardedPage.DEVICE_ADMIN, rig.guarded())
    rig.phone.pass(500)
    rig.phone.resumed(settings, activity = adminPage)
    rig.phone.stopped(GAME)
    rig.phone.pass(600)
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    assertEquals(1, rig.log.lines.count { it.startsWith("guard now device_admin") })
  }

  @Test
  fun aGuardRaisedAtAWindowIsTakenBackWhenUsageShowsAnotherAppAfterItOrAfterFiveSeconds() {
    // Another app resumed after the window: taken back at the first read that shows it.
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.phone.pass(10)
    rig.phone.paused(LAUNCHER)
    rig.phone.pass(30)
    assertTrue(rig.core.windowEvent(oneUiAccessibility))
    assertEquals(GuardedPage.ACCESSIBILITY, rig.guarded())
    rig.phone.pass(300)
    rig.phone.resumed(GAME)
    rig.phone.pass(20)
    assertTrue(rig.core.windowEvent(GAME))
    assertNull(rig.guarded())
    // With no window event from it, at the host's next look.
    val quiet = Rig()
    quiet.open(LAUNCHER)
    quiet.phone.pass(10)
    quiet.phone.paused(LAUNCHER)
    quiet.phone.pass(30)
    quiet.core.windowEvent(oneUiAccessibility)
    quiet.phone.pass(300)
    quiet.phone.resumed(GAME)
    quiet.phone.pass(600)
    assertNull(quiet.guarded())
    // A window of it that never resumes (a floating window over the game):
    // guarded at most five seconds, then not.
    val floating = Rig()
    floating.open(LAUNCHER)
    floating.open(GAME)
    floating.phone.pass(60_000)
    floating.core.windowEvent(oneUiAccessibility)
    assertEquals(GuardedPage.ACCESSIBILITY, floating.guarded())
    floating.settle()
    floating.phone.pass(WatchTimes.GUARD_WINDOW_HOLD_MS - 1_500)
    assertEquals(GuardedPage.ACCESSIBILITY, floating.guarded())
    floating.phone.pass(1_000)
    assertNull(floating.guarded())
    assertEquals(GAME, floating.front().packageName)
  }

  /**
   * Proof two's flaw, for a guarded package: after "Go home" the app that
   * was in front sent one more window event after the home screen's. Here
   * that app is One UI's accessibility settings, and its late window must
   * not leave a guard over the home screen; fourteen tries, as on the first
   * phone.
   */
  @Test
  fun aLateWindowFromAGuardedPackageAfterGoingHomeRaisesNothing() {
    val rig = Rig()
    rig.open(LAUNCHER)
    repeat(14) { round ->
      rig.open(oneUiAccessibility)
      assertEquals("round $round", GuardedPage.ACCESSIBILITY, rig.guarded())
      // Home: the settings pause, the home screen resumes, they stop; then their late window.
      rig.open(LAUNCHER)
      assertNull("round $round", rig.guarded())
      rig.phone.pass(50)
      assertFalse("round $round: nothing told", rig.core.windowEvent(oneUiAccessibility))
      assertNull("round $round", rig.guarded())
      rig.settle()
      rig.phone.pass(2_000)
      assertNull("round $round", rig.guarded())
      assertEquals(FrontKind.HOME, rig.front().kind)
      rig.phone.pass(10_000)
    }
    // The guard went up once in each round, by the package, and never by a late window.
    assertEquals(14, rig.log.lines.count { it == "guard now accessibility by=package package=com.samsung.accessibility" })
    assertFalse(rig.log.lines.any { it.contains("by=window") })

    // The home screen's resume written late, the late window before it:
    // usage still shows the settings in front, so they are guarded as by
    // usage alone, and not once usage shows the home screen.
    val late = Rig()
    late.open(LAUNCHER)
    late.open(oneUiAccessibility)
    late.phone.pass(10)
    late.phone.paused(oneUiAccessibility)
    late.phone.pass(30)
    late.phone.resumed(LAUNCHER, late = true)
    late.phone.pass(20)
    late.core.windowEvent(LAUNCHER)
    late.phone.pass(50)
    late.core.windowEvent(oneUiAccessibility)
    late.phone.pass(200)
    late.phone.catchUp()
    late.phone.stopped(oneUiAccessibility)
    late.core.recheck()
    late.phone.pass(600)
    assertNull(late.guarded())
    assertEquals(FrontKind.HOME, late.front().kind)
  }

  @Test
  fun aWindowRaisedGuardNeverCoversLimosOwnScreenNorADialler() {
    // The guard's PIN on LiMo's own screen, a late window of the settings under it: LiMo's word wins.
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.open(oneUiAccessibility)
    rig.phone.pass(500)
    rig.phone.paused(oneUiAccessibility)
    rig.core.limoScreen(true)
    rig.phone.pass(10)
    rig.core.windowEvent(oneUiAccessibility)
    assertNull(rig.guarded())
    rig.phone.resumed(LIMO)
    rig.phone.pass(600)
    assertNull(rig.guarded())
    // A call's screen in front by usage: a window of the settings over it raises nothing while it is in front.
    val call = Rig()
    call.open(LAUNCHER)
    call.phone.audioMode = 1
    call.core.callChanged()
    call.open(PHONE_APP)
    call.phone.pass(5_000)
    call.core.windowEvent(oneUiAccessibility)
    assertNull(call.guarded())
  }

  @Test
  fun theAccessibilitySettingsAreGuardedWholeWhereAndroidOpensThemInAPackageOfTheirOwn() {
    val access = "org.example.maker.accessibility"
    val phone = TestPhone().apply {
      answers[PageIntent.ACCESSIBILITY] = answering(preinstalled(access, "org.example.maker.accessibility.Main"))
      answers[PageIntent.SETTINGS] = answering(preinstalled(settings, "com.android.settings.Settings"))
    }
    val rig = Rig(phone)
    rig.open(LAUNCHER)
    assertEquals(GuardScope(deviceAdminNamed = false, accessibility = AccessibilityPlace.OWN_PACKAGE), rig.look().guardScope)
    assertTrue(rig.log.lines.contains("guard pages accessibility=own_package package=$access"))
    // In front: guarded whole, by what Android answered, with no maker's name held anywhere.
    rig.open(access, activity = "org.example.maker.accessibility.Main")
    assertEquals(GuardedPage.ACCESSIBILITY, rig.guarded())
    assertTrue(rig.log.lines.contains("guard now accessibility by=package package=$access"))
    rig.open(GAME)
    assertNull(rig.guarded())
    // Its classes are never read: a package guarded whole needs none.
    assertFalse(phone.classReads.contains(access))
    // After a restart of LiMo's process with it in front: guarded at the first look.
    rig.open(access)
    phone.pass(5_000)
    assertEquals(GuardedPage.ACCESSIBILITY, Rig(phone).guarded())
  }

  @Test
  fun accessibilitySettingsInsideAndroidsSettingsAreReportedAsTheyAreAndNotGuarded() {
    // Android's own source: the action opens an activity of Settings, and
    // LiMo's switch is on a page in Settings' shared activity.
    val aosp = TestPhone().apply {
      answers[PageIntent.ACCESSIBILITY] = answering(preinstalled(settings, "com.android.settings.Settings\$AccessibilitySettingsActivity"))
      answers[PageIntent.SETTINGS] =
        answering(preinstalled(settings, "com.android.settings.Settings", target = "com.android.settings.homepage.SettingsHomepageActivity"))
    }
    val rig = Rig(aosp)
    rig.open(LAUNCHER)
    assertEquals(AccessibilityPlace.IN_SETTINGS, rig.look().guardScope.accessibility)
    assertTrue(rig.log.lines.contains("guard pages accessibility=in_settings package=com.android.settings"))
    rig.open(settings, activity = "com.android.settings.Settings\$AccessibilitySettingsActivity")
    assertNull(rig.guarded())
    rig.open(settings, activity = SUB_SETTINGS)
    assertNull(rig.guarded())

    // A maker whose Settings app is a package of its own, the accessibility
    // settings inside it: that Settings is never guarded whole.
    val makerSettings = "org.example.maker.settings"
    val maker = TestPhone().apply {
      answers[PageIntent.ACCESSIBILITY] = answering(preinstalled(makerSettings, "org.example.maker.settings.Accessibility"))
      answers[PageIntent.SETTINGS] = answering(preinstalled(makerSettings, "org.example.maker.settings.Home"))
    }
    val inMaker = Rig(maker)
    inMaker.open(LAUNCHER)
    assertEquals(AccessibilityPlace.IN_SETTINGS, inMaker.look().guardScope.accessibility)
    inMaker.open(makerSettings)
    assertNull(inMaker.guarded())

    // Android's Settings could not be asked: no package can be told from
    // Settings, so none is guarded whole; the fixed list alone.
    val refused = TestPhone().apply {
      answers[PageIntent.ACCESSIBILITY] = answering(preinstalled(makerSettings, "org.example.maker.settings.Accessibility"))
      refusedIntents += PageIntent.SETTINGS
    }
    val unsure = Rig(refused)
    unsure.open(LAUNCHER)
    assertEquals(AccessibilityPlace.NOT_NAMED, unsure.look().guardScope.accessibility)
    unsure.open(makerSettings)
    assertNull(unsure.guarded())
    unsure.open(oneUiAccessibility)
    assertEquals(GuardedPage.ACCESSIBILITY, unsure.guarded())

    // Android named nothing: the fixed list alone, said as it is.
    val bare = Rig()
    bare.open(LAUNCHER)
    assertEquals(GuardScope(), bare.look().guardScope)
    assertTrue(bare.log.lines.contains("guard pages device_admin=not_named"))
    assertTrue(bare.log.lines.contains("guard pages accessibility=not_named"))
  }

  @Test
  fun androidsAnswerIsKeptForTheProcessAndAskedAgainWhenAppsChangeOrAfterARefusal() {
    val phone = makerPhone()
    val rig = Rig(phone)
    rig.open(LAUNCHER)
    val once = phone.answerReads
    assertEquals(PageIntent.values().size, once)
    repeat(30) {
      phone.pass(1_000)
      rig.look()
    }
    assertEquals("kept, not asked at every look", once, phone.answerReads)

    // An update renames the page: Android says an app changed, and the
    // package manager is asked again.
    val renamed = "org.example.maker.NewAdminPage"
    phone.answers[PageIntent.DEVICE_ADMIN] = answering(preinstalled(settings, renamed))
    rig.core.appsChanged()
    rig.open(settings, activity = renamed, window = false)
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    assertEquals(2 * once, phone.answerReads)
    rig.open(settings, activity = MAKER_PAGE, window = false)
    assertNull("the old name is no page now", rig.guarded())

    // Refused: the last good answer holds, logged once, asked again a minute later.
    phone.failing += "guard_pages"
    rig.core.appsChanged()
    rig.look()
    assertEquals(setOf(settings to renamed), rig.core.reachNow().adminPages)
    assertEquals(1, rig.log.lines.count { it.startsWith("fact guard_pages unreadable error=SecurityException fallback_used") })
    phone.failing.clear()
    phone.answers[PageIntent.DEVICE_ADMIN] = answering(preinstalled(settings, MAKER_PAGE))
    phone.pass(30_000)
    rig.look()
    assertEquals(setOf(settings to renamed), rig.core.reachNow().adminPages)
    phone.pass(31_000)
    rig.look()
    assertEquals(setOf(settings to MAKER_PAGE), rig.core.reachNow().adminPages)

    // A new process that Android refuses from the first: the fixed lists
    // alone, Android's own names still guarded.
    val refusing = TestPhone().apply { failing += "guard_pages" }
    val fresh = Rig(refusing)
    fresh.open(LAUNCHER)
    fresh.open(settings, activity = adminPage, window = false)
    assertEquals(GuardedPage.DEVICE_ADMIN, fresh.guarded())
    assertEquals(GuardScope(), fresh.look().guardScope)
  }

  @Test
  fun anActivityThatAlsoOpensSettingsOwnPagesIsNeverTakenForTheDeviceAdminsPage() {
    val phone = TestPhone().apply {
      answers[PageIntent.DEVICE_ADMIN] = answering(preinstalled(settings, "com.android.settings.Settings"))
      answers[PageIntent.SETTINGS] = answering(preinstalled(settings, "com.android.settings.Settings"))
    }
    val rig = Rig(phone)
    rig.open(LAUNCHER)
    rig.open(settings, activity = "com.android.settings.Settings")
    assertNull(rig.guarded())
    assertTrue(rig.log.lines.contains("guard pages device_admin=not_named"))
    assertEquals(GuardScope(), rig.look().guardScope)
  }

  @Test
  fun aClassNameIsReadForSettingsAndTheInstallerOnlyAndMatchedOnlyAgainstTheFixedList() {
    assertEquals(
      setOf("com.android.settings", "com.android.packageinstaller", "com.google.android.packageinstaller"),
      Guarded.CLASS_NAME_PACKAGES,
    )
    assertTrue(Guarded.PAGES.keys.all { it.first in Guarded.CLASS_NAME_PACKAGES })
    assertTrue(Guarded.PACKAGES.keys.none { it in Guarded.CLASS_NAME_PACKAGES })
    for (pkg in Guarded.CLASS_NAME_PACKAGES) assertTrue(Guarded.readsClassName(pkg))
    for (pkg in listOf(GAME, oneUiAccessibility, LIMO, SYSTEM_UI, "")) assertFalse(pkg, Guarded.readsClassName(pkg))
    assertFalse(Guarded.readsClassName(null))
    // The same class name from any other app is nothing.
    assertNull(Guarded.page(GAME, adminPage))
    assertNull(Guarded.page(oneUiAccessibility, adminPage))
    assertEquals(GuardedPage.DEVICE_ADMIN, Guarded.page(settings, adminPage))
    assertEquals(GuardedPage.DEVICE_ADMIN, Guarded.page(settings, "com.android.settings.DeviceAdminAdd"))
    assertNull(Guarded.page(settings, SUB_SETTINGS))
    assertNull(Guarded.page(settings, null))
    assertNull(Guarded.page("com.android.packageinstaller", adminPage))
    // The fixed lists alone read the same.
    val fixed = GuardReach.FIXED
    for (pkg in Guarded.CLASS_NAME_PACKAGES) assertTrue(fixed.readsClassName(pkg))
    assertEquals(setOf(settings), Guarded.CLASS_NAME_PACKAGES.filter { fixed.readsResumedClassName(it) }.toSet())
    assertEquals(GuardedPage.DEVICE_ADMIN, fixed.page(settings, adminPage))
    assertNull(fixed.page(GAME, adminPage))
    assertEquals(GuardedPage.ACCESSIBILITY, fixed.wholePackage(oneUiAccessibility))
    assertNull(fixed.wholePackage(settings))
  }
}
