package ai.lucidloom.limo.child.dev

import ai.lucidloom.limo.child.watch.GuardReach
import ai.lucidloom.limo.child.watch.IntentAnswer
import ai.lucidloom.limo.child.watch.NamedActivity
import ai.lucidloom.limo.child.watch.PageIntent
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Before
import org.junit.Test

/**
 * The debug build's probe of page names: off until a person at the PC
 * switches it on, then a line for each window event of the packages the
 * guard reads windows of, each resumed activity of the packages it reads
 * Android's record of, once, and the package manager's answers; nothing for
 * any other app.
 */
class PageProbeTest {
  private val lines = ArrayList<String>()

  /** A phone whose package manager named a maker's own package for the device admin's page. */
  private val maker = "org.example.maker.security"
  private val reach = GuardReach.from(
    admin = IntentAnswer(null, listOf(NamedActivity(maker, "$maker.AdminPage", null, preinstalled = true))),
    accessibility = null,
    settings = null,
  )

  @Before
  fun quiet() {
    PageProbe.sink = { lines += it }
  }

  @After
  fun off() {
    PageProbe.switch(false)
  }

  @Test
  fun itIsOffUntilSwitchedOnAndLogsOnlyWhatTheGuardReads() {
    // Off by default: nothing.
    assertFalse(PageProbe.on)
    PageProbe.window(reach, "com.android.settings", "com.android.settings.SubSettings")
    PageProbe.resumed(reach, "com.android.settings", "com.android.settings.SubSettings")
    PageProbe.answered(mapOf(PageIntent.SETTINGS to IntentAnswer.NONE))
    assertEquals(emptyList<String>(), lines)

    PageProbe.switch(true)
    PageProbe.window(reach, "com.android.settings", "com.android.settings.applications.specialaccess.deviceadmin.DeviceAdminAdd")
    PageProbe.window(reach, "com.google.android.packageinstaller", "com.android.packageinstaller.UninstallerActivity")
    PageProbe.window(reach, "com.android.packageinstaller", null)
    PageProbe.window(reach, maker, "$maker.AdminPage")
    // Never another app's, whatever it is.
    PageProbe.window(reach, "org.example.game", "org.example.game.MainActivity")
    PageProbe.window(reach, "com.samsung.accessibility", "org.example.Page")
    PageProbe.window(reach, null, "org.example.Page")
    // A name is cut, and holds only what a class name can.
    PageProbe.window(reach, "com.android.settings", "com.android.settings.Page\nfake line " + "x".repeat(300))
    // Resumed activities: Settings' and the maker's, each once; never the installer's or a game's.
    PageProbe.resumed(reach, "com.android.settings", "com.android.settings.SubSettings")
    PageProbe.resumed(reach, "com.android.settings", "com.android.settings.SubSettings")
    PageProbe.resumed(reach, maker, "$maker.AdminPage")
    PageProbe.resumed(reach, "com.android.packageinstaller", "com.android.packageinstaller.UninstallerActivity")
    PageProbe.resumed(reach, "org.example.game", "org.example.game.MainActivity")
    // The package manager's answers, as the watch last had them.
    PageProbe.answered(
      mapOf(
        PageIntent.DEVICE_ADMIN to IntentAnswer(
          NamedActivity("com.android.settings", "com.android.settings.DeviceAdminAlias", "com.android.settings.DeviceAdminPage", preinstalled = true),
          listOf(NamedActivity("org.example.game", "org.example.game.Admin", null, preinstalled = false)),
        ),
        PageIntent.SETTINGS to IntentAnswer.NONE,
      ),
    )
    PageProbe.switch(false)
    PageProbe.window(reach, "com.android.settings", "com.android.settings.SubSettings")
    PageProbe.resumed(reach, maker, "$maker.Other")

    assertEquals(
      listOf(
        "probe on",
        "probe window package=com.android.settings class=com.android.settings.applications.specialaccess.deviceadmin.DeviceAdminAdd",
        "probe window package=com.google.android.packageinstaller class=com.android.packageinstaller.UninstallerActivity",
        "probe window package=com.android.packageinstaller class=none",
        "probe window package=$maker class=$maker.AdminPage",
        "probe window package=com.android.settings class=com.android.settings.Pagefakeline" + "x".repeat(164),
        "probe resumed package=com.android.settings class=com.android.settings.SubSettings",
        "probe resumed package=$maker class=$maker.AdminPage",
        "probe answered intent=device_admin opens=com.android.settings/com.android.settings.DeviceAdminAlias target=com.android.settings.DeviceAdminPage preinstalled",
        "probe answered intent=device_admin handler=org.example.game/org.example.game.Admin installed",
        "probe answered intent=accessibility unread",
        "probe answered intent=settings opens=none",
        "probe off",
      ),
      lines,
    )
  }

  @Test
  fun switchedOnAgainItLogsEachResumedActivityOnceMore() {
    PageProbe.switch(true)
    PageProbe.resumed(reach, "com.android.settings", "com.android.settings.SubSettings")
    PageProbe.switch(true)
    PageProbe.resumed(reach, "com.android.settings", "com.android.settings.SubSettings")
    assertEquals(2, lines.count { it.startsWith("probe resumed ") })
  }
}
