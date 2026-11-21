package ai.lucidloom.limo.child.host

import ai.lucidloom.limo.child.ffi.CoverKind
import ai.lucidloom.limo.child.ffi.GuardedPage
import ai.lucidloom.limo.child.ffi.PIN
import ai.lucidloom.limo.child.ffi.TryKind
import ai.lucidloom.limo.child.ffi.app
import ai.lucidloom.limo.child.watch.Guarded
import ai.lucidloom.limo.child.watch.LAUNCHER
import ai.lucidloom.limo.child.watch.LIMO
import ai.lucidloom.limo.child.watch.Rig
import ai.lucidloom.limo.child.watch.TestPhone
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * The guard through the host, on the real engine: the watch says a guarded
 * page is in front and the guard goes to the shower at once; the PIN typed
 * on it, wrong and right, its secret wiped, kept before the answer, the
 * guard gone by the time the answer comes; logged by kind only.
 */
class HostGuardTest {
  @get:Rule val folder = TemporaryFolder()

  @Test
  fun aGuardedPageIsCoveredAtOnceAndTheGuardsPinOpensItThroughTheHost() {
    HostRig(folder.newFolder()).use { rig ->
      rig.pair()
      rig.front = app("com.android.settings", "Settings")
      rig.changed()
      assertEquals(CoverKind.NOTHING, rig.shower.last.kind)

      // The watch says the device admin's page is in front: the host ticks
      // at once, and the guard reaches the shower.
      rig.guarded = GuardedPage.DEVICE_ADMIN
      rig.changed()
      val cover = rig.shower.last
      assertEquals(CoverKind.GUARD, cover.kind)
      assertEquals(GuardedPage.DEVICE_ADMIN, cover.guard?.page)
      assertEquals("A guardian's PIN is needed here.", cover.guard?.title)
      assertTrue(rig.log.any { it.startsWith("note {\"note\":\"guarded_page_opened\"") && it.contains("\"page\":\"device_admin\"") })

      // A wrong PIN: wiped, kept before the answer, still guarded.
      val wrong = "000000".toByteArray()
      val writes = rig.store.writes
      val first = rig.core.tryGuardPin(wrong)
      assertTrue(wrong.all { it.toInt() == 0 })
      assertEquals(TryKind.WRONG, (first as WayOut.Done).value.kind)
      assertEquals("That PIN is not right. 4 more tries before a wait.", first.value.message)
      assertTrue(rig.store.writes > writes)
      assertEquals(CoverKind.GUARD, rig.shower.last.kind)

      // The right one: the guard is gone when the answer comes, and stays
      // gone while the page is in front, for five minutes.
      val right = PIN.toByteArray()
      val opened = rig.core.tryGuardPin(right)
      assertTrue(right.all { it.toInt() == 0 })
      assertEquals(TryKind.RELEASE_CONFIRMED, (opened as WayOut.Done).value.kind)
      assertTrue(opened.value.message.startsWith("These settings are open until "))
      assertEquals(CoverKind.NOTHING, rig.shower.last.kind)
      rig.pass(4 * 60)
      assertEquals(CoverKind.NOTHING, rig.shower.last.kind)
      rig.pass(60)
      assertEquals(CoverKind.GUARD, rig.shower.last.kind)

      // Logged by kind, never the PIN.
      assertTrue(rig.log.contains("way_out guard_pin answer=wrong"))
      assertTrue(rig.log.contains("way_out guard_pin answer=release_confirmed"))
      assertFalse(rig.log.any { it.contains(PIN) || it.contains("000000") })
    }
  }

  /**
   * The rule the guard keeps, through the real watch on a made up phone and
   * the real engine: a guardian opens a guarded page with the PIN and puts
   * the phone down; its screen goes off, which closes the opening; a child
   * picks it up, unlocks it, and the same page comes back with no window
   * event at all. The guard must be ordered again, however it was raised.
   */
  @Test
  fun aPageOpenedByThePinIsGuardedAgainWhenThePhoneComesBackFromItsScreenOff() {
    val adminPage = "com.android.settings.applications.specialaccess.deviceadmin.DeviceAdminAdd"
    val settings = Guarded.SETTINGS
    val failed = ArrayList<String>()
    for (how in listOf("page", "event", "package")) {
      HostRig(folder.newFolder()).use { host ->
        val phone = TestPhone().apply { admin = true }
        val watch = Rig(phone)
        watch.core.serviceConnected(true)
        host.watch = { watch.facts() }
        // The watch tells the host of a change, on the phone's own clocks.
        fun told(): CoverKind {
          host.wall = phone.wall / 1_000
          host.elapsedMs = phone.elapsed
          host.changed()
          return host.shower.last.kind
        }
        host.pair()

        watch.open(LAUNCHER)
        val pkg = when (how) {
          "page", "event" -> {
            watch.open(settings)
            assertEquals(CoverKind.NOTHING, told())
            phone.pass(10)
            phone.paused(settings)
            phone.pass(30)
            phone.resumed(settings)
            phone.pass(20)
            // The device admin's page by its name, or another maker's name, then Deactivate.
            val name = if (how == "page") adminPage else "org.example.maker.AdminPage"
            watch.core.windowEvent(settings, Guarded.page(settings, name))
            if (how == "event") {
              assertEquals(CoverKind.NOTHING, told())
              phone.pass(2_000)
              watch.core.adminOffAsked()
            }
            settings
          }
          else -> "com.samsung.accessibility".also { watch.open(it) }
        }
        assertEquals(how, CoverKind.GUARD, told())

        // The guard's row opens LiMo's own screen; the right PIN; back to the page, open.
        phone.pass(500)
        phone.paused(pkg)
        phone.pass(10)
        phone.resumed(LIMO)
        watch.core.limoScreen(true)
        assertEquals(how, CoverKind.NOTHING, told())
        phone.pass(4_000)
        host.wall = phone.wall / 1_000
        assertEquals(TryKind.RELEASE_CONFIRMED, (host.core.tryGuardPin(PIN.toByteArray()) as WayOut.Done).value.kind)
        phone.pass(1_500)
        watch.core.limoScreen(false)
        phone.paused(LIMO)
        phone.pass(10)
        phone.resumed(pkg)
        phone.pass(20)
        assertEquals(how, CoverKind.NOTHING, told())

        // Put down: the page pauses, the screen goes off (the opening closes), Android's lock screen.
        phone.pass(1_000)
        phone.paused(pkg)
        phone.screenOn = false
        phone.stopped(pkg)
        assertEquals(how, CoverKind.NOTHING, told())
        phone.pass(5_000)
        phone.keyguard = true
        told()

        // Picked up: the screen on, unlocked, the same page resumes and tells nothing.
        phone.pass(60_000)
        phone.screenOn = true
        told()
        phone.pass(3_000)
        phone.keyguard = false
        phone.pass(40)
        phone.resumed(pkg)
        phone.pass(20)
        val back = told()
        if (back != CoverKind.GUARD) failed += "$how: $back"
      }
    }
    assertTrue("not guarded again: $failed", failed.isEmpty())
  }

  @Test
  fun theStandInShowerLogsTheGuardByItsPageOnly() {
    val lines = ArrayList<String>()
    HostRig(folder.newFolder()).use { rig ->
      rig.pair()
      rig.core.shower = LoggingShower { lines += it }
      rig.front = app("com.android.settings", "Settings")
      rig.guarded = GuardedPage.ACCESSIBILITY
      rig.changed()
    }
    assertTrue(lines.toString(), lines.contains("cover now guard page=accessibility"))
  }
}
