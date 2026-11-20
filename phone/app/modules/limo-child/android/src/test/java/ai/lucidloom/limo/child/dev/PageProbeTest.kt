package ai.lucidloom.limo.child.dev

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Before
import org.junit.Test

/**
 * The debug build's probe of page names: off until a person at the PC
 * switches it on, then a line for each window event of Android's Settings
 * and the installer, and nothing for any other app.
 */
class PageProbeTest {
  private val lines = ArrayList<String>()

  @Before
  fun quiet() {
    PageProbe.sink = { lines += it }
  }

  @After
  fun off() {
    PageProbe.switch(false)
  }

  @Test
  fun itIsOffUntilSwitchedOnAndLogsOnlySettingsAndTheInstaller() {
    // Off by default: nothing.
    assertFalse(PageProbe.on)
    PageProbe.window("com.android.settings", "com.android.settings.SubSettings")
    assertEquals(emptyList<String>(), lines)

    PageProbe.switch(true)
    PageProbe.window("com.android.settings", "com.android.settings.applications.specialaccess.deviceadmin.DeviceAdminAdd")
    PageProbe.window("com.google.android.packageinstaller", "com.android.packageinstaller.UninstallerActivity")
    PageProbe.window("com.android.packageinstaller", null)
    // Never another app's, whatever it is.
    PageProbe.window("org.example.game", "org.example.game.MainActivity")
    PageProbe.window("com.samsung.accessibility", "org.example.Page")
    PageProbe.window(null, "org.example.Page")
    // A name is cut, and holds only what a class name can.
    PageProbe.window("com.android.settings", "com.android.settings.Page\nfake line " + "x".repeat(300))
    PageProbe.switch(false)
    PageProbe.window("com.android.settings", "com.android.settings.SubSettings")

    assertEquals(
      listOf(
        "probe on",
        "probe window package=com.android.settings class=com.android.settings.applications.specialaccess.deviceadmin.DeviceAdminAdd",
        "probe window package=com.google.android.packageinstaller class=com.android.packageinstaller.UninstallerActivity",
        "probe window package=com.android.packageinstaller class=none",
        "probe window package=com.android.settings class=com.android.settings.Pagefakeline" + "x".repeat(164),
        "probe off",
      ),
      lines,
    )
  }
}
