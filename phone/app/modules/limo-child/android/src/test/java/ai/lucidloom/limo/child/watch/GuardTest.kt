package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.ffi.GuardedPage
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** A camera that opens over Android's lock screen. */
private const val CAMERA = "org.example.camera"

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
 * remove it is in front, by the page's name inside Settings, by Android's
 * event when its device admin is asked to switch off, and by a package
 * guarded whole; through real orders of window events and Android's usage
 * record, as on a phone.
 */
class GuardTest {
  private val settings = Guarded.SETTINGS

  /** The device admin's page, as Android 10 to 15 name it. */
  private val adminPage = "com.android.settings.applications.specialaccess.deviceadmin.DeviceAdminAdd"

  /** One UI's accessibility settings. */
  private val oneUiAccessibility = "com.samsung.accessibility"

  /** A window of Settings with [className], as the service hands it: matched against the list, the name dropped. */
  private fun Rig.window(className: String?): Boolean = core.windowEvent(settings, Guarded.page(settings, className))

  /** Another page of Settings comes up: its activity resumes, then its window says so with [className]. */
  private fun Rig.settingsPage(className: String): Boolean {
    phone.pass(10)
    phone.paused(settings)
    phone.pass(30)
    phone.resumed(settings)
    phone.pass(20)
    return window(className)
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
        settingsPage("org.example.maker.AdminPage")
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
        if (pkg == settings) rig.settingsPage("com.android.settings.SubSettings") else rig.open(GAME)
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

  @Test
  fun afterARestartOfLimosProcessAPackageIsGuardedAtOnceAndAPageOnlyWhenItTellsAgain() {
    for (how in Raised.values()) {
      val phone = TestPhone()
      val (_, page) = Rig(phone).raise(how)
      phone.pass(5_000)
      // LiMo's process ends and starts again with the page still in front,
      // which tells nothing: a new watch on the same phone.
      val again = Rig(phone)
      if (how == Raised.BY_PACKAGE) {
        // Android's record says which package is in front: guarded at once.
        assertEquals(page, again.guarded())
        continue
      }
      // What a new process cannot know: nothing of the raise outlives the
      // process, and Android's record says only that Settings is in front.
      assertNull("$how", again.guarded())
      // Guarded again as soon as the page tells again: its window by name
      // (its focus back after the shade, say), or Deactivate asking once more.
      phone.pass(2_000)
      if (how == Raised.BY_PAGE) again.window(adminPage) else again.core.adminOffAsked()
      assertEquals("$how", page, again.guarded())
    }
  }

  @Test
  fun theDeviceAdminsPageIsGuardedByItsNameAndADialogOverItKeepsItGuarded() {
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.open(settings)
    assertNull(rig.guarded())
    rig.settingsPage("com.android.settings.SubSettings")
    assertNull(rig.guarded())

    // LiMo's row under Device admin apps: the device admin's page.
    assertTrue(rig.settingsPage(adminPage))
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())

    // Android's own "Deactivate?" over it: a window of Settings with a
    // dialog's class, and nothing resumed. Still guarded, minutes later.
    rig.phone.pass(2_000)
    rig.window("android.app.AlertDialog")
    rig.phone.pass(120_000)
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())

    // Back to the list: another page of Settings resumed. Not guarded.
    rig.phone.pass(1_000)
    rig.phone.resumed(settings)
    rig.phone.pass(20)
    assertTrue(rig.window("com.android.settings.Settings\$DeviceAdminSettingsActivity"))
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
  fun aPageWhoseOwnResumeAndroidWritesLateIsGuardedAtOnceAndALateWindowOverHomeOnlyForAMoment() {
    val rig = Rig()
    rig.open(LAUNCHER)
    // The device admin's page straight from a notification, Settings' resume
    // written after its window told the service: guarded at once all the same.
    rig.phone.pass(30)
    rig.phone.resumed(settings, late = true)
    rig.phone.pass(20)
    assertTrue(rig.window(adminPage))
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    rig.phone.pass(300)
    rig.phone.catchUp()
    rig.core.recheck()
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    rig.phone.pass(10_000)
    assertEquals("Settings' resume came before the page's window", GuardedPage.DEVICE_ADMIN, rig.guarded())

    // A late window of that page after the home screen came up: guarded for
    // the moment usage takes to say so, then not.
    rig.open(LAUNCHER)
    assertNull(rig.guarded())
    rig.phone.pass(50)
    rig.window(adminPage)
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    rig.settle()
    rig.phone.pass(WatchTimes.CONFIRM_GRACE_MS)
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

    // The guard's PIN, on LiMo's own screen: never guarded, never covered.
    phone.pass(500)
    phone.paused(settings)
    phone.pass(10)
    phone.resumed(LIMO)
    rig.core.limoScreen(true)
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
    phone.resumed(settings)
    phone.pass(20)
    rig.window(adminPage)
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())

    // Then another page of Settings: the guard ends.
    phone.pass(2_000)
    rig.settingsPage("com.android.settings.SubSettings")
    assertNull(rig.guarded())
  }

  @Test
  fun androidAskingTheDeviceAdminToSwitchOffRaisesTheGuardOverItsQuestion() {
    val rig = Rig()
    rig.open(LAUNCHER)
    rig.open(settings)
    // A device admin's page whose name is not on the list (another maker's).
    rig.settingsPage("org.example.maker.AdminPage")
    assertNull(rig.guarded())
    rig.phone.pass(4_000)

    // Deactivate: Android asks LiMo's receiver, and the guard is up at once.
    assertTrue(rig.core.adminOffAsked())
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    assertTrue(rig.log.lines.contains("guard now device_admin by=event package=com.android.settings"))
    // Android's question over the page resumes nothing: guarded until it is answered.
    rig.phone.pass(500)
    rig.window("android.app.AlertDialog")
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
    rig.window(adminPage)
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    rig.window("android.app.AlertDialog")
    rig.phone.pass(30_000)
    assertEquals(GuardedPage.DEVICE_ADMIN, rig.guarded())
    rig.core.windowEvent(GAME)
    assertNull(rig.guarded())
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
    assertNull(Guarded.page(settings, "com.android.settings.SubSettings"))
    assertNull(Guarded.page(settings, null))
    assertNull(Guarded.page("com.android.packageinstaller", adminPage))
  }
}
